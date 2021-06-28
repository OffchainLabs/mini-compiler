/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved
 */

//! Contains utilities for compiling mini source code.

use crate::link::{link, postlink_compile, ExportedFunc, Import, ImportedFunc, LinkedProgram};
use crate::mavm::Instruction;
use crate::pos::{BytePos, Location};
use crate::stringtable::{StringId, StringTable};
use ast::Func;
use clap::Clap;
use lalrpop_util::lalrpop_mod;
use lalrpop_util::ParseError;
use mini::DeclsParser;
use miniconstants::init_constant_table;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::path::Path;
use typecheck::TypeCheckedFunc;

pub use ast::{DebugInfo, GlobalVarDecl, StructField, TopLevelDecl, Type, TypeTree};
pub use source::Lines;
use std::str::FromStr;
pub use typecheck::{AbstractSyntaxTree, InliningMode, TypeCheckedNode};

mod ast;
mod codegen;
pub mod miniconstants;
mod source;
mod typecheck;
lalrpop_mod!(mini);

///Command line options for compile subcommand.
#[derive(Clap, Debug, Default)]
pub struct CompileStruct {
    pub input: Vec<String>,
    #[clap(short, long)]
    pub debug_mode: bool,
    #[clap(short, long)]
    pub test_mode: bool,
    #[clap(short, long)]
    pub warnings_are_errors: bool,
    #[clap(short, long)]
    pub output: Option<String>,
    #[clap(short, long)]
    pub format: Option<String>,
    #[clap(short, long)]
    pub inline: Option<InliningHeuristic>,
    #[clap(short, long)]
    pub consts_file: Option<String>,
    #[clap(short, long)]
    pub must_use_global_consts: bool,
    #[clap(short, long)]
    pub release_build: bool,
}

#[derive(Clap, Debug)]
pub enum InliningHeuristic {
    All,
    None,
}

impl FromStr for InliningHeuristic {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(InliningHeuristic::All),
            "none" => Ok(InliningHeuristic::None),
            other => Err(format!("Unrecognized inlining heuristic: \"{}\"", other)),
        }
    }
}

///Represents the contents of a source file after parsing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Module {
    //TODO: Remove this field
    ///The list of imported functions imported through the old import/export system
    imported_funcs: Vec<ImportedFunc>,
    ///List of functions defined locally within the source file
    funcs: BTreeMap<StringId, Func>,
    ///Map from `StringId`s in this file to the `Type`s they represent.
    named_types: HashMap<StringId, Type>,
    ///List of constants used in this file.
    constants: HashSet<String>,
    ///List of global variables defined within this file.
    global_vars: Vec<GlobalVarDecl>,
    ///List of imported constructs within this file.
    imports: Vec<Import>,
    ///Map from `StringId`s to the names they derived from.
    string_table: StringTable,
    ///Map from `StringId`s to the types of the functions they represent.
    func_table: HashMap<StringId, Type>,
    ///The path to the module
    path: Vec<String>,
    ///The name of the module, this may be removed later.
    name: String,
}

///Represents the contents of a source file after type checking is done.
#[derive(Clone, Debug)]
struct TypeCheckedModule {
    /// Collection of functions defined locally within the source file that have been validated by
    /// typechecking
    checked_funcs: BTreeMap<StringId, TypeCheckedFunc>,
    ///Map from `StringId`s to the names they derived from.
    string_table: StringTable,
    ///The list of imported functions imported through the old import/export system
    imported_funcs: Vec<ImportedFunc>,
    ///The list of exported functions exported through the old import/export system
    exported_funcs: Vec<ExportedFunc>,
    ///Map from `StringId`s in this file to the `Type`s they represent.
    named_types: HashMap<StringId, Type>,
    ///List of constants used in this file.
    constants: HashSet<String>,
    ///List of global variables defined in this module.
    global_vars: Vec<GlobalVarDecl>,
    ///The list of imports declared via `use` statements.
    imports: Vec<Import>,
    ///The path to the module
    path: Vec<String>,
    ///The name of the module, this may be removed later.
    name: String,
}

impl CompileStruct {
    pub fn invoke(&self) -> Result<(LinkedProgram, ErrorSystem), ErrorSystem> {
        let mut error_system = ErrorSystem {
            errors: vec![],
            warnings: vec![],
            warnings_are_errors: self.warnings_are_errors,
            warn_color: match self.warnings_are_errors {
                true => CompileError::PINK,
                false => CompileError::YELLOW,
            },
            file_info_chart: BTreeMap::new(),
        };

        let mut compiled_progs = Vec::new();
        let mut file_info_chart = BTreeMap::new();

        for filename in &self.input {
            let path = Path::new(filename);
            let constants_path = match &self.consts_file {
                Some(path) => Some(Path::new(path)),
                None => None,
            };
            match compile_from_file(
                path,
                &mut file_info_chart,
                &self.inline,
                constants_path,
                self.must_use_global_consts,
                &mut error_system,
                self.release_build,
            ) {
                Ok(idk) => idk,
                Err(err) => {
                    error_system.errors.push(err);
                    error_system.file_info_chart = file_info_chart;
                    return Err(error_system);
                }
            }
            .into_iter()
            .for_each(|prog| {
                file_info_chart.extend(prog.file_info_chart.clone());
                compiled_progs.push(prog)
            });
        }
        let linked_prog = match link(&compiled_progs, self.test_mode, &mut error_system) {
            Ok(idk) => idk,
            Err(err) => {
                error_system.errors.push(err);
                error_system.file_info_chart = file_info_chart;
                return Err(error_system);
            }
        };

        //If this condition is true it means that __fixedLocationGlobal will not be at
        // index [0], but rather [0][0] or [0][0][0] etc
        if linked_prog.globals.len() >= 58 {
            panic!("Too many globals defined in program, location of first global is not correct")
        }

        let postlinked_prog = match postlink_compile(
            linked_prog,
            file_info_chart.clone(),
            &mut error_system,
            self.test_mode,
            self.debug_mode,
        ) {
            Ok(idk) => idk,
            Err(err) => {
                error_system.errors.push(err);
                error_system.file_info_chart = file_info_chart;
                return Err(error_system);
            }
        };

        error_system.file_info_chart = file_info_chart;

        if error_system.warnings.len() > 0 && error_system.warnings_are_errors {
            error_system.errors.push(CompileError::new(
                String::from("Compile Error"),
                String::from("Found warning with -w on"),
                vec![],
            ));
            Err(error_system)
        } else {
            Ok((postlinked_prog, error_system))
        }
    }
}

