/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::mavm::Value;
use crate::run::{_bytestack_from_bytes, load_from_file, run, run_from_file, Machine};
use crate::uint256::Uint256;
use num_bigint::{BigUint, RandBigInt};
use rlp::RlpStream;
use std::convert::TryInto;
use std::path::Path;

mod integration;

fn test_from_file_with_args_and_return(path: &Path, args: Vec<Value>, ret: Value) {
    let res = run_from_file(path, args, false);
    match res {
        Ok(res) => {
            assert_eq!(res[0], ret);
        }
        Err(e) => {
            panic!("{:?}", e);
        }
    }
}

fn test_from_file(path: &Path) {
    test_from_file_with_args_and_return(path, vec![], Value::Int(Uint256::zero()));
}

#[test]
fn test_arraytest() {
    test_from_file(Path::new("builtin/arraytest.mexe"));
}

#[test]
fn test_kvstest() {
    test_from_file(Path::new("builtin/kvstest.mexe"));
}

#[test]
fn test_storage_map() {
    test_from_file(Path::new("stdlib/storageMapTest.mexe"));
}

#[test]
fn test_queuetest() {
    test_from_file(Path::new("stdlib/queuetest.mexe"));
}

#[test]
fn test_globaltest() {
    test_from_file(Path::new("builtin/globaltest.mexe"));
}

#[test]
fn test_pqtest() {
    test_from_file(Path::new("stdlib/priorityqtest.mexe"));
}

#[test]
fn test_bytearray() {
    test_from_file(Path::new("stdlib/bytearraytest.mexe"));
}

#[test]
fn test_map() {
    test_from_file(Path::new("builtin/maptest.mexe"));
}

#[test]
fn test_keccak() {
    test_from_file(Path::new("stdlib/keccaktest.mexe"));
}

#[test]
fn test_bls() {
    test_from_file(Path::new("stdlib/blstest.mexe"));
}

#[test]
fn test_sha256() {
    test_from_file(Path::new("stdlib/sha256test.mexe"));
}

#[test]
fn test_fixedpoint() {
    test_from_file(Path::new("stdlib/fixedpointtest.mexe"));
}

#[test]
fn test_ripemd160() {
    test_from_file(Path::new("stdlib/ripemd160test.mexe"));
}

#[test]
fn test_biguint() {
    test_from_file(Path::new("stdlib/biguinttest.mexe"));
}

#[test]
fn test_rlp() {
    let mut ui = Uint256::one();
    for _i in 0..100 {
        test_rlp_uint(ui.clone(), ui.rlp_encode());
        let ui2 = ui.div(&Uint256::from_usize(2048)).unwrap(); // a valid address
        test_rlp_uint(ui2.clone(), ui2.rlp_encode());
        ui = ui
            .mul(&Uint256::from_usize(19482103))
            .add(&Uint256::from_usize(91));
    }
    let ui = Uint256::from_usize(4313412);
    test_rlp_uint(ui.clone(), ui.rlp_encode());

    let mut byte_testvecs = vec![
        vec![0u8],
        vec![13u8],
        vec![243u8],
        "Hello".as_bytes().to_vec(),
        "The quick brown fox jumped over the lazy dog."
            .as_bytes()
            .to_vec(),
        vec![77u8; 692],
    ];
    let mut new_test_vec: Vec<u8> = Vec::new();
    for i in 0u64..2198 {
        new_test_vec.push(((73 * i) % 256).try_into().unwrap());
    }
    byte_testvecs.push(new_test_vec.clone());
    for testvec in byte_testvecs {
        let res = rlp::encode(&testvec);
        test_rlp_bytearray(testvec, res);
    }

    let list3_testvecs = vec![
        (Uint256::zero(), vec![243u8], Uint256::one()),
        (
            Uint256::from_usize(9831498),
            vec![3u8],
            Uint256::from_usize(4313412),
        ),
        (
            Uint256::from_usize(9831498),
            new_test_vec,
            Uint256::from_usize(4313412),
        ),
    ];
    for testvec in list3_testvecs {
        let res = encode_list3(testvec.clone());
        test_rlp_list3(testvec, res);
    }
}

fn encode_list3(testvec: (Uint256, Vec<u8>, Uint256)) -> Vec<u8> {
    let mut stream = RlpStream::new_list(3);
    stream
        .append(&testvec.0.to_bytes_minimal())
        .append(&testvec.1)
        .append(&testvec.2.to_bytes_minimal());
    stream.out()
}

