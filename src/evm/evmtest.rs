/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::evm::preinstalled_contracts::ArbosTest;
use crate::run::{load_from_file_and_env, Machine, RuntimeEnvironment};
use crate::uint256::Uint256;
use std::collections::HashMap;
use std::io;
use std::path::Path;

pub fn run_evm_tests(path: &Path, logfiles_path: Option<&Path>) -> io::Result<(u64, u64)> {
    let mut num_success = 0;
    let mut num_fail = 0;
    if path.is_dir() {
        for entry in path.read_dir()? {
            let entry = entry?;
            let (ns, nf) = run_evm_tests(&entry.path(), logfiles_path)?;
            num_success = num_success + ns;
            num_fail = num_fail + nf;
        }
    } else {
        let contents =
            std::fs::read_to_string(path.clone()).expect("Something went wrong reading the file");
        let json: serde_json::Value =
            serde_json::from_str(&contents).expect("JSON was not well-formatted");
        if !path.ends_with("gas0.json")
            && !path.ends_with("gas1.json")
            && !path.ends_with("origin.json")
            && !path.ends_with("gasprice.json")
            && !path.ends_with("push32AndSuicide.json")
        {
            // ignore tests that rely on detailed Eth gas accounting
            let result = run_one_test(json, &path, logfiles_path, path.to_str().unwrap());
            match result {
                Ok(()) => {
                    println!("..ok {}", path.to_str().unwrap());
                    num_success = num_success + 1;
                }
                Err(e) => {
                    println!("FAIL ({}) {}", e, path.to_str().unwrap());
                    num_fail = num_fail + 1;
                }
            }
        }
    }
    Ok((num_success, num_fail))
}

fn run_one_test(
    json: serde_json::Value,
    _path: &Path,
    logfiles_path: Option<&Path>,
    raw_filename: &str,
) -> Result<(), ethabi::Error> {
    if let serde_json::Value::Object(omap) = json {
        for (_, v) in omap {
            let (mut machine, arbos_test) = start_test(
                uint256_from_jval(&v["env"]["currentNumber"].to_string(), true),
                uint256_from_jval(&v["env"]["currentTimestamp"].to_string(), true),
            );
            match &v["pre"] {
                serde_json::Value::Null => {}
                serde_json::Value::Object(premap) => {
                    for (astr, adata) in premap {
                        let addr = uint256_from_jval(&astr, false);
                        let balance = match &adata["balance"] {
                            serde_json::Value::Null => Uint256::zero(),
                            v => uint256_from_jval(&v.to_string(), true),
                        };
                        let code = bytevec_from_jval(&adata["code"].to_string(), true);
                        let nonce = uint256_from_jval(&adata["nonce"].to_string(), true);
                        let storage = storage_from_jval(adata["storage"].clone());
                        arbos_test.install_account(
                            &mut machine,
                            addr,
                            balance,
                            nonce,
                            Some(code),
                            Some(serialize_storage(storage)),
                        )?;
                    }
                }
                _ => panic!(),
            }

            let addr_str = &v["exec"]["address"].to_string();
            let addr_str = &addr_str[1..(addr_str.len() - 1)];
            let data = bytevec_from_jval(&v["exec"]["data"].to_string(), true);
            let callvalue = match &v["exec"]["value"] {
                serde_json::Value::Null => Uint256::zero(),
                v => uint256_from_jval(&v.to_string(), true),
            };
            let caller_addr = uint256_from_jval(&v["exec"]["caller"].to_string(), true);
            let callee_addr = uint256_from_jval(addr_str, false);
            let _result = do_call(
                &arbos_test,
                &mut machine,
                caller_addr.clone(),
                callee_addr.clone(),
                &data,
                callvalue.clone(),
                logfiles_path,
                raw_filename,
            );

            match &v["post"] {
                serde_json::Value::Null => {
                    return Ok(());
                }
                serde_json::Value::Object(postmap) => {
                    for (astr, adata) in postmap {
                        let addr = uint256_from_jval(&astr, false);
                        let balance = match &adata["balance"] {
                            serde_json::Value::Null => Uint256::zero(),
                            v => uint256_from_jval(&v.to_string(), true),
                        };
                        let nonce = uint256_from_jval(&adata["nonce"].to_string(), true);
                        let storage = storage_from_jval(adata["storage"].clone());
                        let (actual_balance, actual_nonce, actual_storage) =
                            arbos_test.get_account_info(&mut machine, addr.clone())?;
                        let expected_balance = if addr == callee_addr {
                            balance.add(&callvalue)
                        } else {
                            balance
                        };
                        let expected_nonce = if addr == caller_addr {
                            nonce.add(&Uint256::one())
                        } else {
                            nonce
                        };
                        if actual_balance != expected_balance {
                            return Err(ethabi::Error::from(format!(
                                "address {} balance: expected {}, got {}",
                                addr.clone(),
                                expected_balance,
                                actual_balance,
                            )));
                        } else if actual_nonce != expected_nonce {
                            return Err(ethabi::Error::from(format!(
                                "address {} nonce: expected {}, got {}",
                                addr, expected_nonce, actual_nonce,
                            )));
                        } else if !compare_storage(&storage, &deserialize_storage(actual_storage)) {
                            return Err(ethabi::Error::from(format!(
                                "address {} storage mismatch",
                                addr
                            )));
                        }
                    }
                    return Ok(());
                }
                _ => {
                    panic!();
                }
            }
        }
    }
    Ok(())
}