impl Module {
    fn new(
        imported_funcs: Vec<ImportedFunc>,
        funcs: BTreeMap<StringId, Func>,
        named_types: HashMap<usize, Type>,
        constants: HashSet<String>,
        global_vars: Vec<GlobalVarDecl>,
        imports: Vec<Import>,
        string_table: StringTable,
        func_table: HashMap<usize, Type>,
        path: Vec<String>,
        name: String,
    ) -> Self {
        Self {
            imported_funcs,
            funcs,
            named_types,
            constants,
            global_vars,
            imports,
            string_table,
            func_table,
            path,
            name,
        }
    }
}

impl TypeCheckedModule {
    fn new(
        checked_funcs: BTreeMap<StringId, TypeCheckedFunc>,
        string_table: StringTable,
        imported_funcs: Vec<ImportedFunc>,
        exported_funcs: Vec<ExportedFunc>,
        named_types: HashMap<usize, Type>,
        constants: HashSet<String>,
        global_vars: Vec<GlobalVarDecl>,
        imports: Vec<Import>,
        path: Vec<String>,
        name: String,
    ) -> Self {
        Self {
            checked_funcs,
            string_table,
            imported_funcs,
            exported_funcs,
            constants,
            named_types,
            global_vars,
            imports,
            path,
            name,
        }
    }
    ///Inlines functions in the AST by replacing function calls with `CodeBlock` expressions where
    /// appropriate
    fn inline(&mut self, heuristic: &InliningHeuristic) {
        let mut new_funcs = self.checked_funcs.clone();
        for (_id, func) in &mut new_funcs {
            func.inline(
                &self.checked_funcs.values().cloned().collect(),
                &self.imported_funcs,
                &self.string_table,
                heuristic,
            )
        }
        self.checked_funcs = new_funcs;
    }
    ///Propagates inherited attributes down top-level decls.
    fn propagate_attributes(&mut self) {
        for (_id, func) in &mut self.checked_funcs {
            let attributes = func.debug_info.attributes.clone();
            TypeCheckedNode::propagate_attributes(func.child_nodes(), &attributes);
        }
    }
    ///Creates callgraph that associates module functions to those that they call
    fn build_callgraph(
        &mut self,
    ) -> BTreeMap<StringId, (Vec<(StringId, Option<Import>)>, Location)> {
        let mut call_graph = BTreeMap::new();

        let mut import_ids = HashMap::<StringId, Import>::new();
        for import in &self.imports {
            if let Some(id) = import.id {
                import_ids.insert(id, import.clone());
            }
        }

        for (_, func) in &mut self.checked_funcs {
            let call_ids = Func::determine_funcs_used(func.child_nodes());
            let mut calls = Vec::<(StringId, Option<Import>)>::new();

            for id in call_ids {
                match import_ids.get(&id) {
                    None => calls.push((id, None)),
                    Some(import) => {
                        if import.path[0] != "core" && import.path[0] != "std" {
                            calls.push((id, Some(import.clone())));
                        }
                    }
                }
            }

            call_graph.insert(func.name, (calls, func.debug_info.location.unwrap()));
        }

        call_graph
    }
    ///Reasons about control flow and construct usage within the typechecked AST
    fn flowcheck(&mut self, error_system: &mut ErrorSystem) {
        let mut flow_warnings = vec![];

        let mut imports: BTreeMap<StringId, Import> = BTreeMap::new();

        for import in self.imports.iter() {
            let id = self.string_table.get_if_exists(&import.name).unwrap();

            if let Some(prior) = imports.get(&id) {
                flow_warnings.push(CompileError::new_warning(
                    String::from("Compile warning"),
                    format!(
                        "use statement {}{}{} is a duplicate",
                        error_system.warn_color,
                        import.name,
                        CompileError::RESET,
                    ),
                    prior
                        .location
                        .into_iter()
                        .chain(import.location.into_iter())
                        .collect(),
                ));
            }

            if import.path[0] != "core" {
                imports.insert(id, import.clone());
            }
        }

        for global in &self.global_vars {
            for nominal in &global.tipe.find_nominals() {
                imports.remove(nominal);
            }
        }

        for (_id, tipe) in &self.named_types {
            for nominal in &tipe.find_nominals() {
                imports.remove(nominal);
            }
        }

        for (_id, func) in &mut self.checked_funcs {
            flow_warnings.extend(func.flowcheck(
                &mut imports, // will remove from imports everything used
                &mut self.string_table,
                error_system,
            ));
        }

        for (_id, import) in imports {
            flow_warnings.push(CompileError::new_warning(
                String::from("Compile warning"),
                format!(
                    "use statement {}{}{} is unnecessary",
                    error_system.warn_color,
                    import.name,
                    CompileError::RESET,
                ),
                import.location.into_iter().collect(),
            ));
        }

        flow_warnings.sort_by(|a, b| {
            a.locations
                .last()
                .unwrap()
                .line
                .to_usize()
                .cmp(&b.locations.last().unwrap().line.to_usize())
        });

        error_system.warnings.extend(flow_warnings);
    }
}