fn test_rlp_uint(ui: Uint256, correct_result: Vec<u8>) {
    test_from_file_with_args_and_return(
        Path::new("stdlib/rlptest.mexe"),
        vec![Value::Int(Uint256::zero()), Value::Int(ui)],
        _bytestack_from_bytes(&correct_result),
    );
}

fn test_rlp_bytearray(input: Vec<u8>, correct_result: Vec<u8>) {
    test_from_file_with_args_and_return(
        Path::new("stdlib/rlptest.mexe"),
        vec![Value::Int(Uint256::one()), _bytestack_from_bytes(&input)],
        _bytestack_from_bytes(&correct_result),
    );
}

fn test_rlp_list3(testvec: (Uint256, Vec<u8>, Uint256), correct_result: Vec<u8>) {
    test_from_file_with_args_and_return(
        Path::new("stdlib/rlptest.mexe"),
        vec![
            Value::Int(Uint256::from_usize(2)),
            Value::new_tuple(vec![
                Value::Int(testvec.0),
                _bytestack_from_bytes(&testvec.1),
                Value::Int(testvec.2),
            ]),
        ],
        _bytestack_from_bytes(&correct_result),
    );
}

#[test]
fn test_codeload() {
    test_from_file(Path::new("minitests/codeloadtest.mexe"));
}

#[test]
fn test_direct_deploy_add() {
    crate::evm::evm_direct_deploy_add(None, false);
}

#[test]
fn test_sha256_precompile() {
    crate::evm::evm_eval_sha256(None, false);
}

#[test]
fn test_ecpairing_precompile() {
    crate::evm::_evm_ecpairing_precompile(None, false);
}

#[test]
fn test_ripemd160_precompile() {
    crate::evm::_evm_eval_ripemd160(None, false);
}

/*  Disabled this test because the format it uses is no longer supported. Aggregator testing
    will still exercise this functionality.
#[test]
fn test_non_eip155_signed_tx() {
    crate::evm::evm_deploy_using_non_eip159_signature(None, false).unwrap();
}
*/

#[test]
fn test_direct_deploy_and_call_add() {
    let _log = crate::evm::evm_direct_deploy_and_call_add(None, false);
}

#[test]
fn test_call_from_contract() {
    let _log = crate::evm::_evm_test_contract_call(None, false);
}

#[test]
fn test_direct_deploy_and_compressed_call_add() {
    let _log = crate::evm::evm_direct_deploy_and_compressed_call_add(None, false);
}

#[test]
fn test_payment_in_constructor() {
    crate::evm::_evm_test_payment_in_constructor(None, false);
}

#[test]
fn test_block_num_consistency() {
    let _ = crate::evm::_evm_block_num_consistency_test(false).unwrap();
}

#[test]
fn test_arbsys() {
    let _log = crate::evm::evm_test_arbsys(None, false);
}

#[test]
fn test_arbsys_direct() {
    crate::evm::evm_test_arbsys_direct(None, false).unwrap();
}

#[test]
fn test_rate_control() {
    //FIXME crate::evm::_evm_test_rate_control(None, false).unwrap();
}

#[test]
fn test_function_table_access() {
    crate::evm::evm_test_function_table_access(None, false).unwrap();
}

#[test]
fn test_l2_to_l1_call() {
    crate::evm::_evm_test_callback(None, false).unwrap();
}

#[test]
fn test_evm_add_code() {
    crate::evm::_basic_evm_add_test(None, false).unwrap();
}

