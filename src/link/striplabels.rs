/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

//!Provides utilities used in the `postlink_compile` function

use super::ImportedFunc;
use crate::compile::CompileError;
use crate::mavm::{AVMOpcode, CodePt, Instruction, Label, Opcode, Value};
use crate::uint256::Uint256;
use std::collections::{HashMap, HashSet};

///Replaces labels with code points in code_in, and in copies of jump_table, and exported_funcs. A
/// tuple of these modified values is returned if the function is successful, and the label causing
/// the error is returned otherwise.
///
/// The maybe_evm_pcs argument appends a list of PCs to the immediate of the first instruction, if
/// set to Some, this should only be done for modules.
pub fn strip_labels(
    code_in: Vec<Instruction>,
    jump_table: &[Label],
    imported_funcs: &[ImportedFunc],
) -> Result<(Vec<Instruction>, Vec<CodePt>), CompileError> {
    let mut label_map = HashMap::new();

    for imp_func in imported_funcs {
        let new_codept = CodePt::new_external(imp_func.slot_num);
        label_map.insert(Label::External(imp_func.slot_num), new_codept);
        label_map.insert(Label::Func(imp_func.name_id), new_codept);
    }

    let mut after_count = 0;
    for insn in &code_in {
        match insn.get_label() {
            Some(label) => {
                let old_value = label_map.insert(*label, CodePt::new_internal(after_count));
                if let Some(CodePt::Internal(x)) = old_value {
                    return Err(CompileError::new(
                        String::from("Compile error: Internal error"),
                        format!("Duplicate instance of internal label {:?}", x),
                        insn.debug_info.location.into_iter().collect(),
                    ));
                }
            }
            None => {
                after_count += 1;
            }
        }
    }

    let mut code_out = Vec::new();
    for insn in code_in {
        match insn.get_label() {
            Some(_) => {}
            None => {
                let insn_in = insn.clone();
                code_out.push(insn_in.replace_labels(&label_map)
                              .map_err(|lab| CompileError::new(
                                  String::from("Compile error"),
                                  format!("Couldn't find a definition for label {:?} contained in instruction {:?}, most likely reference to non-existent frunction", lab, insn),
                                  insn.debug_info.location.into_iter().collect(),
                              )
                    )?
                );
            }
        }
    }

    let mut jump_table_out = Vec::new();
    for jt_item in jump_table {
        match label_map.get(&jt_item) {
            Some(index) => {
                jump_table_out.push(*index);
            }
            None => {
                return Err(CompileError::new(
                    String::from("Compile error: strip_labels"),
                    format!("lookup failed for jump table item: {:?}", jt_item),
                    vec![],
                ));
            }
        }
    }

    Ok((code_out, jump_table_out))
}

///Replaces jumps to labels not moving the PC forward with a series of instructions emulating a
/// direct jump.
///
/// Returns the modified set of instructions and a vector of labels in order of appearance in the
/// code.
pub fn fix_nonforward_labels(
    code_in: &[Instruction],
    imported_funcs: &[ImportedFunc],
    jump_table_index_in_globals: usize,
) -> (Vec<Instruction>, Vec<Label>) {
    let mut jump_table = Vec::new();
    let mut jump_table_index = HashMap::new();
    let mut imm_labels_seen = HashSet::new();
    let mut code_out = Vec::new();

    let mut imported_func_set = HashSet::new();
    for imp_func in imported_funcs {
        let new_label = Label::External(imp_func.slot_num);
        imported_func_set.insert(new_label);
        /*
        jump_table_index.insert(new_label, jump_table.len());
        jump_table.push(new_label);
        */
    }

    for insn in code_in {
        let insn_in = insn.clone();
        match insn_in.immediate {
            Some(val) => match val {
                Value::Label(label) => {
                    if imm_labels_seen.contains(&label) || imported_func_set.contains(&label) {
                        let idx = match jump_table_index.get(&label) {
                            Some(index) => *index,
                            None => {
                                let index = jump_table.len();
                                jump_table_index.insert(label.clone(), index);
                                jump_table.push(label);
                                index
                            }
                        };
                        code_out.push(Instruction::from_opcode(
                            Opcode::GetGlobalVar(jump_table_index_in_globals),
                            insn_in.debug_info,
                        ));
                        code_out.push(Instruction::from_opcode(
                            Opcode::PushExternal(idx),
                            insn_in.debug_info,
                        ));
                        code_out.push(Instruction::from_opcode(insn_in.opcode, insn_in.debug_info));
                    } else {
                        code_out.push(Instruction::from_opcode_imm(
                            insn_in.opcode,
                            val,
                            insn_in.debug_info,
                        ));
                    }
                }
                _ => {
                    code_out.push(Instruction::from_opcode_imm(
                        insn_in.opcode,
                        val,
                        insn_in.debug_info,
                    ));
                }
            },
            None => {
                code_out.push(Instruction::from_opcode(insn_in.opcode, insn_in.debug_info));
            }
        }
        if let Opcode::Label(label) = insn_in.opcode {
            imm_labels_seen.insert(label);
        }
    }

    let mut code_xformed = Vec::new();
    for insn in code_out.iter() {
        match insn.opcode {
            Opcode::PushExternal(idx) => {
                if let Some(val) = &insn.immediate {
                    code_xformed.push(Instruction::from_opcode_imm(
                        Opcode::AVMOpcode(AVMOpcode::Noop),
                        val.clone(),
                        insn.debug_info,
                    ));
                }
                code_xformed.push(Instruction::from_opcode_imm(
                    Opcode::TupleGet(jump_table.len()),
                    Value::Int(Uint256::from_usize(idx)),
                    insn.debug_info,
                ));
            }
            _ => {
                code_xformed.push(insn.clone());
            }
        }
    }

    (code_xformed, jump_table)
}