///Represents a mini program or module that has been compiled and possibly linked, but has not had
/// post-link compilation steps applied. Is directly serialized to and from .mao files.
#[derive(Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    ///Instructions to be run to execute the program/module
    pub code: Vec<Instruction>,
    ///The list of exported functions exported through the old import/export system
    pub exported_funcs: Vec<ExportedFunc>,
    ///The list of imported functions imported through the old import/export system
    pub imported_funcs: Vec<ImportedFunc>,
    ///Highest ID used for any global in this program, used for linking
    pub globals: Vec<GlobalVarDecl>,
    ///Contains list of offsets of the various modules contained in this program
    pub source_file_map: Option<SourceFileMap>,
    ///Map from u64 hashes of file names to the `String`s, `Path`s, and `Content`s they originate from
    pub file_info_chart: HashMap<u64, FileInfo>,
    ///Tree of the types
    pub type_tree: TypeTree,
}

impl CompiledProgram {
    pub fn new(
        code: Vec<Instruction>,
        exported_funcs: Vec<ExportedFunc>,
        imported_funcs: Vec<ImportedFunc>,
        globals: Vec<GlobalVarDecl>,
        source_file_map: Option<SourceFileMap>,
        file_info_chart: HashMap<u64, FileInfo>,
        type_tree: TypeTree,
    ) -> Self {
        CompiledProgram {
            code,
            exported_funcs,
            imported_funcs,
            globals,
            source_file_map,
            file_info_chart,
            type_tree,
        }
    }

    ///Takes self by value and returns a tuple. The first value in the tuple is modified version of
    /// self with internal code references shifted forward by int_offset instructions, external code
    /// references offset by ext_offset instructions, function references shifted forward by
    /// func_offset, num_globals modified assuming the first *globals_offset* slots are occupied,
    /// and source_file_map replacing the existing source_file_map field.
    ///
    /// The second value of the tuple is the function offset after applying this operation.
    pub fn relocate(
        self,
        int_offset: usize,
        ext_offset: usize,
        func_offset: usize,
        globals_offset: Vec<GlobalVarDecl>,
        source_file_map: Option<SourceFileMap>,
    ) -> (Self, usize) {
        let mut relocated_code = Vec::new();
        let mut max_func_offset = func_offset;
        for insn in &self.code {
            let (relocated_insn, new_func_offset) =
                insn.clone()
                    .relocate(int_offset, ext_offset, func_offset, globals_offset.len());
            relocated_code.push(relocated_insn);
            if max_func_offset < new_func_offset {
                max_func_offset = new_func_offset;
            }
        }

        let mut relocated_exported_funcs = Vec::new();
        for exp_func in self.exported_funcs {
            let (relocated_exp_func, new_func_offset) =
                exp_func.relocate(int_offset, ext_offset, func_offset);
            relocated_exported_funcs.push(relocated_exp_func);
            if max_func_offset < new_func_offset {
                max_func_offset = new_func_offset;
            }
        }

        let mut relocated_imported_funcs = Vec::new();
        for imp_func in self.imported_funcs {
            relocated_imported_funcs.push(imp_func.relocate(int_offset, ext_offset));
        }

        (
            CompiledProgram::new(
                relocated_code,
                relocated_exported_funcs,
                relocated_imported_funcs,
                {
                    let mut new_vec = globals_offset.clone();
                    new_vec.append(&mut self.globals.clone());
                    new_vec
                },
                source_file_map,
                self.file_info_chart,
                self.type_tree,
            ),
            max_func_offset,
        )
    }

    ///Writes self to output in format "format".  Supported values are: "pretty", "json", or
    /// "bincode" if None is specified, json is used, and if an invalid format is specified this
    /// value appended by "invalid format: " will be written instead
    pub fn _to_output(&self, output: &mut dyn io::Write, format: Option<&str>) {
        match format {
            Some("pretty") => {
                writeln!(output, "exported: {:?}", self.exported_funcs).unwrap();
                writeln!(output, "imported: {:?}", self.imported_funcs).unwrap();
                for (idx, insn) in self.code.iter().enumerate() {
                    writeln!(output, "{:04}:  {}", idx, insn).unwrap();
                }
            }
            None | Some("json") => match serde_json::to_string(self) {
                Ok(prog_str) => {
                    writeln!(output, "{}", prog_str).unwrap();
                }
                Err(e) => {
                    writeln!(output, "json serialization error: {:?}", e).unwrap();
                }
            },
            Some("bincode") => match bincode::serialize(self) {
                Ok(encoded) => {
                    if let Err(e) = output.write_all(&encoded) {
                        writeln!(output, "bincode write error: {:?}", e).unwrap();
                    }
                }
                Err(e) => {
                    writeln!(output, "bincode serialization error: {:?}", e).unwrap();
                }
            },
            Some(weird_value) => {
                writeln!(output, "invalid format: {}", weird_value).unwrap();
            }
        }
    }
}

