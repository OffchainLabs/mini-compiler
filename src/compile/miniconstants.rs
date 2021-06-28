/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved
 */

//!Creates a fixed list of globally accessible constants.

use crate::compile::CompileError;
use crate::evm::{contract_path, AbiForContract};
use crate::uint256::Uint256;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ConstantsFile {
    pub arbos_version: u64,
    integer: BTreeMap<String, u64>,
    hex: BTreeMap<String, String>,
    contract_folder: String,
    contract: BTreeSet<String>,
}

///Creates a fixed list of globally accessible constants.
pub fn init_constant_table(
    constants_path: Option<&Path>,
) -> Result<HashMap<String, Uint256>, CompileError> {
    let mut ret = HashMap::new();

    let consts = if let Some(consts_file) = constants_path {
        let mut file = File::open(consts_file).map_err(|_| {
            CompileError::new(
                String::from("Compile error"),
                format!("Could not open constants file {:?}", consts_file),
                vec![],
            )
        })?;
        let mut consts_string = String::new();
        file.read_to_string(&mut consts_string).map_err(|_| {
            CompileError::new(
                String::from("Compile error"),
                format!("Could not read file {:?} to a string", consts_file),
                vec![],
            )
        })?;
        serde_json::from_str::<ConstantsFile>(&consts_string).map_err(|_| {
            CompileError::new(
                String::from("Compile error"),
                format!("Could not parse {:?} as constants file", consts_file),
                vec![],
            )
        })?
    } else {
        ConstantsFile::default()
    };

    for (s, i) in consts.integer {
        ret.insert(s, Uint256::from_u64(i));
    }

    for (s, u) in consts.hex {
        ret.insert(s.to_string(), Uint256::from_string_hex(&u).unwrap());
    }

    for builtin in consts.contract {
        let fcodes =
            func_codes_for_builtin_contract(&consts.contract_folder, &builtin).map_err(|e| {
                CompileError::new(
                    String::from("Compile error"),
                    format!("Error accessing builtin function {}: {}", builtin, e),
                    vec![],
                )
            })?;
        for (name, code) in fcodes {
            ret.insert(name, code);
        }

        let etopics = event_topics_for_builtin_contract(&consts.contract_folder, &builtin)
            .map_err(|e| {
                CompileError::new(
                    String::from("Compile error"),
                    format!("Error accessing builtin event {}: {}", builtin, e),
                    vec![],
                )
            })?;
        for (name, topic) in etopics {
            ret.insert(name, topic);
        }
    }

    ret.insert(
        "ArbosVersionNumber".to_string(),
        Uint256::from_u64(consts.arbos_version),
    );

    Ok(ret)
}

fn func_codes_for_builtin_contract(
    folder: &str,
    contract_name: &str,
) -> Result<Vec<(String, Uint256)>, ethabi::Error> {
    let cabi = AbiForContract::new_from_file(&contract_path(folder, contract_name))?;
    let mut ret = vec![];
    for (_, funcs) in &cabi.contract.functions {
        for func in funcs {
            let func_name = &func.name;
            ret.push((
                "funcCode_".to_owned() + contract_name + "_" + func_name,
                Uint256::from_bytes(&cabi.short_signature_for_function(func_name)?[..]),
            ))
        }
    }
    Ok(ret)
}

fn event_topics_for_builtin_contract(
    folder: &str,
    contract_name: &str,
) -> Result<Vec<(String, Uint256)>, ethabi::Error> {
    let cabi = AbiForContract::new_from_file(&contract_path(folder, contract_name))?;
    let mut ret = vec![];
    for (_, events) in &cabi.contract.events {
        for event in events {
            let event_name = &event.name;
            ret.push((
                "eventTopic_".to_owned() + contract_name + "_" + event_name,
                Uint256::from_bytes(&event.signature()[..]),
            ))
        }
    }
    Ok(ret)
}