fn bytevec_from_jval(s: &str, strip_quotes: bool) -> Vec<u8> {
    hex::decode(if strip_quotes {
        &s[3..(s.len() - 1)]
    } else {
        &s[2..]
    })
    .unwrap()
}

fn uint256_from_jval(s: &str, strip_quotes: bool) -> Uint256 {
    Uint256::from_bytes(&bytevec_from_jval(s, strip_quotes))
}

fn storage_from_jval(jval: serde_json::Value) -> HashMap<Uint256, Uint256> {
    let mut ret = HashMap::new();
    if jval != serde_json::Value::Null {
        if let serde_json::Value::Object(omap) = jval {
            for (k, v) in omap {
                let k_ui = Uint256::from_bytes(&hex::decode(&k[2..]).unwrap());
                let v_ui = uint256_from_jval(&v.to_string(), true);
                ret.insert(k_ui, v_ui);
            }
        } else {
            println!("{}", jval);
            panic!("can't parse incoming storage");
        }
    }
    ret
}

fn start_test(blocknum: Uint256, timestamp: Uint256) -> (Machine, ArbosTest) {
    let rt_env = RuntimeEnvironment::new_with_blocknum_timestamp(blocknum, timestamp, None, None);
    let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
    machine.start_at_zero(true);

    let arbos_test = ArbosTest::new(false);

    (machine, arbos_test)
}

fn do_call(
    arbos_test: &ArbosTest,
    machine: &mut Machine,
    caller_addr: Uint256,
    callee_addr: Uint256,
    calldata: &[u8],
    callvalue: Uint256,
    logfiles_path: Option<&Path>,
    raw_filename: &str,
) -> Result<(), ethabi::Error> {
    match arbos_test.call(
        machine,
        caller_addr,
        callee_addr,
        calldata.to_vec(),
        callvalue,
    ) {
        Ok(_result) => {
            if let Some(logs_path) = logfiles_path {
                let logfile_name = raw_filename.replace("/", "_").replace("_json", ".aoslog");
                let this_log_path = [logs_path.to_str().unwrap(), &logfile_name].concat();
                machine
                    .runtime_env
                    .recorder
                    .to_file(
                        Path::new(&this_log_path),
                        machine.get_total_gas_usage().to_u64().unwrap(),
                    )
                    .unwrap();
            }

            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn serialize_storage(st: HashMap<Uint256, Uint256>) -> Vec<u8> {
    let mut ret: Vec<u8> = vec![];
    for (k, v) in st {
        ret.extend(k.to_bytes_be());
        ret.extend(v.to_bytes_be());
    }
    ret
}

fn deserialize_storage(buf: Vec<u8>) -> HashMap<Uint256, Uint256> {
    let mut ret = HashMap::new();
    let mut offset = 0;
    while offset < buf.len() {
        let k = Uint256::from_bytes(&buf[offset..offset + 32]);
        let v = Uint256::from_bytes(&buf[offset + 32..offset + 64]);
        ret.insert(k, v);
        offset = offset + 64;
    }
    ret
}

fn compare_storage(
    expected: &HashMap<Uint256, Uint256>,
    actual: &HashMap<Uint256, Uint256>,
) -> bool {
    let zero = Uint256::zero();
    for (k, v) in expected {
        if !v.is_zero() {
            if actual.get(k) != Some(v) {
                println!(
                    "storage mismatch at {}: expected {}, got {}",
                    k,
                    v,
                    if let Some(vv) = actual.get(k) {
                        vv
                    } else {
                        &zero
                    }
                );
                return false;
            }
        }
    }
    for (k, v) in actual {
        if !v.is_zero() {
            if expected.get(k) != Some(v) {
                println!(
                    "storage mismatch at {}: expected {}, got {}",
                    k,
                    if let Some(vv) = expected.get(k) {
                        vv
                    } else {
                        &zero
                    },
                    v
                );
                return false;
            }
        }
    }

    true
}