///Returns either a CompiledProgram generated from source code at path, otherwise returns a
/// CompileError.
///
/// The file_id specified will be used as the file_id in locations originating from this source
/// file, and if debug is set to true, then compiler internal debug information will be printed.
pub fn compile_from_file(
    path: &Path,
    file_info_chart: &mut BTreeMap<u64, FileInfo>,
    inline: &Option<InliningHeuristic>,
    constants_path: Option<&Path>,
    must_use_global_consts: bool,
    error_system: &mut ErrorSystem,
    release_build: bool,
) -> Result<Vec<CompiledProgram>, CompileError> {
    let library = path
        .parent()
        .map(|par| {
            par.file_name()
                .map(|lib| {
                    let res = lib.to_str();
                    if res == Some("builtin") {
                        Some("core")
                    } else if res == Some("stdlib") {
                        Some("std")
                    } else {
                        None
                    }
                })
                .unwrap_or(None)
        })
        .unwrap_or(None);
    if path.is_dir() {
        compile_from_folder(
            path,
            library,
            "main",
            file_info_chart,
            inline,
            constants_path,
            must_use_global_consts,
            error_system,
            release_build,
        )
    } else if let (Some(parent), Some(file_name)) = (path.parent(), path.file_stem()) {
        compile_from_folder(
            parent,
            library,
            file_name.to_str().ok_or_else(|| {
                CompileError::new(
                    String::from("Compile error"),
                    format!("File name {:?} must be UTF-8", file_name),
                    vec![],
                )
            })?,
            file_info_chart,
            inline,
            constants_path,
            must_use_global_consts,
            error_system,
            release_build,
        )
    } else {
        Err(CompileError::new(
            String::from("Compile error"),
            format!(
                "Could not parse {}{}{} as valid path",
                CompileError::RED,
                path.display(),
                CompileError::RESET,
            ),
            vec![],
        ))
    }
}

///Prints the AST nodes with indentation representing their depth, currently not used.
fn _print_node(node: &mut TypeCheckedNode, state: &String, mut_state: &mut usize) -> bool {
    for _ in 0..*mut_state {
        print!("{}", state);
    }
    println!("{:?}", node);
    *mut_state += 1;
    true
}

///Compiles a `Vec<CompiledProgram>` from a folder or generates a `CompileError` if a problem is
///encountered during compilation.
///
///The `folder` argument gives the path to the folder, `library` optionally contains a library
///prefix attached to the front of all paths, `main` contains the name of the main file in the
///folder, `file_info_chart` contains a map from the `u64` hashes of file names to the `FileInfo`
///they represent, useful for formatting errors, and `inline` determines whether inlining is used
///when compiling this folder.
pub fn compile_from_folder(
    folder: &Path,
    library: Option<&str>,
    main: &str,
    file_info_chart: &mut BTreeMap<u64, FileInfo>,
    inline: &Option<InliningHeuristic>,
    constants_path: Option<&Path>,
    must_use_global_consts: bool,
    error_system: &mut ErrorSystem,
    release_build: bool,
) -> Result<Vec<CompiledProgram>, CompileError> {
    let (mut programs, import_map) = create_program_tree(
        folder,
        library,
        main,
        file_info_chart,
        constants_path,
        error_system,
    )?;

    resolve_imports(&mut programs, &import_map, error_system)?;

    //Conversion of programs from `HashMap` to `Vec` for typechecking
    let type_tree = create_type_tree(&programs);
    let mut modules = vec![programs
        .remove(&if let Some(lib) = library {
            vec![lib.to_string(), main.to_string()]
        } else {
            vec![main.to_string()]
        })
        .expect("no main")];
    modules.append(&mut {
        let mut out: Vec<_> = programs.values().cloned().collect();
        out.sort_by(|module1, module2| module2.name.cmp(&module1.name));
        out
    });
    let mut typechecked_modules =
        typecheck_programs(&type_tree, modules, file_info_chart, error_system)?;

    if must_use_global_consts {
        check_global_constants(&typechecked_modules, constants_path, error_system);
    }

    // Control flow analysis stage
    let mut program_callgraph = HashMap::new();
    for module in &mut typechecked_modules {
        module.flowcheck(error_system);
        program_callgraph.insert(module.path.clone(), module.build_callgraph());
    }
    consume_program_callgraph(program_callgraph, &mut typechecked_modules, error_system);

    // Inlining stage
    if let Some(cool) = inline {
        typechecked_modules
            .iter_mut()
            .for_each(|module| module.inline(cool));
    }

    for module in &mut typechecked_modules {
        module.propagate_attributes();
    }

    let progs = codegen_programs(
        typechecked_modules,
        file_info_chart,
        error_system,
        type_tree,
        folder,
        release_build,
    )?;
    Ok(progs)
}

///Converts the `Vec<String>` used to identify a path into a single formatted string
fn path_display(path: &Vec<String>) -> String {
    let mut s = "".to_string();
    if let Some(first) = path.get(0) {
        s.push_str(first);
    }
    for item in path.iter().skip(1) {
        s.push_str("::");
        s.push_str(item);
    }
    s
}

