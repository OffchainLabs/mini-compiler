/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

//!Provides utilities for dealing with nested tuples and conversion from large flat tuples to
//! nested tuples

use crate::compile::{CompileError, DebugInfo};
use crate::mavm::{AVMOpcode, CodePt, Instruction, Opcode, Value};
use crate::uint256::Uint256;

///The maximum size of an AVM tuple
pub const TUPLE_SIZE: usize = 8;

///Takes a slice of instructions from a single function scope, and changes tuples of size greater
/// than TUPLE_SIZE to nested tuples with each subtuple at most TUPLE_SIZE
pub fn fix_tuple_size(
    code_in: &[Instruction],
    num_globals: usize,
) -> Result<Vec<Instruction>, CompileError> {
    let mut code_out = Vec::new();
    let mut locals_tree = TupleTree::new(1, true);
    let global_tree = TupleTree::new(num_globals, false);

    for insn in code_in.iter() {
        let debug_info = insn.debug_info;
        match insn.opcode {
            Opcode::MakeFrame(nargs, ntotal, return_address) => {
                if return_address {
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::AuxPush),
                        debug_info,
                    )); // move return address to aux stack
                }
                locals_tree = TupleTree::new(ntotal, true);
                if let Some(imm) = &insn.immediate {
                    code_out.push(Instruction::from_opcode_imm(
                        Opcode::AVMOpcode(AVMOpcode::Noop),
                        imm.clone(),
                        debug_info,
                    ));
                }
                code_out.push(Instruction::from_opcode_imm(
                    Opcode::AVMOpcode(AVMOpcode::AuxPush),
                    TupleTree::make_empty(&locals_tree),
                    debug_info,
                ));
                for lnum in 0..nargs {
                    locals_tree.write_code(true, lnum, &mut code_out, debug_info)?;
                }
            }
            Opcode::TupleGet(size) => {
                let ttree = TupleTree::new(size, false);
                if let Some(index) = &insn.immediate {
                    match index.to_usize() {
                        Some(iu) => {
                            ttree.read_code(false, iu, &mut code_out, debug_info)?;
                        }
                        None => {
                            return Err(CompileError::new(
                                String::from("Compile error: fix_tuple_size"),
                                "index too large".to_string(),
                                debug_info.location.into_iter().collect(),
                            ))
                        }
                    }
                } else {
                    return Err(CompileError::new(
                        String::from("Compile error: fix_tuple_size"),
                        "TupleGet without immediate arg".to_string(),
                        debug_info.location.into_iter().collect(),
                    ));
                }
            }
            Opcode::TupleSet(size) => {
                let ttree = TupleTree::new(size, false);
                if let Some(index) = &insn.immediate {
                    match index.to_usize() {
                        Some(iu) => {
                            ttree.write_code(false, iu, &mut code_out, debug_info)?;
                        }
                        None => {
                            return Err(CompileError::new(
                                String::from("Compile error: fix_tuple_size"),
                                "TupleSet index too large".to_string(),
                                debug_info.location.into_iter().collect(),
                            ))
                        }
                    }
                } else {
                    return Err(CompileError::new(
                        String::from("Compile error: fix_tuple_size"),
                        "TupleSet without immediate arg".to_string(),
                        debug_info.location.into_iter().collect(),
                    ));
                }
            }
            Opcode::SetLocal => {
                if let Some(index) = &insn.immediate {
                    match index.to_usize() {
                        Some(iu) => {
                            locals_tree.write_code(true, iu, &mut code_out, debug_info)?;
                        }
                        None => {
                            return Err(CompileError::new(
                                String::from("Compile error: fix_tuple_size"),
                                "index too large".to_string(),
                                debug_info.location.into_iter().collect(),
                            ))
                        }
                    }
                } else {
                    return Err(CompileError::new(
                        String::from("Compile error: fix_tuple_size"),
                        "SetLocal without immediate arg".to_string(),
                        debug_info.location.into_iter().collect(),
                    ));
                }
            }
            Opcode::GetLocal => {
                if let Some(index) = &insn.immediate {
                    match index.to_usize() {
                        Some(iu) => {
                            locals_tree.read_code(true, iu, &mut code_out, debug_info)?;
                        }
                        None => {
                            return Err(CompileError::new(
                                String::from("Compile error: fix_tuple_size"),
                                "index too large".to_string(),
                                debug_info.location.into_iter().collect(),
                            ))
                        }
                    }
                } else {
                    return Err(CompileError::new(
                        String::from("Compile error: fix_tuple_size"),
                        "GetLocal without immediate arg".to_string(),
                        debug_info.location.into_iter().collect(),
                    ));
                }
            }
            Opcode::SetGlobalVar(idx) => {
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Rpush),
                    debug_info,
                ));
                global_tree.write_code(false, idx, &mut code_out, debug_info)?;
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Rset),
                    debug_info,
                ));
            }
            Opcode::GetGlobalVar(idx) => {
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Rpush),
                    debug_info,
                ));
                global_tree.read_code(false, idx, &mut code_out, debug_info)?;
            }
            Opcode::Return => {
                code_out.push(Instruction::new(
                    Opcode::AVMOpcode(AVMOpcode::AuxPop),
                    match &insn.immediate {
                        Some(v) => Some(v.clone()),
                        None => None,
                    },
                    debug_info,
                ));
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Pop),
                    debug_info,
                ));
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::AuxPop),
                    debug_info,
                ));
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Jump),
                    debug_info,
                ));
            }
            Opcode::UncheckedFixedArrayGet(sz) => {
                let tup_size_val = Value::Int(Uint256::from_usize(TUPLE_SIZE));
                let mut remaining_size = sz;
                while remaining_size > TUPLE_SIZE {
                    //TODO: can probably make this more efficient
                    // stack: idx arr
                    code_out.push(Instruction::from_opcode_imm(
                        Opcode::AVMOpcode(AVMOpcode::Dup1),
                        tup_size_val.clone(),
                        debug_info,
                    ));
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Mod),
                        debug_info,
                    ));
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Swap1),
                        debug_info,
                    ));
                    // stack: idx slot arr
                    code_out.push(Instruction::from_opcode_imm(
                        Opcode::AVMOpcode(AVMOpcode::Swap1),
                        tup_size_val.clone(),
                        debug_info,
                    ));
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Div),
                        debug_info,
                    ));
                    // stack: subindex slot arr
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Swap2),
                        debug_info,
                    ));
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Swap1),
                        debug_info,
                    ));
                    // stack: slot arr subindex
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Tget),
                        debug_info,
                    ));
                    code_out.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Swap1),
                        debug_info,
                    ));
                    // stack: subindex subarr
                    remaining_size = (remaining_size + (TUPLE_SIZE - 1)) / TUPLE_SIZE;
                }
                code_out.push(Instruction::from_opcode(
                    Opcode::AVMOpcode(AVMOpcode::Tget),
                    debug_info,
                ));
            }
            _ => {
                code_out.push(insn.clone());
            }
        }
    }

    Ok(code_out)
}

