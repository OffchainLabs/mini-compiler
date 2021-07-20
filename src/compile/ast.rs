/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

//!Contains types and utilities for constructing the mini AST

use crate::compile::typecheck::{
    AbstractSyntaxTree, InliningMode, PropertiesList, TypeCheckedNode,
};
use crate::compile::{path_display, CompileError};
use crate::link::{value_from_field_list, Import, TUPLE_SIZE};
use crate::mavm::{Instruction, Value};
use crate::pos::Location;
use crate::stringtable::{StringId, StringTable};
use crate::uint256::Uint256;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Formatter;

///This is a map of the types at a given location, with the Vec<String> representing the module path
///and the usize representing the stringID of the type at that location.
pub type TypeTree = HashMap<(Vec<String>, usize), (Type, String)>;

///Debugging info serialized into mini executables, currently only contains a location.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugInfo {
    pub location: Option<Location>,
    pub attributes: Attributes,
}

///A list of properties that an AST node has.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attributes {
    ///Is true if the current node is a breakpoint, false otherwise.
    pub breakpoint: bool,
    pub inline: InliningMode,
    #[serde(skip)]
    ///Whether generated instructions should be printed to the console.
    pub codegen_print: bool,
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
    UseDecl(Import),
    ConstDecl,
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
    Union(Vec<Type>),
}

impl AbstractSyntaxTree for Type {
    fn child_nodes(&mut self) -> Vec<TypeCheckedNode> {
        match self {
            Type::Void
            | Type::Uint
            | Type::Int
            | Type::Bool
            | Type::Bytes32
            | Type::EthAddress
            | Type::Buffer
            | Type::Any
            | Type::Every
            | Type::Nominal(_, _) => vec![],
            Type::Tuple(types) | Type::Union(types) => {
                types.iter_mut().map(|t| TypeCheckedNode::Type(t)).collect()
            }
            Type::Array(tipe) | Type::FixedArray(tipe, _) | Type::Option(tipe) => {
                vec![TypeCheckedNode::Type(tipe)]
            }
            Type::Struct(fields) => fields
                .iter_mut()
                .map(|field| TypeCheckedNode::Type(&mut field.tipe))
                .collect(),
            Type::Func(_, args, ret) => vec![TypeCheckedNode::Type(ret)]
                .into_iter()
                .chain(args.iter_mut().map(|t| TypeCheckedNode::Type(t)))
                .collect(),
            Type::Map(key, value) => vec![TypeCheckedNode::Type(key), TypeCheckedNode::Type(value)],
        }
    }
    fn is_pure(&mut self) -> bool {
        true
    }
    fn display_string(&self, _string_table: &StringTable) -> String {
        self.display()
    }
}

impl Type {
    ///Gets the representation of a `Nominal` type, based on the types in `type_tree`, returns self
    /// if the type is not `Nominal`, or a `CompileError` if the type of `self` cannot be resolved in
    /// `type_tree`.
    pub fn get_representation(&self, type_tree: &TypeTree) -> Result<Self, CompileError> {
        let mut base_type = self.clone();
        while let Type::Nominal(path, id) = base_type.clone() {
            base_type = type_tree
                .get(&(path.clone(), id))
                .cloned()
                .ok_or(CompileError::new_type_error(
                    format!("No type at {:?}, {}", path, id),
                    vec![],
                ))?
                .0;
        }
        Ok(base_type)
    }