///Parsing stage of the compiler, creates a `HashMap` containing a list of modules and imports
/// generated by interpreting the contents of `folder` as source code. Returns a `CompileError` if
/// the contents of `folder` fail to parse.
fn create_program_tree(
    folder: &Path,
    library: Option<&str>,
    main: &str,
    file_info_chart: &mut BTreeMap<u64, FileInfo>,
    constants_path: Option<&Path>,
    error_system: &mut ErrorSystem,
) -> Result<
    (
        HashMap<Vec<String>, Module>,
        HashMap<Vec<String>, Vec<Import>>,
    ),
    CompileError,
> {
    let mut paths = if let Some(lib) = library {
        vec![vec![lib.to_owned(), main.to_owned()]]
    } else {
        vec![vec![main.to_owned()]]
    };

    let mut programs = HashMap::new();
    let mut import_map = HashMap::new();
    let mut seen_paths = HashSet::new();
    while let Some(path) = paths.pop() {
        if seen_paths.contains(&path) {
            continue;
        } else {
            seen_paths.insert(path.clone());
        }
        let name = if path.len() == 1 {
            path[0].clone()
        } else if path[0] == "std" {
            format!("../stdlib/{}", path[1])
        } else if path[0] == "core" {
            format!("../builtin/{}", path[1])
        } else {
            path[0].clone()
        } + ".mini";
        let mut file = File::open(folder.join(name.clone())).map_err(|why| {
            CompileError::new(
                String::from("Compile error"),
                format!("Can not open {}/{}: {:?}", folder.display(), name, why),
                vec![],
            )
        })?;

        let mut source = String::new();
        file.read_to_string(&mut source).map_err(|why| {
            CompileError::new(
                String::from("Compile error"),
                format!("Can not read {}/{}: {:?}", folder.display(), name, why),
                vec![],
            )
        })?;
        let mut file_hasher = DefaultHasher::new();
        path.hash(&mut file_hasher);
        let file_id = file_hasher.finish();

        file_info_chart.insert(
            file_id,
            FileInfo {
                name: path_display(&path),
                path: folder.join(name.clone()).display().to_string(),
                contents: source.split("\n").map(|x| x.to_string()).collect(),
            },
        );

        let mut string_table = StringTable::new();
        let mut used_constants = HashSet::new();
        let (imports, funcs, named_types, global_vars, hm) = typecheck::sort_top_level_decls(
            &parse_from_source(
                source,
                file_id,
                &path,
                &mut string_table,
                constants_path,
                &mut used_constants,
                error_system,
            )?,
            path.clone(),
        );
        paths.append(&mut imports.iter().map(|imp| imp.path.clone()).collect());
        import_map.insert(path.clone(), imports.clone());
        programs.insert(
            path.clone(),
            Module::new(
                vec![],
                funcs,
                named_types,
                used_constants,
                global_vars,
                imports,
                string_table,
                hm,
                path,
                name,
            ),
        );
    }
    Ok((programs, import_map))
}

fn resolve_imports(
    programs: &mut HashMap<Vec<String>, Module>,
    import_map: &HashMap<Vec<String>, Vec<Import>>,
    error_system: &mut ErrorSystem,
) -> Result<(), CompileError> {
    for (name, imports) in import_map {
        for import in imports {
            let import_path = import.path.clone();
            let (named_type, imp_func) = if let Some(program) = programs.get_mut(&import_path) {
                //Looks up info from target program
                let index = program.string_table.get(import.name.clone());
                let named_type = program.named_types.get(&index).cloned();
                let imp_func = program.func_table.get(&index).cloned();
                (named_type, imp_func)
            } else {
                return Err(CompileError::new(
                    String::from("Compile error: Internal error"),
                    format!(
                        "Can not find target file for import \"{}::{}\"",
                        import.path.get(0).cloned().unwrap_or_else(String::new),
                        import.name
                    ),
                    import.location.into_iter().collect(),
                ));
            };
            //Modifies origin program to include import
            let origin_program = programs.get_mut(name).ok_or_else(|| {
                CompileError::new(
                    String::from("Compile error: Internal error"),
                    format!(
                        "Can not find originating file for import \"{}::{}\"",
                        import.path.get(0).cloned().unwrap_or_else(String::new),
                        import.name
                    ),
                    import.location.into_iter().collect(),
                )
            })?;
            let index = origin_program.string_table.get(import.name.clone());
            if let Some(named_type) = named_type {
                origin_program.named_types.insert(index, named_type.clone());
            } else if let Some(imp_func) = imp_func {
                origin_program.func_table.insert(index, imp_func.clone());
                origin_program.imported_funcs.push(ImportedFunc::new(
                    origin_program.imported_funcs.len(),
                    index,
                    &origin_program.string_table,
                ));
            } else {
                error_system.warnings.push(CompileError::new_warning(
                    String::from("Compile warning"),
                    format!(
                        "import \"{}::{}\" does not correspond to a type or function",
                        import.path.get(0).cloned().unwrap_or_else(String::new),
                        import.name
                    ),
                    import.location.into_iter().collect(),
                ));
            }
        }
    }
    Ok(())
}