///Used for generating the static_val for a `LinkedProgram`.
///
/// Takes a vector of codepoints, and places them in order into a nested tuple `Value`
pub fn jump_table_to_value(jump_table: Vec<CodePt>) -> Value {
    let mut jump_table_codepoints = Vec::new();
    for pc in &jump_table {
        jump_table_codepoints.push(Value::CodePoint(*pc));
    }
    let shape = TupleTree::new(jump_table.len(), false);
    shape.make_value(jump_table_codepoints)
}

///Generates a `Value` that is a nested tuple with size total leaf values, all leaf values are null.
pub fn make_uninitialized_tuple(size: usize) -> Value {
    TupleTree::new(size, false).make_empty()
}

///Represents tuple structure of mini value.
#[derive(Debug)]
pub enum TupleTree {
    Single,
    Tree(usize, Vec<TupleTree>),
}

impl TupleTree {
    ///Constructs new `TupleTree` with capacity of size.
    ///
    /// The is_local argument indicates whether the `TupleTree` is intended to be used for locals,
    /// if set to false and size is 1 will return the Single variant, and will return the Tree
    /// variant otherwise.
    pub fn new(size: usize, is_local: bool) -> TupleTree {
        if (size == 1) && !is_local {
            return TupleTree::Single;
        }
        let mut current_size: usize = 1;
        while current_size * (TUPLE_SIZE * TUPLE_SIZE) <= size {
            current_size *= TUPLE_SIZE;
        }

        let mut v = Vec::new();
        let mut remaining_size = size;
        let mut remaining_slots = TUPLE_SIZE;

        while remaining_size > 0 {
            if current_size >= remaining_size {
                v.push(TupleTree::new(remaining_size, false));
                remaining_size = 0;
            } else if current_size * (1 + (remaining_slots - 1) * TUPLE_SIZE) >= remaining_size {
                v.push(TupleTree::new(current_size, false));
                remaining_size -= current_size;
                remaining_slots -= 1;
            } else {
                current_size *= TUPLE_SIZE;
            }
        }
        TupleTree::Tree(size, v)
    }

