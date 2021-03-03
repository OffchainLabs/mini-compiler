/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

//!Contains types and utilities for constructing the mini AST

use super::typecheck::{new_type_error, TypeError};
use crate::compile::typecheck::PropertiesList;
use crate::link::{value_from_field_list, TUPLE_SIZE};
use crate::mavm::{Instruction, Value};
use crate::pos::Location;
use crate::stringtable::StringId;
use crate::uint256::Uint256;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

///This is a map of the types at a given location, with the Vec<String> representing the module path
///and the usize representing the stringID of the type at that location.
pub type TypeTree = HashMap<(Vec<String>, usize), Type>;

///Debugging info serialized into mini executables, currently only contains a location.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugInfo {
    pub location: Option<Location>,
    pub attributes: Attributes,
}

///A list of properties that an AST node has, currently only contains breakpoints.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attributes {
    ///Is true if the current node is a breakpoint, false otherwise.
    pub breakpoint: bool,
    pub inline: bool,
}

impl DebugInfo {
    pub fn new(location: Option<Location>, attributes: Attributes) -> Self {
        DebugInfo {
            location,
            attributes,
        }
    }
}

impl From<Option<Location>> for DebugInfo {
    fn from(location: Option<Location>) -> Self {
        DebugInfo {
            location,
            attributes: Attributes::default(),
        }
    }
}

///A top level language declaration.  Represents any language construct that can be directly
/// embedded in a source file, and do not need to be contained in a function or other context.
#[derive(Debug, Clone)]
pub enum TopLevelDecl {
    TypeDecl(TypeDecl),
    FuncDecl(Func),
    VarDecl(GlobalVarDecl),
    UseDecl(Vec<String>, String),
}

///Type Declaration, contains the StringId corresponding to the type name, and the underlying Type.
#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: StringId,
    pub tipe: Type,
}

pub fn new_type_decl(name: StringId, tipe: Type) -> TypeDecl {
    TypeDecl { name, tipe }
}

///A type in the mini language.
#[derive(Debug, Clone, Eq, Serialize, Deserialize, Hash)]
pub enum Type {
    Void,
    Uint,
    Int,
    Bool,
    Bytes32,
    EthAddress,
    Buffer,
    Tuple(Vec<Type>),
    Array(Box<Type>),
    FixedArray(Box<Type>, usize),
    Struct(Vec<StructField>),
    Nominal(Vec<String>, StringId),
    Func(bool, Vec<Type>, Box<Type>),
    Map(Box<Type>, Box<Type>),
    Any,
    Every,
    Option(Box<Type>),
}

impl Type {
    ///Gets the representation of a `Nominal` type, based on the types in `type_tree`, returns self
    /// if the type is not `Nominal`, or a `TypeError` if the type of `self` cannot be resolved in
    /// `type_tree`.
    pub fn get_representation(&self, type_tree: &TypeTree) -> Result<Self, TypeError> {
        let mut base_type = self.clone();
        while let Type::Nominal(path, id) = base_type.clone() {
            base_type = type_tree
                .get(&(path.clone(), id))
                .cloned()
                .ok_or(new_type_error(
                    format!("No type at {:?}, {}", path, id),
                    None,
                ))?;
        }
        Ok(base_type)
    }

    ///If self is a Struct, and name is the StringID of a field of self, then returns Some(n), where
    /// n is the index of the field of self whose ID matches name.  Otherwise returns None.
    pub fn get_struct_slot_by_name(&self, name: String) -> Option<usize> {
        match self {
            Type::Struct(fields) => {
                for (i, field) in fields.iter().enumerate() {
                    if field.name == name {
                        return Some(i);
                    }
                }
                None
            }
            _ => None,
        }
    }