///Constructor for `TypeTree`
fn create_type_tree(program_tree: &HashMap<Vec<String>, Module>) -> TypeTree {
    program_tree
        .iter()
        .map(|(path, program)| {
            program
                .named_types
                .iter()
                .map(|(id, tipe)| {
                    (
                        (path.clone(), *id),
                        (
                            tipe.clone(),
                            program_tree
                                .get(path)
                                .map(|module| module.string_table.name_from_id(*id).clone())
                                .unwrap(),
                        ),
                    )
                })
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect()
}

fn typecheck_programs(
    type_tree: &TypeTree,
    modules: Vec<Module>,
    _file_info_chart: &mut BTreeMap<u64, FileInfo>,
    error_system: &mut ErrorSystem,
) -> Result<Vec<TypeCheckedModule>, CompileError> {
    let (typechecked_modules, module_warnings) = modules
        .into_par_iter()
        .map(
            |Module {
                 imported_funcs,
                 funcs,
                 named_types,
                 constants,
                 global_vars,
                 imports,
                 string_table,
                 func_table: hm,
                 path,
                 name,
             }| {
                let mut typecheck_warnings = vec![];
                let mut checked_funcs = BTreeMap::new();
                let (exported_funcs, global_vars, string_table) =
                    typecheck::typecheck_top_level_decls(
                        funcs,
                        &named_types,
                        global_vars,
                        &imports,
                        string_table,
                        hm,
                        &mut checked_funcs,
                        type_tree,
                    )?;

                checked_funcs.iter_mut().for_each(|(id, func)| {
                    let detected_purity = func.is_pure();
                    let declared_purity = func.properties.pure;

                    if detected_purity != declared_purity {
                        typecheck_warnings.push(CompileError::new_warning(
                            String::from("Compile warning"),
                            format!(
                                "func {}{}{} {}",
                                error_system.warn_color,
                                string_table.name_from_id(*id),
                                CompileError::RESET,
                                match declared_purity {
                                    true => "is impure but not marked impure",
                                    false => "is declared impure but does not contain impure code",
                                },
                            ),
                            func.debug_info.location.into_iter().collect(),
                        ));
                    }
                });
                Ok((
                    TypeCheckedModule::new(
                        checked_funcs,
                        string_table,
                        imported_funcs,
                        exported_funcs,
                        named_types,
                        constants,
                        global_vars,
                        imports,
                        path,
                        name,
                    ),
                    typecheck_warnings,
                ))
            },
        )
        .collect::<Result<(Vec<TypeCheckedModule>, Vec<Vec<CompileError>>), CompileError>>()?;

    error_system
        .warnings
        .extend(module_warnings.into_iter().flatten());

    Ok(typechecked_modules)
}

fn check_global_constants(
    modules: &Vec<TypeCheckedModule>,
    constants_path: Option<&Path>,
    error_system: &mut ErrorSystem,
) {
    let mut global_constants = init_constant_table(constants_path).unwrap();
    for module in modules {
        for constant in &module.constants {
            global_constants.remove(constant);
        }
    }

    for (constant, _) in global_constants {
        if !constant.starts_with('_') {
            error_system.warnings.push(CompileError::new_warning(
                String::from("Compile warning"),
                format!(
                    "global constant {}{}{} is never used",
                    error_system.warn_color,
                    constant,
                    CompileError::RESET,
                ),
                vec![],
            ));
        }
    }
}

///Walks the program callgraph function by function across module boundries,
/// deleting edges in each module's callgraph along the way to eliminate all reachable functions
fn callgraph_descend(
    func: StringId,
    module: &TypeCheckedModule,
    program_callgraph: &mut HashMap<
        Vec<String>,
        BTreeMap<StringId, (Vec<(StringId, Option<Import>)>, Location)>,
    >,
    paths_to_modules: &HashMap<Vec<String>, &TypeCheckedModule>,
) {
    if module.path[0] == "core" || module.path[0] == "std" {
        return;
    }

    let module_callgraph = program_callgraph.get_mut(&module.path).unwrap();
    let calls = match &module_callgraph.get(&func) {
        Some(calls) => calls.0.clone(),
        None => return,
    };

    module_callgraph.remove(&func);
    for (call, dest) in calls {
        match dest {
            None => callgraph_descend(call, module, program_callgraph, paths_to_modules),
            Some(import) => {
                // outbound function crosses module boundry, so we jump there
                let other_module = paths_to_modules.get(&import.path).unwrap();
                let other_func = other_module
                    .string_table
                    .get_if_exists(&import.name)
                    .unwrap();
                callgraph_descend(
                    other_func,
                    other_module,
                    program_callgraph,
                    paths_to_modules,
                );
            }
        }
    }
}

///Walks the callgraph, pruning and/or warning on any unused functions
fn consume_program_callgraph(
    mut program_callgraph: HashMap<
        Vec<String>,
        BTreeMap<StringId, (Vec<(StringId, Option<Import>)>, Location)>,
    >,
    modules: &mut Vec<TypeCheckedModule>,
    error_system: &mut ErrorSystem,
) {
    let mut paths_to_modules = HashMap::<_, &TypeCheckedModule>::new();
    let main_module = &modules[0];
    let main_func = *modules[0].checked_funcs.keys().collect::<Vec<_>>()[0]; //.name;
    for module in modules.iter() {
        paths_to_modules.insert(module.path.clone(), module);
    }

    if main_module.path[0] == "std" || main_module.path[0] == "core" {
        // the entry point is in the standard library,
        // so we shouldn't require that functions be used
        return;
    }

    callgraph_descend(
        main_func,
        main_module,
        &mut program_callgraph,
        &paths_to_modules,
    );

    // we transform to a mutable now, rather than before, since graph traversal would
    // otherwise create ownership conflicts
    let mut paths_to_modules = HashMap::<_, &mut TypeCheckedModule>::new();
    for module in modules.iter_mut() {
        paths_to_modules.insert(module.path.clone(), module);
    }

    for (path, module_callgraph) in program_callgraph {
        if path[0] == "core" || path[0] == "std" {
            continue;
        }

        for (func, data) in module_callgraph {
            let module = paths_to_modules.get_mut(&path).unwrap();
            let func_name = module.string_table.name_from_id(func);

            if !func_name.starts_with('_') {
                error_system.warnings.push(CompileError::new_warning(
                    String::from("Compile warning"),
                    format!(
                        "func {}{}{} is unreachable",
                        error_system.warn_color,
                        func_name,
                        CompileError::RESET,
                    ),
                    vec![data.1],
                ));
            }

            module.checked_funcs.remove(&func);
        }
    }
}

fn codegen_programs(
    typechecked_modules: Vec<TypeCheckedModule>,
    file_info_chart: &mut BTreeMap<u64, FileInfo>,
    error_system: &mut ErrorSystem,
    type_tree: TypeTree,
    folder: &Path,
    release_build: bool,
) -> Result<Vec<CompiledProgram>, CompileError> {
    let mut progs = vec![];
    for TypeCheckedModule {
        checked_funcs,
        string_table,
        imported_funcs,
        exported_funcs,
        named_types: _,
        constants: _,
        global_vars,
        imports: _,
        path: _,
        name,
    } in typechecked_modules
    {
        let code_out = codegen::mavm_codegen(
            checked_funcs,
            &string_table,
            &imported_funcs,
            &global_vars,
            file_info_chart,
            error_system,
            release_build,
        )
        .map_err(|e| {
            CompileError::new(
                String::from("Codegen error"),
                e.reason.to_string(),
                e.location.into_iter().collect(),
            )
        })?;
        progs.push(CompiledProgram::new(
            code_out.to_vec(),
            exported_funcs,
            imported_funcs,
            global_vars,
            Some(SourceFileMap::new(
                code_out.len(),
                folder.join(name.clone()).display().to_string(),
            )),
            HashMap::new(),
            type_tree.clone(),
        ))
    }
    Ok(progs)
}

pub fn comma_list(input: &[String]) -> String {
    let mut base = String::new();
    if input.len() > 0 {
        for object in input.iter().take(input.len() - 1) {
            base.push_str(object);
            base.push(',');
            base.push(' ');
        }
        base.push_str(&input[input.len() - 1]);
    }
    base
}

///Converts source string `source` into a series of `TopLevelDecl`s, uses identifiers from
/// `string_table` and records new ones in it as well.  The `file_id` argument is used to construct
/// file information for the location fields.
pub fn parse_from_source(
    source: String,
    file_id: u64,
    file_path: &[String],
    string_table: &mut StringTable,
    constants_path: Option<&Path>,
    used_constants: &mut HashSet<String>,
    error_system: &mut ErrorSystem,
) -> Result<Vec<TopLevelDecl>, CompileError> {
    let comment_re = regex::Regex::new(r"//.*").unwrap();
    let source = comment_re.replace_all(&source, "");
    let lines = Lines::new(source.bytes());
    let mut constants = init_constant_table(constants_path)?;
    let mut local_constants = HashMap::<String, Location>::new();
    let parsed = DeclsParser::new()
        .parse(
            string_table,
            &lines,
            file_id,
            file_path,
            &mut constants,
            &mut local_constants,
            used_constants,
            error_system,
            &source,
        )
        .map_err(|e| match e {
            ParseError::UnrecognizedToken {
                token: (offset, _tok, end),
                expected,
            } => CompileError::new(
                String::from("Compile error: unexpected token"),
                format!(
                    "{}, expected one of: {}",
                    &source[offset..end],
                    comma_list(&expected),
                ),
                vec![lines.location(BytePos::from(offset), file_id).unwrap()],
            ),
            ParseError::InvalidToken { location } => CompileError::new(
                String::from("Compile error"),
                format!("found invalid token"),
                lines
                    .location(location.into(), file_id)
                    .into_iter()
                    .collect(),
            ),
            ParseError::UnrecognizedEOF { location, expected } => CompileError::new(
                String::from("Compile error: unexpected end of file"),
                format!("expected one of: {}", comma_list(&expected)),
                lines
                    .location(location.into(), file_id)
                    .into_iter()
                    .collect(),
            ),
            ParseError::ExtraToken {
                token: (offset, _tok, end),
            } => CompileError::new(
                String::from("Compile error: extra token"),
                format!("{}", &source[offset..end],),
                vec![lines.location(BytePos::from(offset), file_id).unwrap()],
            ),
            ParseError::User { error } => CompileError::new(
                String::from("Internal error"),
                format!(
                    "This should be impossible under the new error system {}",
                    error
                ),
                vec![],
            ),
        })?;

    for (constant, loc) in local_constants {
        if !used_constants.contains(&constant) {
            error_system.warnings.push(CompileError::new_warning(
                String::from("Compile warning"),
                format!(
                    "Constant {}{}{} is never used",
                    error_system.warn_color,
                    constant,
                    CompileError::RESET,
                ),
                vec![loc],
            ));
        }
    }

    Ok(parsed)
}

///Represents any error encountered during compilation.
#[derive(Debug, Clone)]
pub struct CompileError {
    ///The error title
    pub title: String,
    ///What the error is.
    pub description: String,
    ///Where the error happened.
    pub locations: Vec<Location>,
    ///Whether the error should not stop compilation
    pub is_warning: bool,
}

impl Display for CompileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.description)
    }
}