    ///Creates a `Value` with the same tree structure as self.
    pub fn make_empty(&self) -> Value {
        match self {
            TupleTree::Single => Value::new_tuple(Vec::new()),
            TupleTree::Tree(_, v) => {
                let mut tvec: Vec<Value> = Vec::new();
                for tt in v.iter() {
                    tvec.push(tt.make_empty());
                }
                Value::new_tuple(tvec)
            }
        }
    }

    ///Create a nested tuple `Value` with structure determined by self, and leaf nodes determined
    /// left to right by vals.
    fn make_value(&self, vals: Vec<Value>) -> Value {
        let (val, _) = self.make_value_2(vals);
        val
    }

    ///Internal call used by `make_value`.
    ///
    /// The returned `Value` is the value constructed from vals, and the returned `Vec<Values>` are
    /// the unconsumed `Value`s in vals.
    fn make_value_2(&self, vals: Vec<Value>) -> (Value, Vec<Value>) {
        match self {
            TupleTree::Single => (vals[0].clone(), vals[1..].to_vec()),
            TupleTree::Tree(_, subtrees) => {
                let mut ret = Vec::new();
                let mut vals = vals;
                for subtree in subtrees {
                    let (subval, new_vals) = subtree.make_value_2(vals);
                    ret.push(subval);
                    vals = new_vals;
                }
                (Value::new_tuple(ret), vals)
            }
        }
    }

    ///Gets the total number of nodes in the `TupleTree`
    fn tsize(&self) -> usize {
        match self {
            TupleTree::Single => 1,
            TupleTree::Tree(sz, _) => *sz,
        }
    }