    ///Finds all nominal sub-types present under a type
    pub fn find_nominals(&self) -> Vec<usize> {
        match self {
            Type::Nominal(_, id) => {
                vec![*id]
            }
            Type::Array(tipe) | Type::FixedArray(tipe, ..) | Type::Option(tipe) => {
                tipe.find_nominals()
            }
            Type::Tuple(entries) => {
                let mut tipes = vec![];
                for entry in entries {
                    tipes.extend(entry.find_nominals());
                }
                tipes
            }
            Type::Func(_, args, ret) => {
                let mut tipes = ret.find_nominals();
                for arg in args {
                    tipes.extend(arg.find_nominals());
                }
                tipes
            }
            Type::Struct(fields) => {
                let mut tipes = vec![];
                for field in fields {
                    tipes.extend(field.tipe.find_nominals());
                }
                tipes
            }

            Type::Map(domain_tipe, codomain_tipe) => {
                let mut tipes = domain_tipe.find_nominals();
                tipes.extend(codomain_tipe.find_nominals());
                tipes
            }
            _ => vec![],
        }
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

    pub fn covariant_castable(
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
            Type::Uint | Type::Int | Type::Bool | Type::Bytes32 | Type::EthAddress => match &rhs {
                Type::Uint | Type::Int | Type::Bool | Type::Bytes32 | Type::EthAddress => true,
                _ => false,
            },
            Type::Buffer | Type::Void | Type::Every => rhs == self,
            Type::Tuple(tvec) => {
                if let Ok(Type::Tuple(tvec2)) = rhs.get_representation(type_tree) {
                    type_vectors_covariant_castable(tvec, &tvec2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Array(t) => {
                if let Ok(Type::Array(t2)) = rhs.get_representation(type_tree) {
                    t.covariant_castable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::FixedArray(t, s) => {
                if let Ok(Type::FixedArray(t2, s2)) = rhs.get_representation(type_tree) {
                    (*s == s2) && t.covariant_castable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Struct(fields) => {
                if let Ok(Type::Struct(fields2)) = rhs.get_representation(type_tree) {
                    field_vectors_covariant_castable(fields, &fields2, type_tree, seen)
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
                        left.covariant_castable(&right, type_tree, seen)
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            Type::Func(_, args, ret) => {
                if let Type::Func(_, args2, ret2) = rhs {
                    //note: The order of arg2 and args, and ret and ret2 are in this order to ensure contravariance in function arg types
                    type_vectors_covariant_castable(args2, args, type_tree, seen.clone())
                        && (ret.covariant_castable(ret2, type_tree, seen))
                } else {
                    false
                }
            }
            Type::Map(key1, val1) => {
                if let Type::Map(key2, val2) = rhs {
                    if let Ok(val2) = val2.get_representation(type_tree) {
                        key1.covariant_castable(key2, type_tree, seen.clone())
                            && (val1.covariant_castable(&val2, type_tree, seen))
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Type::Option(_) => {
                if let Ok(Type::Option(_)) = rhs.get_representation(type_tree) {
                    true
                } else {
                    false
                }
            }
            Type::Union(inner) => {
                if let Ok(Type::Union(inner2)) = rhs.get_representation(type_tree) {
                    type_vectors_covariant_castable(&*inner2, inner, type_tree, seen.clone())
                } else {
                    false
                }
            }
        }
    }

    pub fn castable(
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
            Type::Uint | Type::Int | Type::Bytes32 => match &rhs {
                Type::Uint | Type::Int | Type::Bytes32 => true,
                _ => false,
            },
            Type::EthAddress => match &rhs {
                Type::Uint | Type::Int | Type::Bytes32 | Type::EthAddress => true,
                _ => false,
            },
            Type::Bool => match &rhs {
                Type::Uint | Type::Int | Type::Bool | Type::Bytes32 | Type::EthAddress => true,
                _ => false,
            },
            Type::Buffer | Type::Void | Type::Every => rhs == self,
            Type::Tuple(tvec) => {
                if let Ok(Type::Tuple(tvec2)) = rhs.get_representation(type_tree) {
                    type_vectors_castable(tvec, &tvec2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Array(t) => {
                if let Ok(Type::Array(t2)) = rhs.get_representation(type_tree) {
                    t.castable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::FixedArray(t, s) => {
                if let Ok(Type::FixedArray(t2, s2)) = rhs.get_representation(type_tree) {
                    (*s == s2) && t.castable(&t2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Struct(fields) => {
                if let Ok(Type::Struct(fields2)) = rhs.get_representation(type_tree) {
                    field_vectors_castable(fields, &fields2, type_tree, seen)
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
                        left.castable(&right, type_tree, seen)
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
                        && type_vectors_castable(args2, args, type_tree, seen.clone())
                        && (ret.castable(ret2, type_tree, seen))
                } else {
                    false
                }
            }
            Type::Map(key1, val1) => {
                if let Type::Map(key2, val2) = rhs {
                    if let Ok(val2) = val2.get_representation(type_tree) {
                        key1.castable(key2, type_tree, seen.clone())
                            && (val1.castable(&val2, type_tree, seen))
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Type::Option(inner) => {
                if let Ok(Type::Option(inner2)) = rhs.get_representation(type_tree) {
                    inner.castable(&inner2, type_tree, seen)
                } else {
                    false
                }
            }
            Type::Union(inner) => {
                if let Ok(Type::Union(inner2)) = rhs.get_representation(type_tree) {
                    type_vectors_castable(&*inner2, inner, type_tree, seen.clone())
                } else {
                    false
                }
            }
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
            Type::Union(types) => {
                if let Ok(Type::Union(types2)) = rhs.get_representation(type_tree) {
                    type_vectors_assignable(types, &types2, type_tree, seen)
                } else {
                    false
                }
            }
        }
    }

    pub fn first_mismatch(
        &self,
        rhs: &Self,
        type_tree: &TypeTree,
        mut seen: HashSet<(Type, Type)>,
    ) -> Option<TypeMismatch> {
        if *rhs == Type::Every {
            return None;
        }
        match self {
            Type::Any => {
                if *rhs != Type::Void {
                    None
                } else {
                    Some(TypeMismatch::Type(Type::Any, Type::Void))
                }
            }
            Type::Void
            | Type::Uint
            | Type::Int
            | Type::Bool
            | Type::Bytes32
            | Type::EthAddress
            | Type::Buffer
            | Type::Every => {
                if self == rhs {
                    None
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Tuple(tvec) => {
                if let Ok(Type::Tuple(tvec2)) = rhs.get_representation(type_tree) {
                    for (index, (left, right)) in tvec.iter().zip(tvec2.iter()).enumerate() {
                        if let Some(inner) = left.first_mismatch(right, type_tree, seen.clone()) {
                            return Some(TypeMismatch::Tuple(index, Box::new(inner)));
                        }
                    }
                    if tvec.len() != tvec2.len() {
                        return Some(TypeMismatch::TupleLength(tvec.len(), tvec2.len()));
                    }
                    None
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Array(t) => {
                if let Ok(Type::Array(t2)) = rhs.get_representation(type_tree) {
                    t.first_mismatch(&t2, type_tree, seen)
                        .map(|mismatch| TypeMismatch::ArrayMismatch(Box::new(mismatch)))
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::FixedArray(t, s) => {
                if let Ok(Type::FixedArray(t2, s2)) = rhs.get_representation(type_tree) {
                    if let Some(inner) = t.first_mismatch(&t2, type_tree, seen) {
                        Some(TypeMismatch::ArrayMismatch(Box::new(inner)))
                    } else if *s != s2 {
                        Some(TypeMismatch::ArrayLength(*s, s2))
                    } else {
                        None
                    }
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Struct(fields) => {
                if let Ok(Type::Struct(fields2)) = rhs.get_representation(type_tree) {
                    field_vectors_mismatch(fields, &fields2, type_tree, seen)
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Nominal(_, _) => {
                match (
                    self.get_representation(type_tree),
                    rhs.get_representation(type_tree),
                ) {
                    (Ok(left), Ok(right)) => {
                        if seen.insert((self.clone(), rhs.clone())) {
                            left.first_mismatch(&right, type_tree, seen)
                        } else {
                            None
                        }
                    }
                    (Ok(_), Err(_)) => Some(TypeMismatch::UnresolvedRight(self.clone())),
                    (Err(_), Ok(_)) => Some(TypeMismatch::UnresolvedLeft(rhs.clone())),
                    (Err(_), Err(_)) => {
                        Some(TypeMismatch::UnresolvedBoth(self.clone(), rhs.clone()))
                    }
                }
            }
            Type::Func(is_impure, args, ret) => {
                if let Type::Func(is_impure2, args2, ret2) = rhs {
                    for (index, (left, right)) in args.iter().zip(args2.iter()).enumerate() {
                        if let Some(inner) = left.first_mismatch(right, type_tree, seen.clone()) {
                            return Some(TypeMismatch::FuncArg(index, Box::new(inner)));
                        }
                    }
                    if args.len() != args2.len() {
                        return Some(TypeMismatch::FuncArgLength(args.len(), args2.len()));
                    }
                    if let Some(inner) = ret.first_mismatch(ret2, type_tree, seen) {
                        return Some(TypeMismatch::FuncReturn(Box::new(inner)));
                    }
                    if !is_impure && *is_impure2 {
                        return Some(TypeMismatch::Purity);
                    }
                    None
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Map(key1, val1) => {
                if let Type::Map(key2, val2) = rhs {
                    if let Ok(val2) = val2.get_representation(type_tree) {
                        key1.first_mismatch(key2, type_tree, seen.clone())
                            .map(|mismatch| (true, mismatch))
                            .or_else(|| {
                                val1.first_mismatch(&val2, type_tree, seen)
                                    .map(|mismatch| (false, mismatch))
                            })
                            .map(|(is_key, mismatch)| TypeMismatch::Map {
                                is_key,
                                inner: Box::new(mismatch),
                            })
                    } else {
                        Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                    }
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Option(inner) => {
                if let Ok(Type::Option(inner2)) = rhs.get_representation(type_tree) {
                    inner
                        .first_mismatch(&inner2, type_tree, seen)
                        .map(|mismatch| TypeMismatch::Option(Box::new(mismatch)))
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
            Type::Union(types) => {
                if let Ok(Type::Union(types2)) = rhs.get_representation(type_tree) {
                    for (index, (left, right)) in types.iter().zip(types2.iter()).enumerate() {
                        if let Some(inner) = left.first_mismatch(right, type_tree, seen.clone()) {
                            return Some(TypeMismatch::Union(index, Box::new(inner)));
                        }
                    }
                    if types.len() != types2.len() {
                        return Some(TypeMismatch::UnionLength(types.len(), types2.len()));
                    }
                    None
                } else {
                    Some(TypeMismatch::Type(self.clone(), rhs.clone()))
                }
            }
        }
    }

    pub fn mismatch_string(&self, rhs: &Type, type_tree: &TypeTree) -> Option<String> {
        let (left, right) = (
            &self.get_representation(type_tree).ok()?,
            &rhs.get_representation(type_tree).ok()?,
        );
        self.first_mismatch(rhs, type_tree, HashSet::new())
            .map(|mismatch| {
                format!(
                    "{}{}",
                    {
                        //This will be a lot simpler to write in 1.53 when or-patterns syntax stabilizes
                        match left {
                            Type::Any
                            | Type::Void
                            | Type::Uint
                            | Type::Int
                            | Type::Bool
                            | Type::Bytes32
                            | Type::EthAddress
                            | Type::Buffer
                            | Type::Every => String::new(),
                            _ => match right {
                                Type::Any
                                | Type::Void
                                | Type::Uint
                                | Type::Int
                                | Type::Bool
                                | Type::Bytes32
                                | Type::EthAddress
                                | Type::Buffer
                                | Type::Every => String::new(),
                                _ => format!(
                                    "\nleft: {}\nright {}\nFirst mismatch: ",
                                    left.display(),
                                    right.display()
                                ),
                            },
                        }
                    },
                    mismatch
                )
            })
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
            Type::Union(_) => (Value::none(), false),
        }
    }

    pub fn display(&self) -> String {
        self.display_indented(0, "::", None, false, &TypeTree::new())
            .0
    }

    pub fn display_separator(
        &self,
        separator: &str,
        prefix: Option<&str>,
        include_pathname: bool,
        type_tree: &TypeTree,
    ) -> (String, HashSet<(Type, String)>) {
        self.display_indented(0, separator, prefix, include_pathname, type_tree)
    }

    fn display_indented(
        &self,
        indent_level: usize,
        separator: &str,
        prefix: Option<&str>,
        include_pathname: bool,
        type_tree: &TypeTree,
    ) -> (String, HashSet<(Type, String)>) {
        let mut type_set = HashSet::new();
        match self {
            Type::Void => ("void".to_string(), type_set),
            Type::Uint => ("uint".to_string(), type_set),
            Type::Int => ("int".to_string(), type_set),
            Type::Bool => ("bool".to_string(), type_set),
            Type::Bytes32 => ("bytes32".to_string(), type_set),
            Type::EthAddress => ("address".to_string(), type_set),
            Type::Buffer => ("buffer".to_string(), type_set),
            Type::Tuple(subtypes) => {
                let mut out = "(".to_string();
                for s in subtypes {
                    //This should be improved by removing the final trailing comma.
                    let (displayed, subtypes) = s.display_indented(
                        indent_level,
                        separator,
                        prefix,
                        include_pathname,
                        type_tree,
                    );
                    out.push_str(&(displayed + ", "));
                    type_set.extend(subtypes);
                }
                out.push(')');
                (out, type_set)
            }
            Type::Array(t) => {
                let (displayed, subtypes) = t.display_indented(
                    indent_level,
                    separator,
                    prefix,
                    include_pathname,
                    type_tree,
                );
                (format!("[]{}", displayed), subtypes)
            }
            Type::FixedArray(t, size) => {
                let (displayed, subtypes) = t.display_indented(
                    indent_level,
                    separator,
                    prefix,
                    include_pathname,
                    type_tree,
                );
                (format!("[{}]{}", size, displayed), subtypes)
            }
            Type::Struct(fields) => {
                let mut out = "struct {\n".to_string();
                for _ in 0..indent_level {
                    out.push_str("    ");
                }
                for field in fields {
                    //This should indent further when dealing with sub-structs
                    let (displayed, subtypes) = field.tipe.display_indented(
                        indent_level + 1,
                        separator,
                        prefix,
                        include_pathname,
                        type_tree,
                    );
                    out.push_str(&format!("    {}: {},\n", field.name, displayed));
                    for _ in 0..indent_level {
                        out.push_str("    ");
                    }
                    type_set.extend(subtypes);
                }
                out.push('}');
                (out, type_set)
            }
            Type::Nominal(path, id) => {
                let out = format!(
                    "{}{}{}",
                    prefix.unwrap_or(""),
                    if include_pathname {
                        path.iter()
                            .map(|name| name.clone() + "_")
                            .collect::<String>()
                    } else {
                        format!("")
                    },
                    type_tree
                        .get(&(path.clone(), *id))
                        .map(|(_, name)| name.clone())
                        .unwrap_or(format!(
                            "Failed to resolve type name: {}",
                            path_display(path)
                        ))
                );
                type_set.insert((
                    self.clone(),
                    type_tree
                        .get(&(path.clone(), *id))
                        .map(|d| d.1.clone())
                        .unwrap_or_else(|| "bad".to_string()),
                ));
                (out, type_set)
            }
            Type::Func(impure, args, ret) => {
                let mut out = String::new();
                if *impure {
                    out.push_str("impure ");
                }
                out.push_str("func(");
                for arg in args {
                    let (displayed, subtypes) = arg.display_indented(
                        indent_level,
                        separator,
                        prefix,
                        include_pathname,
                        type_tree,
                    );
                    out.push_str(&(displayed + ", "));
                    type_set.extend(subtypes)
                }
                out.push(')');
                if **ret != Type::Void {
                    let (displayed, subtypes) = ret.display_indented(
                        indent_level,
                        separator,
                        prefix,
                        include_pathname,
                        type_tree,
                    );
                    out.push_str(" -> ");
                    out.push_str(&displayed);
                    type_set.extend(subtypes);
                }
                (out, type_set)
            }
            Type::Map(key, val) => {
                let (key_display, key_subtypes) = key.display_indented(
                    indent_level,
                    separator,
                    prefix,
                    include_pathname,
                    type_tree,
                );
                type_set.extend(key_subtypes);
                let (val_display, val_subtypes) = val.display_indented(
                    indent_level,
                    separator,
                    prefix,
                    include_pathname,
                    type_tree,
                );
                type_set.extend(val_subtypes);
                (format!("map<{},{}>", key_display, val_display), type_set)
            }
            Type::Any => ("any".to_string(), type_set),
            Type::Every => ("every".to_string(), type_set),
            Type::Option(t) => {
                let (display, subtypes) = t.display_indented(
                    indent_level,
                    separator,
                    prefix,
                    include_pathname,
                    type_tree,
                );
                (format!("option<{}> ", display), subtypes)
            }
            Type::Union(types) => {
                let mut s = String::from("union<");
                let mut subtypes = HashSet::new();
                for tipe in types {
                    let (name, new_subtypes) = tipe.display_indented(
                        indent_level + 1,
                        separator,
                        prefix,
                        include_pathname,
                        type_tree,
                    );
                    s.push_str(&name);
                    s.push_str(", ");
                    subtypes.extend(new_subtypes);
                }
                s.push('>');
                (s, subtypes)
            }
        }
    }
}

pub fn type_vectors_covariant_castable(
    tvec1: &[Type],
    tvec2: &[Type],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1
            .iter()
            .zip(tvec2)
            .all(|(t1, t2)| t1.covariant_castable(t2, type_tree, seen.clone()))
}

pub fn type_vectors_castable(
    tvec1: &[Type],
    tvec2: &[Type],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1
            .iter()
            .zip(tvec2)
            .all(|(t1, t2)| t1.castable(t2, type_tree, seen.clone()))
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

fn field_vectors_covariant_castable(
    tvec1: &[StructField],
    tvec2: &[StructField],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1.iter().zip(tvec2).all(|(t1, t2)| {
            t1.tipe
                .covariant_castable(&t2.tipe, type_tree, seen.clone())
        })
}

fn field_vectors_castable(
    tvec1: &[StructField],
    tvec2: &[StructField],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> bool {
    tvec1.len() == tvec2.len()
        && tvec1
            .iter()
            .zip(tvec2)
            .all(|(t1, t2)| t1.tipe.castable(&t2.tipe, type_tree, seen.clone()))
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

pub fn field_vectors_mismatch(
    tvec1: &[StructField],
    tvec2: &[StructField],
    type_tree: &TypeTree,
    seen: HashSet<(Type, Type)>,
) -> Option<TypeMismatch> {
    for (t1, t2) in tvec1.iter().zip(tvec2.iter()) {
        if let Some(mismatch) = t1.tipe.first_mismatch(&t2.tipe, type_tree, seen.clone()) {
            return Some(TypeMismatch::FieldType(t1.name.clone(), Box::new(mismatch)));
        }
        if t1.name != t2.name {
            return Some(TypeMismatch::FieldName(t1.name.clone(), t2.name.clone()));
        }
    }
    if tvec1.len() != tvec2.len() {
        return Some(TypeMismatch::Length(tvec1.len(), tvec2.len()));
    }
    None
}

///Identical to `type_vectors_assignable` but using StructField slices as inputs and comparing their
/// inner types.
fn field_vectors_assignable(
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
            (Type::Union(x), Type::Union(y)) => type_vectors_equal(x, y),
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

#[derive(Debug)]
pub enum TypeMismatch {
    Type(Type, Type),
    FieldName(String, String),
    FieldType(String, Box<TypeMismatch>),
    UnresolvedRight(Type),
    UnresolvedLeft(Type),
    UnresolvedBoth(Type, Type),
    Length(usize, usize),
    ArrayMismatch(Box<TypeMismatch>),
    ArrayLength(usize, usize),
    Tuple(usize, Box<TypeMismatch>),
    TupleLength(usize, usize),
    FuncArg(usize, Box<TypeMismatch>),
    FuncArgLength(usize, usize),
    FuncReturn(Box<TypeMismatch>),
    Map {
        is_key: bool,
        inner: Box<TypeMismatch>,
    },
    Option(Box<TypeMismatch>),
    Union(usize, Box<TypeMismatch>),
    UnionLength(usize, usize),
    Purity,
}

impl fmt::Display for TypeMismatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TypeMismatch::Type(left, right) =>
                    format!("expected {} got {}", left.display(), right.display()),
                TypeMismatch::FieldType(name, problem) =>
                    format!("in field \"{}\": {}", name, problem),
                TypeMismatch::FieldName(left, right) =>
                    format!("expected field name \"{}\", got \"{}\"", left, right),
                TypeMismatch::UnresolvedRight(tipe) =>
                    format!("could not resolve right-hand type \"{}\"", tipe.display()),
                TypeMismatch::UnresolvedLeft(tipe) =>
                    format!("could not resolve left-hand type \"{}\"", tipe.display()),
                TypeMismatch::UnresolvedBoth(left, right) => format!(
                    "could not resolve both right hand type \"{}\" and left hand type\"{}\"",
                    left.display(),
                    right.display()
                ),
                TypeMismatch::Length(left, right) => format!(
                    "structs of different lengths: expected length {} got length {}",
                    left, right
                ),
                TypeMismatch::ArrayLength(left, right) => format!(
                    "arrays of different lengths: expected length {} got length {}",
                    left, right
                ),
                TypeMismatch::ArrayMismatch(mismatch) =>
                    format!("inner array type mismatch {}", mismatch),
                TypeMismatch::Tuple(index, mismatch) =>
                    format!("in tuple field {}: {}", index + 1, mismatch),
                TypeMismatch::TupleLength(left, right) => format!(
                    "tuples of different lengths: expected length {} got length {}",
                    left, right
                ),
                TypeMismatch::FuncArg(index, mismatch) =>
                    format!("in function argument {}: {}", index + 1, mismatch),
                TypeMismatch::FuncArgLength(left, right) => format!(
                    "left func has {} args but right func has {} args",
                    left, right
                ),
                TypeMismatch::FuncReturn(mismatch) =>
                    format!("in function return type: {}", mismatch),
                TypeMismatch::Map { is_key, inner } => format!(
                    "in map {}: {}",
                    if *is_key { "key" } else { "value" },
                    inner
                ),
                TypeMismatch::Option(mismatch) => format!("in inner option type: {}", mismatch),
                TypeMismatch::Union(index, mismatch) =>
                    format!("In type {} of union: {}", index + 1, mismatch),
                TypeMismatch::UnionLength(left, right) => format!(
                    "left func has {} args but right func has {} args",
                    left, right
                ),
                TypeMismatch::Purity => format!("assigning impure function to pure function"),
            }
        )
    }
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
    pub name_id: StringId,
    pub name: String,
    pub tipe: Type,
    pub location: Option<Location>,
}

impl GlobalVarDecl {
    pub fn new(name_id: StringId, name: String, tipe: Type, location: Option<Location>) -> Self {
        GlobalVarDecl {
            name_id,
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
        debug_info: DebugInfo,
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
            debug_info,
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
    ReturnVoid(),
    Return(Expr),
    Break(Option<Expr>, Option<String>),
    Expression(Expr),
    Let(MatchPattern, Expr),
    Assign(StringId, Expr),
    While(Expr, Vec<Statement>),
    Asm(Vec<Instruction>, Vec<Expr>),
    DebugPrint(Expr),
    Assert(Expr),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchPattern<T = ()> {
    pub(crate) kind: MatchPatternKind<MatchPattern<T>>,
    pub(crate) debug_info: DebugInfo,
    pub(crate) cached: T,
}

///Either a single identifier or a tuple of identifiers, used in mini let bindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchPatternKind<T> {
    Bind(StringId),
    Assign(StringId),
    Tuple(Vec<T>),
}

impl<T> MatchPattern<T> {
    pub fn new_bind(id: StringId, debug_info: DebugInfo, cached: T) -> Self {
        Self {
            kind: MatchPatternKind::Bind(id),
            debug_info,
            cached,
        }
    }
    pub fn new_assign(id: StringId, debug_info: DebugInfo, cached: T) -> Self {
        Self {
            kind: MatchPatternKind::Assign(id),
            debug_info,
            cached,
        }
    }
    pub fn new_tuple(id: Vec<MatchPattern<T>>, debug_info: DebugInfo, cached: T) -> Self {
        Self {
            kind: MatchPatternKind::Tuple(id),
            debug_info,
            cached,
        }
    }
    pub fn collect_identifiers(&self) -> Vec<(StringId, bool, DebugInfo)> {
        match &self.kind {
            MatchPatternKind::Bind(id) => vec![(*id, false, self.debug_info)],
            MatchPatternKind::Assign(id) => vec![(*id, true, self.debug_info)],
            MatchPatternKind::Tuple(pats) => pats
                .iter()
                .flat_map(|pat| pat.collect_identifiers())
                .collect(),
        }
    }
    pub fn display(&self, string_table: &StringTable) -> String {
        match &self.kind {
            MatchPatternKind::Bind(id) => string_table.name_from_id(*id).clone(),
            MatchPatternKind::Assign(id) => format!("*{}", string_table.name_from_id(*id)),
            MatchPatternKind::Tuple(pats) => {
                let mut s = String::from("(");
                for pat in pats {
                    s.push_str(&format!("{}, ", pat.display(string_table)))
                }
                s.pop();
                s.pop();
                s.push(')');
                s
            }
        }
    }
}

///An identifier or array index for left-hand-side substructure assignments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubData {
    Dot(StringId),
    ArrayOrMap(Expr),
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
    NewUnion(Vec<Type>, Box<Expr>),
    ArrayOrMapMod(Box<Expr>, Box<Expr>, Box<Expr>),
    StructMod(Box<Expr>, String, Box<Expr>),
    WeakCast(Box<Expr>, Type),
    Cast(Box<Expr>, Type),
    CovariantCast(Box<Expr>, Type),
    UnsafeCast(Box<Expr>, Type),
    Asm(Type, Vec<Instruction>, Vec<Expr>),
    Error,
    GetGas,
    SetGas(Box<Expr>),
    Try(Box<Expr>),
    If(Box<Expr>, CodeBlock, Option<CodeBlock>),
    IfLet(StringId, Box<Expr>, CodeBlock, Option<CodeBlock>),
    Loop(Vec<Statement>),
    UnionCast(Box<Expr>, Type),
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

impl UnaryOp {
    pub fn to_name(&self) -> String {
        format!(
            "{}",
            match self {
                UnaryOp::Minus => "unary minus",
                UnaryOp::BitwiseNeg => "~",
                UnaryOp::Not => "!",
                UnaryOp::Hash => "hash 1 item",
                UnaryOp::Len => "len",
                UnaryOp::ToUint => "uint cast",
                UnaryOp::ToInt => "int cast",
                UnaryOp::ToBytes32 => "bytes32 cast",
                UnaryOp::ToAddress => "address cast",
            }
        )
    }
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
    Hash,
    GetBuffer8,
    GetBuffer64,
    GetBuffer256,
}

impl BinaryOp {
    pub fn to_name(&self) -> String {
        format!(
            "{}",
            match self {
                BinaryOp::Plus => "+",
                BinaryOp::Minus => "-",
                BinaryOp::Times => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Sdiv => "signed /",
                BinaryOp::Smod => "signed %",
                BinaryOp::LessThan => "<",
                BinaryOp::GreaterThan => ">",
                BinaryOp::LessEq => "<=",
                BinaryOp::GreaterEq => ">=",
                BinaryOp::SLessThan => "< signed",
                BinaryOp::SGreaterThan => "> signed",
                BinaryOp::SLessEq => "<= signed",
                BinaryOp::SGreaterEq => ">= signed",
                BinaryOp::Equal => "==",
                BinaryOp::NotEqual => "!=",
                BinaryOp::BitwiseAnd => "&",
                BinaryOp::BitwiseOr => "|",
                BinaryOp::BitwiseXor => "^",
                BinaryOp::ShiftLeft => "<<",
                BinaryOp::ShiftRight => ">>",
                BinaryOp::Hash => "hash 2 items",
                BinaryOp::GetBuffer8 => "get buffer 8",
                BinaryOp::GetBuffer64 => "get buffer 64",
                BinaryOp::GetBuffer256 => "get buffer 256",
            }
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrinaryOp {
    SetBuffer8,
    SetBuffer64,
    SetBuffer256,
}

impl TrinaryOp {
    pub fn to_name(&self) -> String {
        format!(
            "{}",
            match self {
                TrinaryOp::SetBuffer8 => "set buffer 8",
                TrinaryOp::SetBuffer64 => "set buffer 64",
                TrinaryOp::SetBuffer256 => "set buffer 256",
            }
        )
    }
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