impl CompileError {
    pub fn new(title: String, description: String, locations: Vec<Location>) -> Self {
        CompileError {
            title,
            description,
            locations,
            is_warning: false,
        }
    }

    pub fn new_warning(title: String, description: String, locations: Vec<Location>) -> Self {
        CompileError {
            title,
            description,
            locations,
            is_warning: true,
        }
    }

    pub fn new_type_error(description: String, locations: Vec<Location>) -> Self {
        CompileError {
            title: String::from("Typecheck Error"),
            description,
            locations,
            is_warning: false,
        }
    }

    const RED: &'static str = "\x1b[31;1m";
    const BLUE: &'static str = "\x1b[34;1m";
    const YELLOW: &'static str = "\x1b[33;1m";
    const PINK: &'static str = "\x1b[38;5;161;1m";
    const RESET: &'static str = "\x1b[0;0m";

    pub fn pretty_fmt(
        &self,
        file_info_chart: &BTreeMap<u64, FileInfo>,
        warnings_are_errors: bool,
    ) -> String {
        let blue = CompileError::BLUE;
        let reset = CompileError::RESET;

        let err_color = match self.is_warning {
            true => match warnings_are_errors {
                true => CompileError::PINK,
                false => CompileError::YELLOW,
            },
            false => CompileError::RED,
        };

        let last_line = &self.locations.last();

        let mut pretty = format!(
            "{}{}{}: {}\n{}    --> {}{}\n",
            err_color,
            &self.title,
            reset,
            self.description,
            blue,
            match last_line {
                None => String::from("Could not determine location of error"),
                Some(location) => match file_info_chart.get(&location.file_id) {
                    None => String::from("file with id ") + &location.file_id.to_string(),
                    Some(info) => format!(
                        "{}{} line {}{}{} column {}{}",
                        info.path, reset, blue, location.line, reset, blue, location.column,
                    ),
                },
            },
            reset
        );

        pretty += &self
            .locations
            .iter()
            .map(|location| match file_info_chart.get(&location.file_id) {
                None => String::new(),
                Some(info) => format!(
                    "     {}|\n{: <4} | {}{}\n",
                    blue,
                    location.line,
                    reset,
                    match info.contents.get(location.line.to_usize()) {
                        None => "could not recover line",
                        Some(line) => line,
                    }
                ),
            })
            .collect::<String>();

        pretty += &self
            .locations
            .last()
            .into_iter()
            .map(|x| {
                format!(
                    "     {}|{}{:0space$}{}^{}\n",
                    blue,
                    reset,
                    " ",
                    err_color,
                    reset,
                    space = x.column.to_usize() + 1
                )
            })
            .collect::<String>();

        pretty
    }