    ///Generates code for pushing a copy the index-th element of self to the top of the stack.
    ///
    /// Argument is_local defines whether locals or globals are being accessed, generated code is
    /// appended to the code argument and also returned as an owned value, location is used to
    /// assign a code location for the generated instructions.
    fn read_code(
        &self,
        is_local: bool,
        index: usize,
        code: &mut Vec<Instruction>,
        debug_info: DebugInfo,
    ) -> Result<(), CompileError> {
        match self {
            TupleTree::Single => {
                if is_local {
                    code.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::AuxPop),
                        debug_info,
                    ));
                    code.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::Dup0),
                        debug_info,
                    ));
                    code.push(Instruction::from_opcode(
                        Opcode::AVMOpcode(AVMOpcode::AuxPush),
                        debug_info,
                    ));
                }
                Ok(())
            }
            TupleTree::Tree(_, v) => {
                let mut index = index;
                for (slot, subtree) in v.iter().enumerate() {
                    if index < subtree.tsize() {
                        code.push(Instruction::from_opcode_imm(
                            if is_local {
                                Opcode::AVMOpcode(AVMOpcode::Xget)
                            } else {
                                Opcode::AVMOpcode(AVMOpcode::Tget)
                            },
                            Value::Int(Uint256::from_usize(slot)),
                            debug_info,
                        ));
                        return subtree.read_code(false, index, code, debug_info);
                    } else {
                        index -= subtree.tsize();
                    }
                }
                Err(CompileError::new(
                    String::from("Compile error: TupleTree::read_code"),
                    "out-of-bounds read".to_string(),
                    debug_info.location.into_iter().collect(),
                ))
            }
        }
    }

    ///Generates code for writing to the index_in-th element of self.
    ///
    /// Argument is_local defines whether locals or globals are being accessed, generated code is
    /// appended to the code argument and also returned as an owned value, location is used to
    /// assign a code location for the generated instructions.
    fn write_code(
        &self,
        is_local: bool,
        index_in: usize,
        code: &mut Vec<Instruction>,
        debug_info: DebugInfo,
    ) -> Result<(), CompileError> {
        if let TupleTree::Tree(_, v) = self {
            let mut index = index_in;
            for (slot, subtree) in v.iter().enumerate() {
                if index < subtree.tsize() {
                    match *subtree {
                        TupleTree::Single => {
                            code.push(Instruction::from_opcode_imm(
                                if is_local {
                                    Opcode::AVMOpcode(AVMOpcode::Xset)
                                } else {
                                    Opcode::AVMOpcode(AVMOpcode::Tset)
                                },
                                Value::Int(Uint256::from_usize(slot)),
                                debug_info,
                            ));
                            return Ok(());
                        }
                        TupleTree::Tree(_, _) => {
                            if is_local {
                                code.push(Instruction::from_opcode_imm(
                                    Opcode::AVMOpcode(AVMOpcode::Xget),
                                    Value::Int(Uint256::from_usize(slot)),
                                    debug_info,
                                ));
                            } else {
                                code.push(Instruction::from_opcode(
                                    Opcode::AVMOpcode(AVMOpcode::Swap1),
                                    debug_info,
                                ));
                                code.push(Instruction::from_opcode(
                                    Opcode::AVMOpcode(AVMOpcode::Dup1),
                                    debug_info,
                                ));
                                code.push(Instruction::from_opcode_imm(
                                    Opcode::AVMOpcode(AVMOpcode::Tget),
                                    Value::Int(Uint256::from_usize(slot)),
                                    debug_info,
                                ));
                            }
                            subtree.write_code(false, index, code, debug_info)?;
                            if is_local {
                                code.push(Instruction::from_opcode_imm(
                                    Opcode::AVMOpcode(AVMOpcode::Xset),
                                    Value::Int(Uint256::from_usize(slot)),
                                    debug_info,
                                ));
                            } else {
                                code.push(Instruction::from_opcode(
                                    Opcode::AVMOpcode(AVMOpcode::Swap1),
                                    debug_info,
                                ));
                                code.push(Instruction::from_opcode_imm(
                                    Opcode::AVMOpcode(AVMOpcode::Tset),
                                    Value::Int(Uint256::from_usize(slot)),
                                    debug_info,
                                ));
                            }
                            return Ok(());
                        }
                    }
                } else {
                    index -= subtree.tsize();
                }
            }
            return Err(CompileError::new(
                String::from("Compile error: TupleTree::write_code"),
                "out-of-bounds write".to_string(),
                debug_info.location.into_iter().collect(),
            ));
        } else {
            code.push(Instruction::from_opcode(
                Opcode::AVMOpcode(AVMOpcode::Pop),
                debug_info,
            ));
            Ok(())
        }
    }
}

///Generates a `TupleTree` of size equal to the length of lis, and fills its leaf nodes with the
/// `Value`s in lis.
pub fn value_from_field_list(lis: Vec<Value>) -> Value {
    TupleTree::new(lis.len(), false).make_value(lis)
}