    ///Returns true if rhs is a subtype of self, and false otherwise
    pub fn assignable(
        &self,
        rhs: &Self,
        type_tree: &TypeTree,
        mut seen: HashSet<(Type, Type)>,
    ) -> bool {
        if *rhs == Type::Every {
            return true;
        }
        match self {
            Type::Any => *rhs != Type::Void,
            Type::Void
            | Type::Uint
            | Type::Int
            | Type::Bool
            | Type::Bytes32
            | Type::EthAddress
            | Type::Buffer
            | Type::Every => (self == rhs),
            Type::Tuple(tvec) => {
                if let Ok(Type::Tuple(tvec2)) = rhs.get_representation(type_tree) {
                    type_vectors_assignable(tvec, &tvec2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Array(t) => {
                if let Ok(Type::Array(t2)) = rhs.get_representation(type_tree) {
                    t.assignable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::FixedArray(t, s) => {
                if let Ok(Type::FixedArray(t2, s2)) = rhs.get_representation(type_tree) {
                    (*s == s2) && t.assignable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Struct(fields) => {
                if let Ok(Type::Struct(fields2)) = rhs.get_representation(type_tree) {
                    field_vectors_assignable(fields, &fields2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Nominal(_, _) => {
                if let (Ok(left), Ok(right)) = (
                    self.get_representation(type_tree),
                    rhs.get_representation(type_tree),
                ) {
                    if seen.insert((left.clone(), right.clone())) {
                        left.assignable(&right, type_tree, seen)
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            Type::Func(is_impure, args, ret) => {
                if let Type::Func(is_impure2, args2, ret2) = rhs {
                    //note: The order of arg2 and args, and ret and ret2 are in this order to ensure contravariance in function arg types
                    (*is_impure || !is_impure2)
                        && arg_vectors_assignable(args2, args, type_tree, seen.clone())
                        && (ret.assignable(ret2, type_tree, seen))
                } else {
                    false
                }
            }
            Type::Map(key1, val1) => {
                if let Type::Map(key2, val2) = rhs {
                    if let Ok(val2) = val2.get_representation(type_tree) {
                        key1.assignable(key2, type_tree, seen.clone())
                            && (val1.assignable(&val2, type_tree, seen))
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Type::Option(inner) => {
                if let Ok(Type::Option(inner2)) = rhs.get_representation(type_tree) {
                    inner.assignable(&inner2, type_tree, seen)
                } else {
                    false
                }
            }
        }
    }

    ///Returns a tuple containing `Type`s default value and a `bool` representing whether use of
    /// that default is type-safe.
    // TODO: have this resolve nominal types
    pub fn default_value(&self) -> (Value, bool) {
        match self {
            Type::Void => (Value::none(), false),
            Type::Buffer => (Value::new_buffer(vec![]), true),
            Type::Uint | Type::Int | Type::Bytes32 | Type::EthAddress | Type::Bool => {
                (Value::Int(Uint256::zero()), true)
            }
            Type::Tuple(tvec) => {
                let mut default_tup = Vec::new();
                let mut is_safe = true;
                for t in tvec {
                    let (def, safe) = t.default_value();
                    default_tup.push(def);
                    is_safe = is_safe && safe;
                }
                (Value::new_tuple(default_tup), is_safe)
            }
            Type::Array(t) => {
                let (def, safe) = t.default_value();
                (
                    Value::new_tuple(vec![
                        Value::Int(Uint256::one()),
                        Value::Int(Uint256::one()),
                        Value::new_tuple(vec![def]),
                    ]),
                    safe,
                )
            }
            Type::FixedArray(t, sz) => {
                let (default_val, safe) = t.default_value();
                let mut val = Value::new_tuple(vec![default_val; 8]);
                let mut chunk_size = 1;
                while chunk_size * TUPLE_SIZE < *sz {
                    val = Value::new_tuple(vec![val; 8]);
                    chunk_size *= 8;
                }
                (val, safe)
            }
            Type::Struct(fields) => {
                let mut vals = Vec::new();
                let mut is_safe = true;
                for field in fields {
                    let (val, safe) = field.tipe.default_value();
                    vals.push(val);
                    is_safe = is_safe && safe;
                }
                (value_from_field_list(vals), is_safe)
            }
            Type::Map(_, _) | Type::Func(_, _, _) | Type::Nominal(_, _) => (Value::none(), false),
            Type::Any => (Value::none(), true),
            Type::Every => (Value::none(), false),
            Type::Option(_) => (Value::new_tuple(vec![Value::Int(Uint256::zero())]), true),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Type::Void => "void".to_string(),
            Type::Uint => "uint".to_string(),
            Type::Int => "int".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Bytes32 => "bytes32".to_string(),
            Type::EthAddress => "address".to_string(),
            Type::Buffer => "buffer".to_string(),
            Type::Tuple(subtypes) => {
                let mut out = "(".to_string();
                for s in subtypes {
                    //This should be improved by removing the final trailing comma.
                    out.push_str(&(s.display() + ", "));
                }
                out.push(')');
                out
            }
            Type::Array(t) => format!("[]{}", t.display()),
            Type::FixedArray(t, size) => format!("[{}]{}", size, t.display()),
            Type::Struct(fields) => {
                let mut out = "struct {\n".to_string();
                for field in fields {
                    //This should indent further when dealing with sub-structs
                    out.push_str(&format!("    {}: {},\n", field.name, field.tipe.display()));
                }
                out.push('}');
                out
            }
            Type::Nominal(path, id) => {
                let mut out = String::new();
                for path_item in path {
                    out.push_str(&format!("{}::", path_item))
                }
                out.push_str(&format!("{}", id));
                out
            }
            Type::Func(impure, args, ret) => {
                let mut out = String::new();
                if *impure {
                    out.push_str("impure ");
                }
                out.push_str("func(");
                for arg in args {
                    out.push_str(&(arg.display() + ", "));
                }
                out.push(')');
                if **ret != Type::Void {
                    out.push_str(" -> ");
                    out.push_str(&ret.display());
                }
                out
            }
            Type::Map(key, val) => {
                format!("map<{},{}>", key.display(), val.display())
            }
            Type::Any => "any".to_string(),
            Type::Every => "every".to_string(),
            Type::Option(t) => format!("option<{}>", t.display()),
        }
    }
}

///Returns true if each type in tvec2 is a subtype of the type in tvec1 at the same index, and tvec1
/// and tvec2 have the same length.
pub fn type_vectors_assignable(
    tvec1: &[Type],
    tvec2: &[Type],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1
            .iter()
            .zip(tvec2)
            .all(|(t1, t2)| t1.assignable(t2, type_tree, seen.clone()))
}

///Identical to `type_vectors_assignable`
pub fn arg_vectors_assignable(
    tvec1: &[Type],
    tvec2: &[Type],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1
            .iter()
            .zip(tvec2)
            .all(|(t1, t2)| t1.assignable(t2, type_tree, seen.clone()))
}

///Identical to `type_vectors_assignable` but using StructField slices as inputs and comparing their
/// inner types.
pub fn field_vectors_assignable(
    tvec1: &[StructField],
    tvec2: &[StructField],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1.iter().zip(tvec2).all(|(t1, t2)| {
            t1.tipe.assignable(&t2.tipe, type_tree, seen.clone()) && t1.name == t2.name
        })
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Type::Void, Type::Void)
            | (Type::Uint, Type::Uint)
            | (Type::Int, Type::Int)
            | (Type::Bool, Type::Bool)
            | (Type::Bytes32, Type::Bytes32)
            | (Type::EthAddress, Type::EthAddress)
            | (Type::Any, Type::Any)
            | (Type::Buffer, Type::Buffer)
            | (Type::Every, Type::Every) => true,
            (Type::Tuple(v1), Type::Tuple(v2)) => type_vectors_equal(&v1, &v2),
            (Type::Array(a1), Type::Array(a2)) => *a1 == *a2,
            (Type::FixedArray(a1, s1), Type::FixedArray(a2, s2)) => (s1 == s2) && (*a1 == *a2),
            (Type::Struct(f1), Type::Struct(f2)) => struct_field_vectors_equal(&f1, &f2),
            (Type::Map(k1, v1), Type::Map(k2, v2)) => (*k1 == *k2) && (*v1 == *v2),
            (Type::Func(i1, a1, r1), Type::Func(i2, a2, r2)) => {
                (i1 == i2) && type_vectors_equal(&a1, &a2) && (*r1 == *r2)
            }
            (Type::Nominal(p1, id1), Type::Nominal(p2, id2)) => (p1, id1) == (p2, id2),
            (Type::Option(x), Type::Option(y)) => *x == *y,
            (_, _) => false,
        }
    }
}

///Returns true if the contents of the slices are equal
fn type_vectors_equal(v1: &[Type], v2: &[Type]) -> bool {
    v1 == v2
}

///Returns true if the contents of the slices are equal
fn struct_field_vectors_equal(f1: &[StructField], f2: &[StructField]) -> bool {
    f1 == f2
}

///Field of a struct, contains field name and underlying type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct StructField {
    pub name: String,
    pub tipe: Type,
}

impl StructField {
    pub fn new(name: String, tipe: Type) -> StructField {
        StructField { name, tipe }
    }
}

///Argument to a function, contains field name and underlying type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FuncArg {
    pub name: StringId,
    pub tipe: Type,
    pub debug_info: DebugInfo,
}

pub fn new_func_arg(name: StringId, tipe: Type, debug_info: DebugInfo) -> FuncArg {
    FuncArg {
        name,
        tipe,
        debug_info,
    }
}

///Represents a declaration of a global mini variable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalVarDecl {
    pub name: StringId,
    pub tipe: Type,
    pub location: Option<Location>,
}

impl GlobalVarDecl {
    pub fn new(name: StringId, tipe: Type, location: Option<Location>) -> Self {
        GlobalVarDecl {
            name,
            tipe,
            location,
        }
    }
}

///Represents whether the FuncDecl that contains it is public or private.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FuncDeclKind {
    Public,
    Private,
}

///Represents a top level function declaration.  The is_impure, args, and ret_type fields are
/// assumed to be derived from tipe, and this must be upheld by the user of this type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Func<T = Statement> {
    pub name: StringId,
    pub args: Vec<FuncArg>,
    pub ret_type: Type,
    pub code: Vec<T>,
    pub tipe: Type,
    pub kind: FuncDeclKind,
    pub debug_info: DebugInfo,
    pub properties: PropertiesList,
}

impl Func {
    pub fn new(
        name: StringId,
        is_impure: bool,
        args: Vec<FuncArg>,
        ret_type: Type,
        code: Vec<Statement>,
        exported: bool,
        location: Option<Location>,
    ) -> Self {
        let mut arg_types = Vec::new();
        let args_vec = args.to_vec();
        for arg in args.iter() {
            arg_types.push(arg.tipe.clone());
        }
        Func {
            name,
            args: args_vec,
            ret_type: ret_type.clone(),
            code,
            tipe: Type::Func(is_impure, arg_types, Box::new(ret_type)),
            kind: if exported {
                FuncDeclKind::Public
            } else {
                FuncDeclKind::Private
            },
            debug_info: DebugInfo::from(location),
            properties: PropertiesList { pure: !is_impure },
        }
    }
}

///A statement in the mini language with associated `DebugInfo` that has not yet been type checked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    pub debug_info: DebugInfo,
}

///A raw statement containing no debug information that has not yet been type checked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatementKind {
    Noop(),
    ReturnVoid(),
    Return(Expr),
    Break(Option<Expr>, Option<String>),
    Expression(Expr),
    Let(MatchPattern, Expr),
    Assign(StringId, Expr),
    While(Expr, Vec<Statement>),
    Asm(Vec<Instruction>, Vec<Expr>),
    DebugPrint(Expr),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchPattern<T = ()> {
    pub(crate) kind: MatchPatternKind<MatchPattern<T>>,
    pub(crate) cached: T,
}

///Either a single identifier or a tuple of identifiers, used in mini let bindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchPatternKind<T> {
    Simple(StringId),
    Tuple(Vec<T>),
}

impl<T> MatchPattern<T> {
    pub fn new_simple(id: StringId, cached: T) -> Self {
        Self {
            kind: MatchPatternKind::Simple(id),
            cached,
        }
    }
    pub fn new_tuple(id: Vec<MatchPattern<T>>, cached: T) -> Self {
        Self {
            kind: MatchPatternKind::Tuple(id),
            cached,
        }
    }
}

///Represents a constant mini value of type Option<T> for some type T.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptionConst {
    _Some(Box<Constant>),
    None(Type),
}

///Represents a mini constant value. This is different than `Value` as it encodes Options as distinct
/// from tuples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Constant {
    Uint(Uint256),
    Int(Uint256),
    Bool(bool),
    Option(OptionConst),
    Null,
}

impl OptionConst {
    ///Gets the type of the value
    pub(crate) fn type_of(&self) -> Type {
        Type::Option(Box::new(match self {
            OptionConst::_Some(c) => (*c).type_of(),
            OptionConst::None(t) => t.clone(),
        }))
    }

    ///Exracts the value from the Constant
    pub(crate) fn value(&self) -> Value {
        match self {
            OptionConst::_Some(c) => {
                Value::new_tuple(vec![Value::Int(Uint256::one()), c.clone().value()])
            }
            OptionConst::None(_) => Value::new_tuple(vec![Value::Int(Uint256::zero())]),
        }
    }
}

impl Constant {
    ///Gets the type of the value
    pub(crate) fn type_of(&self) -> Type {
        match self {
            Constant::Uint(_) => Type::Uint,
            Constant::Int(_) => Type::Int,
            Constant::Bool(_) => Type::Bool,
            Constant::Option(inner) => inner.type_of(),
            Constant::Null => Type::Void,
        }
    }

    ///Exracts the value from the Constant
    pub(crate) fn value(&self) -> Value {
        match self {
            Constant::Uint(ui) => Value::Int(ui.clone()),
            Constant::Int(i) => Value::Int(i.clone()),
            Constant::Bool(b) => Value::Int(Uint256::from_bool(*b)),
            Constant::Option(c) => c.value(),
            Constant::Null => Value::none(),
        }
    }
}

///A mini expression that has not yet been type checked with an associated `DebugInfo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub debug_info: DebugInfo,
}

///A mini expression that has not yet been type checked, contains no debug information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExprKind {
    UnaryOp(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Trinary(TrinaryOp, Box<Expr>, Box<Expr>, Box<Expr>),
    ShortcutOr(Box<Expr>, Box<Expr>),
    ShortcutAnd(Box<Expr>, Box<Expr>),
    VariableRef(StringId),
    TupleRef(Box<Expr>, Uint256),
    DotRef(Box<Expr>, String),
    Constant(Constant),
    OptionInitializer(Box<Expr>),
    FunctionCall(Box<Expr>, Vec<Expr>),
    CodeBlock(CodeBlock),
    ArrayOrMapRef(Box<Expr>, Box<Expr>),
    StructInitializer(Vec<FieldInitializer>),
    Tuple(Vec<Expr>),
    NewArray(Box<Expr>, Type),
    NewFixedArray(usize, Option<Box<Expr>>),
    NewMap(Type, Type),
    ArrayOrMapMod(Box<Expr>, Box<Expr>, Box<Expr>),
    StructMod(Box<Expr>, String, Box<Expr>),
    UnsafeCast(Box<Expr>, Type),
    Asm(Type, Vec<Instruction>, Vec<Expr>),
    Panic,
    Try(Box<Expr>),
    If(Box<Expr>, CodeBlock, Option<CodeBlock>),
    IfLet(StringId, Box<Expr>, CodeBlock, Option<CodeBlock>),
    Loop(Vec<Statement>),
    NewBuffer,
}

impl Expr {
    ///Returns an expression that applies unary operator op to e.
    pub fn new_unary(op: UnaryOp, e: Expr, loc: Option<Location>) -> Self {
        Self {
            kind: ExprKind::UnaryOp(op, Box::new(e)),
            debug_info: DebugInfo::from(loc),
        }
    }

    ///Returns an expression that applies binary operator op to e1 and e2.
    pub fn new_binary(op: BinaryOp, e1: Expr, e2: Expr, loc: Option<Location>) -> Self {
        Self {
            kind: ExprKind::Binary(op, Box::new(e1), Box::new(e2)),
            debug_info: DebugInfo::from(loc),
        }
    }

    ///Returns an expression that applies trinary operator op to e1, e2, and e3.
    pub fn new_trinary(op: TrinaryOp, e1: Expr, e2: Expr, e3: Expr, loc: Option<Location>) -> Self {
        Self {
            kind: ExprKind::Trinary(op, Box::new(e1), Box::new(e2), Box::new(e3)),
            debug_info: DebugInfo::from(loc),
        }
    }
}

///A mini unary operator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnaryOp {
    Minus,
    BitwiseNeg,
    Not,
    Hash,
    Len,
    ToUint,
    ToInt,
    ToBytes32,
    ToAddress,
}

///A mini binary operator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BinaryOp {
    Plus,
    Minus,
    Times,
    Div,
    Mod,
    Sdiv,
    Smod,
    LessThan,
    GreaterThan,
    LessEq,
    GreaterEq,
    SLessThan,
    SGreaterThan,
    SLessEq,
    SGreaterEq,
    Equal,
    NotEqual,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    _LogicalAnd,
    LogicalOr,
    Hash,
    GetBuffer8,
    GetBuffer64,
    GetBuffer256,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrinaryOp {
    SetBuffer8,
    SetBuffer64,
    SetBuffer256,
}

///Used in StructInitializer expressions to map expressions to fields of the struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldInitializer<T = Expr> {
    pub name: String,
    pub value: T,
}

impl<T> FieldInitializer<T> {
    pub fn new(name: String, value: T) -> Self {
        FieldInitializer { name, value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeBlock {
    pub body: Vec<Statement>,
    pub ret_expr: Option<Box<Expr>>,
}

impl CodeBlock {
    pub fn new(body: Vec<Statement>, ret_expr: Option<Box<Expr>>) -> Self {
        Self { body, ret_expr }
    }
}