    pub fn print(&self, file_info_chart: &BTreeMap<u64, FileInfo>, warnings_are_errors: bool) {
        eprintln!("{}", self.pretty_fmt(file_info_chart, warnings_are_errors));
    }
}

///A collection of all compiler warnings encountered and the mechanism to handle them.
pub struct ErrorSystem {
    ///All compilation errors
    pub errors: Vec<CompileError>,
    ///All compilation warnings
    pub warnings: Vec<CompileError>,
    ///Whether these should halt compilation
    pub warnings_are_errors: bool,
    ///The color to use when highlighting parts of the body text
    pub warn_color: &'static str,
    ///File information that helps the error system pretty-print errors and warnings
    pub file_info_chart: BTreeMap<u64, FileInfo>,
}

impl ErrorSystem {
    pub fn print(&self) {
        for warning in &self.warnings {
            warning.print(&self.file_info_chart, self.warnings_are_errors);
        }
        for error in &self.errors {
            error.print(&self.file_info_chart, self.warnings_are_errors);
        }
    }
}

///Lists the offset of each source file contained by a CompiledProgram in offsets, and the
/// instruction directly following the last in the CompiledProgram.
#[derive(Clone, Serialize, Deserialize)]
pub struct SourceFileMap {
    offsets: Vec<(usize, String)>,
    end: usize,
}

impl SourceFileMap {
    pub fn new(size: usize, first_filepath: String) -> Self {
        SourceFileMap {
            offsets: vec![(0, first_filepath)],
            end: size,
        }
    }

    pub fn new_empty() -> Self {
        SourceFileMap {
            offsets: Vec::new(),
            end: 0,
        }
    }

    ///Adds a new source file to self with offset at current end
    pub fn push(&mut self, size: usize, filepath: String) {
        self.offsets.push((self.end, filepath));
        self.end += size;
    }

    ///Panics if offset is past the end of the SourceFileMap
    pub fn get(&self, offset: usize) -> String {
        for i in 0..(self.offsets.len() - 1) {
            if offset < self.offsets[i + 1].0 {
                return self.offsets[i].1.clone();
            }
        }
        if offset < self.end {
            return self.offsets[self.offsets.len() - 1].1.clone();
        }
        panic!("SourceFileMap: bounds check error");
    }
}

///Lists the offset of each source file contained by a CompiledProgram in offsets, and the
/// instruction directly following the last in the CompiledProgram.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileInfo {
    pub name: String,
    #[serde(skip)]
    pub path: String,
    #[serde(skip)]
    pub contents: Vec<String>,
}

impl Debug for FileInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "FileInfo {{path: {:?}, name: {:?}, contents:...}}",
            self.path, self.name
        )
    }
}

impl Display for FileInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.name)
    }
}