#[test]
pub fn test_crosscontract_call_with_constructors() {
    match crate::evm::evm_xcontract_call_with_constructors(None, false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

#[test]
pub fn test_tx_with_deposit() {
    match crate::evm::_evm_tx_with_deposit(None, false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

#[test]
pub fn test_create_opcode() {
    match crate::evm::evm_test_create(None, false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

#[test]
pub fn test_crosscontract_call_using_batch() {
    match crate::evm::evm_xcontract_call_using_batch(None, false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

pub fn _test_crosscontract_call_using_compressed_batch() {
    match crate::evm::_evm_xcontract_call_using_compressed_batch(None, false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

#[test]
fn test_payment_to_empty_address() {
    crate::evm::evm_payment_to_empty_address(None, false);
}

#[test]
fn test_underfunded_nested_call() {
    assert!(crate::evm::_underfunded_nested_call_test(None, false).is_ok());
}

fn test_call_to_precompile5(
    machine: &mut Machine,
    sender_addr: &Uint256,
    b: BigUint,
    e: BigUint,
    m: BigUint,
) -> Result<BigUint, ethabi::Error> {
    let b_bytes = b.to_bytes_be();
    let e_bytes = e.to_bytes_be();
    let m_bytes = m.to_bytes_be();
    let mut calldata = Uint256::from_usize(b_bytes.len()).to_bytes_be();
    calldata.extend(Uint256::from_usize(e_bytes.len()).to_bytes_be());
    calldata.extend(Uint256::from_usize(m_bytes.len()).to_bytes_be());
    calldata.extend(b_bytes);
    calldata.extend(e_bytes);
    calldata.extend(m_bytes);

    let txid = machine.runtime_env.insert_tx_message(
        sender_addr.clone(),
        Uint256::from_u64(1_000_000_000),
        None,
        Uint256::from_u64(5),
        Uint256::zero(),
        &calldata,
        false,
    );
    let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
    let _gas_used = machine.run(None);
    let logs = machine.runtime_env.get_all_receipt_logs();
    assert_eq!(logs.len(), num_logs_before + 1);
    let receipt = logs[num_logs_before].clone();

    assert_eq!(receipt.get_request_id(), txid);
    assert!(receipt.succeeded());

    Ok(BigUint::from_bytes_be(&receipt.get_return_data()))
}

#[test]
fn test_precompile5_small() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let my_addr = Uint256::from_usize(1025);

    match test_call_to_precompile5(
        &mut machine,
        &my_addr,
        BigUint::from(2u64),
        BigUint::from(4u64),
        BigUint::from(9u64),
    ) {
        Ok(bi) => {
            assert_eq!(bi, BigUint::from(7u64));
        }
        Err(e) => {
            panic!("{}", e);
        }
    }
}

#[test]
fn test_precompile5_big() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let my_addr = Uint256::from_usize(1025);

    let mut rng = rand::thread_rng();
    let b: BigUint = rng.gen_biguint(128);
    let e: BigUint = rng.gen_biguint(32);
    let m: BigUint = rng.gen_biguint(128);
    match test_call_to_precompile5(&mut machine, &my_addr, b.clone(), e.clone(), m.clone()) {
        Ok(actual) => {
            let expected = b.modpow(&e, &m);
            println!("actual   {}", actual);
            println!("expected {}", expected);
            assert_eq!(actual, expected);
        }
        Err(e) => {
            panic!("{}", e);
        }
    }
}

#[test]
fn reinterpret_register() {
    let mut old_machine = load_from_file(Path::new("upgradetests/regcopy_old.mexe"));
    let _ = run(&mut old_machine, vec![], false);
    let mut new_machine = load_from_file(Path::new("upgradetests/regcopy_new.mexe"));
    run(&mut new_machine, vec![old_machine.register], false).unwrap();
    assert_eq!(
        *new_machine.stack_top().unwrap(),
        Value::Int(Uint256::one())
    );
}

#[test]
fn small_upgrade() {
    use crate::upload::CodeUploader;
    let mut machine = load_from_file(Path::new("upgradetests/upgrade1_old.mexe"));
    let uploader = CodeUploader::_new_from_file(Path::new("upgradetests/upgrade1_new.mexe"));
    let code_bytes = uploader._to_flat_vec();
    let msg = Value::new_tuple(vec![
        Value::Int(Uint256::from_usize(code_bytes.len())),
        Value::new_buffer(code_bytes),
    ]);
    machine.runtime_env.insert_full_inbox_contents(vec![msg]);
    let _ = run(&mut machine, vec![], false);

    //let mut new_machine = load_from_file(Path::new("upgradetests/regcopy_new.mexe"), rt_env);
    //run(&mut new_machine, vec![machine.register], false).unwrap();
    println!("Machine state after: {:?}", machine.state);
    assert_eq!(
        *machine.stack_top().unwrap(),
        Value::Int(Uint256::from_u64(42))
    );
}

#[test]
fn small_upgrade_auto_remap() {
    use crate::upload::CodeUploader;

    let mut machine = load_from_file(Path::new("upgradetests/upgrade2_old.mexe"));
    let uploader = CodeUploader::_new_from_file(Path::new("upgradetests/upgrade2_new.mexe"));
    let code_bytes = uploader._to_flat_vec();
    let msg = Value::new_tuple(vec![
        Value::Int(Uint256::from_usize(code_bytes.len())),
        Value::new_buffer(code_bytes),
    ]);
    machine.runtime_env.insert_full_inbox_contents(vec![msg]);
    let _ = run(&mut machine, vec![], false);

    println!("Machine state after: {:?}", machine.state);
    assert_eq!(
        *machine.stack_top().unwrap(),
        Value::Int(Uint256::from_u64(42))
    );
}
