use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use serde::{Serialize, Deserialize};
use crate::stringtable;
use crate::mavm::Instruction;
use crate::link::{ExportedFunc, ImportedFunc};


lalrpop_mod!(pub mini); 


#[derive(Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub code: Vec<Instruction>,
    pub exported_funcs: Vec<ExportedFunc>,
    pub imported_funcs: Vec<ImportedFunc>,
}

impl CompiledProgram {
    pub fn new(
        code: Vec<Instruction>, 
        exported_funcs: Vec<ExportedFunc>, 
        imported_funcs: Vec<ImportedFunc>
    ) -> Self {
        CompiledProgram{ code, exported_funcs, imported_funcs }
    }

    pub fn relocate(self, int_offset: usize, ext_offset: usize, func_offset: usize) -> (Self, usize) {
        let mut relocated_code = Vec::new();
        let mut max_func_offset = 0;
        for insn in &self.code {
            let (relocated_insn, new_func_offset) = insn.clone().relocate(int_offset, ext_offset, func_offset);
            relocated_code.push(relocated_insn);
            if max_func_offset < new_func_offset {
                max_func_offset = new_func_offset;
            }
        }

        let mut relocated_exported_funcs = Vec::new();
        for exp_func in self.exported_funcs {
            let (relocated_exp_func, new_func_offset) = exp_func.relocate(int_offset, ext_offset, func_offset);
            relocated_exported_funcs.push(relocated_exp_func);
            if max_func_offset < new_func_offset {
                max_func_offset = new_func_offset;
            }
        }

        let mut relocated_imported_funcs = Vec::new();
        for imp_func in self.imported_funcs {
            relocated_imported_funcs.push(imp_func.relocate(int_offset, ext_offset));
        }

        (CompiledProgram::new(relocated_code, relocated_exported_funcs, relocated_imported_funcs), max_func_offset)
    }

    pub fn to_output(&self, output: &mut dyn io::Write, format: Option<&str>) {
		match format {
			Some("pretty") => {
				writeln!(output, "exported: {:?}", self.exported_funcs).unwrap();
				writeln!(output, "imported: {:?}", self.imported_funcs).unwrap();
				for (idx, insn) in self.code.iter().enumerate() {
					writeln!(output, "{:04}:  {}", idx, insn).unwrap();
				}
			}
			None |
			Some("json") => {
				match serde_json::to_string(self) {
					Ok(prog_str) => {
						writeln!(output, "{}", prog_str).unwrap();
					}
					Err(e) => {
						writeln!(output, "json serialization error: {:?}", e).unwrap();
					}
				}
			}
			Some("bincode") => {
				match bincode::serialize(self) {
					Ok(encoded) => {
						if let Err(e) = output.write_all(&encoded) {
							writeln!(output, "bincode write error: {:?}", e).unwrap();
					   }
					}
					Err(e) => {
						writeln!(output, "bincode serialization error: {:?}", e).unwrap();
					}
				}
			}
			Some(weird_value) => { writeln!(output, "invalid format: {}", weird_value).unwrap(); }
		} 
	}
}

pub fn compile_from_file<'a>(path: &Path, debug: bool) -> Result<CompiledProgram, CompileError<'a>> {
   let display = path.display();

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {:?}", display, why),
        Ok(file) => file,
    };

    let mut s = String::new();
    s = match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {:?}", display, why),
        Ok(_) => s,
    };

    let parse_result: Result<CompiledProgram, serde_json::Error> = serde_json::from_str(&s);
    match parse_result {
        Ok(compiled_prog) => Ok(compiled_prog),
        Err(_) => compile_from_source(&s, debug),  // json parsing failed, try to parse as source code
    }
}

pub fn compile_from_source<'a>(s: &str, debug: bool) -> Result<CompiledProgram, CompileError<'a>> {
    let mut string_table_1 = stringtable::StringTable::new();
    let res = mini::DeclsParser::new()
        .parse(&mut string_table_1, s)
        .unwrap();
    let mut checked_funcs = Vec::new();
    let res2 = crate::typecheck::typecheck_top_level_decls(&res, &mut checked_funcs, string_table_1);
    match res2 {
    	Ok((exported_funcs, imported_funcs, string_table)) => { 
            let mut code = Vec::new();
    		match crate::codegen::mavm_codegen(checked_funcs, &mut code, &string_table, &imported_funcs) {
                Ok(code_out) => {
                    if debug {
                        println!("========== after initial codegen ===========");
                        println!("Exported: {:?}", exported_funcs);
                        println!("Imported: {:?}", imported_funcs);
                        for (idx, insn) in code_out.iter().enumerate() {
                         println!("{:04}:  {}", idx, insn);
                        }
                    }
                    Ok(CompiledProgram::new(code_out.to_vec(), exported_funcs, imported_funcs))
                }
                Err(e) => Err(CompileError::new(e.reason)),
            }
        },
        Err(res3) => Err(CompileError::new(res3.reason)),
    }
} 

#[derive(Debug)]
pub struct CompileError<'a> {
    description: &'a str,
}

impl<'a> CompileError<'a> {
    pub fn new(description: &'a str) -> Self {
        CompileError{ description }
    }
}
