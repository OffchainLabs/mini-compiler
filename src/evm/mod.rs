/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::evm::abi::FunctionTable;
use crate::evm::abi::{ArbAddressTable, ArbBLS, ArbFunctionTable, ArbSys};
use crate::evm::preinstalled_contracts::{ArbosTest, _ArbInfo};
use crate::run::{load_from_file, load_from_file_and_env, RuntimeEnvironment};
use crate::uint256::Uint256;
use ethers_signers::Signer;
use std::path::Path;

use crate::compile::miniconstants::init_constant_table;
pub use abi::{builtin_contract_path, contract_path, AbiForContract};
pub use benchmarks::make_benchmarks;
pub use evmtest::run_evm_tests;

pub mod abi;
mod benchmarks;
#[cfg(test)]
mod bls;
mod evmtest;
pub mod preinstalled_contracts;

#[derive(Clone)]
pub struct CallInfo<'a> {
    function_name: &'a str,
    args: &'a [ethabi::Token],
    payment: Uint256,
    mutating: bool,
}

pub fn test_contract_path(contract_name: &str) -> String {
    format!(
        "contracts/artifacts/arbos/test/{}.sol/{}.json",
        contract_name, contract_name
    )
}

pub fn _test_contract_path2(parent_name: &str, contract_name: &str) -> String {
    format!(
        "contracts/artifacts/arbos/test/{}.sol/{}.json",
        parent_name, contract_name
    )
}

pub fn evm_xcontract_call_with_constructors(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(100000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let (logs, sends) = pc_contract.call_function(
        my_addr.clone(),
        "deposit",
        &[],
        &mut machine,
        Uint256::from_usize(10000),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    let (logs, sends) = pc_contract.call_function(
        my_addr,
        "transferFib",
        vec![
            ethabi::Token::Address(ethabi::Address::from_low_u64_be(1025)),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn _evm_tx_with_deposit(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;

    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let (logs, sends) = pc_contract._call_function_with_deposit(
        my_addr.clone(),
        "deposit",
        &[],
        &mut machine,
        Uint256::from_usize(10000),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);

    assert!(logs[0].succeeded());

    let (logs, sends) = pc_contract.call_function(
        my_addr,
        "transferFib",
        vec![
            ethabi::Token::Address(ethabi::Address::from_low_u64_be(1025)),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);

    assert!(logs[0].succeeded());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn _evm_block_num_consistency_test(debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);

    let mut bn_contract = AbiForContract::new_from_file(&test_contract_path("BlockNum"))?;
    if bn_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy BlockNum contract");
    }

    let (logs, sends) = bn_contract.call_function(
        my_addr.clone(),
        "getBlock",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());
    let get_block_result = Uint256::from_bytes(&logs[0].get_return_data());

    let (logs, sends) = bn_contract.call_function(
        my_addr.clone(),
        "setBlock",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    let (logs, sends) = bn_contract.call_function(
        my_addr.clone(),
        "currBlock",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());
    let curr_block_result = Uint256::from_bytes(&logs[0].get_return_data());

    assert_eq!(get_block_result, curr_block_result);

    Ok(())
}

pub fn evm_test_arbsys_direct(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbsys = ArbSys::new(&wallet, debug);
    let arb_address_table = ArbAddressTable::new(&wallet, debug);
    AbiForContract::new_from_file(&builtin_contract_path("ArbSys")).unwrap();
    let arb_bls = ArbBLS::new(&wallet, debug);

    let version = arbsys._arbos_version(&mut machine)?;
    assert_eq!(
        version,
        *init_constant_table(Some(Path::new("arb_os/constants.json")))
            .unwrap()
            .get("ArbosVersionNumber")
            .unwrap()
    );

    let tx_count = arbsys.get_transaction_count(&mut machine, my_addr.clone())?;
    assert_eq!(tx_count, Uint256::from_u64(2));

    assert!(arbsys.is_top_level_call(&mut machine)?);

    let mut add_contract = AbiForContract::new_from_file(&test_contract_path("Add")).unwrap();
    let res = add_contract.deploy(&[], &mut machine, Uint256::zero(), None, false);
    assert!(res.is_ok());
    let (add_receipts, _) = add_contract.call_function(
        my_addr.clone(),
        "isTopLevel",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(add_receipts.len(), 1);
    assert_eq!(
        add_receipts[0].get_return_data(),
        Uint256::one().to_bytes_be()
    );
    let (add_receipts, _) = add_contract.call_function(
        my_addr.clone(),
        "isNotTopLevel",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(add_receipts.len(), 1);
    assert_eq!(
        add_receipts[0].get_return_data(),
        Uint256::zero().to_bytes_be()
    );

    let addr_table_index = arb_address_table.register(&mut machine, my_addr.clone())?;
    let lookup_result = arb_address_table.lookup(&mut machine, my_addr.clone())?;
    assert_eq!(addr_table_index, lookup_result);

    let recovered_addr = arb_address_table.lookup_index(&mut machine, lookup_result)?;
    assert_eq!(recovered_addr, my_addr);

    let my_addr_compressed = arb_address_table.compress(&mut machine, my_addr.clone())?;
    let (my_addr_decompressed, offset) =
        arb_address_table.decompress(&mut machine, &my_addr_compressed, Uint256::zero())?;
    assert_eq!(my_addr.clone(), my_addr_decompressed);
    assert_eq!(offset, Uint256::from_usize(my_addr_compressed.len()));

    assert_eq!(Uint256::from_u64(3), arb_address_table.size(&mut machine)?);

    let an_addr = Uint256::from_u64(581351734971918347);
    let an_addr_compressed = arb_address_table.compress(&mut machine, an_addr.clone())?;
    let (an_addr_decompressed, offset) =
        arb_address_table.decompress(&mut machine, &an_addr_compressed, Uint256::zero())?;
    assert_eq!(an_addr.clone(), an_addr_decompressed);
    assert_eq!(offset, Uint256::from_usize(an_addr_compressed.len()));

    let x0 = Uint256::from_u64(17);
    let x1 = Uint256::from_u64(35);
    let y0 = Uint256::from_u64(71);
    let y1 = Uint256::from_u64(143);
    arb_bls.register(&mut machine, x0.clone(), x1.clone(), y0.clone(), y1.clone())?;
    let (ox0, ox1, oy0, oy1) = arb_bls.get_public_key(&mut machine, my_addr.clone())?;
    assert_eq!(x0, ox0);
    assert_eq!(x1, ox1);
    assert_eq!(y0, oy0);
    assert_eq!(y1, oy1);

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(())
}

#[test]
fn test_pay_eoa_from_contract() {
    crate::evm::_evm_pay_eoa_from_contract(None, false);
}

pub fn evm_test_function_table_access(
    log_to: Option<&Path>,
    debug: bool,
) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbsys = ArbSys::new(&wallet, debug);
    let arb_function_table = ArbFunctionTable::new(&wallet, debug);

    let gtc_short_sig = arbsys
        .contract_abi
        .short_signature_for_function("getTransactionCount")
        .unwrap();
    let mut func_table = FunctionTable::new();
    arbsys.contract_abi.append_to_compression_func_table(
        &mut func_table,
        "getTransactionCount",
        false,
        Uint256::from_u64(10000000),
    )?;
    arb_function_table.upload(&mut machine, &func_table)?;

    assert_eq!(
        arb_function_table.size(&mut machine, my_addr.clone())?,
        Uint256::one()
    );

    let (func_code, is_payable, gas_limit) =
        arb_function_table.get(&mut machine, my_addr, Uint256::zero())?;
    assert_eq!(
        func_code,
        Uint256::from_bytes(&gtc_short_sig).shift_left(256 - 32)
    );
    assert_eq!(is_payable, false);
    assert_eq!(gas_limit, Uint256::from_u64(10000000));

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(())
}

pub fn _basic_evm_add_test(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let arbos_test = ArbosTest::new(debug);

    let code = hex::decode("7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0160005500").unwrap();
    let result = arbos_test._install_account_and_call(
        &mut machine,
        Uint256::from_u64(89629813089426890),
        Uint256::zero(),
        Uint256::one(),
        code,
        vec![],
        vec![],
    )?;
    let mut right_answer = vec![0u8; 32];
    right_answer.extend(vec![255u8; 31]);
    right_answer.extend(vec![254u8]);
    assert_eq!(result, right_answer);

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(())
}

pub fn _underfunded_nested_call_test(
    log_to: Option<&Path>,
    debug: bool,
) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let mut contract = AbiForContract::new_from_file(&test_contract_path("Underfunded"))?;
    if contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let (logs, sends) = contract.call_function(
        Uint256::from_u64(1028),
        "nestedCall",
        &[ethabi::Token::Uint(Uint256::zero().to_u256())],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    let (logs, sends) = contract.call_function(
        Uint256::from_u64(1028),
        "nestedCall",
        &[ethabi::Token::Uint(Uint256::one().to_u256())],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(())
}

pub fn _evm_test_callback(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let mut contract = AbiForContract::new_from_file(&test_contract_path("Callback"))?;
    if contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Callback contract");
    }

    let (logs, sends) = contract.call_function(
        Uint256::from_u64(1028),
        "sendDummies",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());
    let evmlogs = logs[0]._get_evm_logs();
    assert_eq!(evmlogs.len(), 3);
    for i in 0..2 {
        assert_eq!(evmlogs[i].addr, contract.address);
        assert_eq!(evmlogs[i].vals[1], Uint256::from_usize(i + 1));
        assert_eq!(
            evmlogs[i].data[0..32],
            Uint256::from_usize(i + 11).to_bytes_be()[0..32]
        );
        assert_eq!(
            evmlogs[i].data[32..64],
            Uint256::from_usize(i + 21).to_bytes_be()[0..32]
        );
    }

    let (logs, _) = contract.call_function(
        Uint256::from_u64(1028),
        "doCallback",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert!(logs[0].succeeded());
    let evmlogs = logs[0]._get_evm_logs();
    assert_eq!(evmlogs.len(), 8);

    println!("{}", evmlogs[2].vals[0]);
    assert_eq!(
        evmlogs[2].vals[0],
        Uint256::from_bytes(
            &hex::decode("5baaa87db386365b5c161be377bc3d8e317e8d98d71a3ca7ed7d555340c8f767")
                .unwrap()
        )
    );
    assert_eq!(evmlogs[2].addr, Uint256::from_u64(100)); // log was emitted by ArbSys
    assert_eq!(evmlogs[2].vals[2], Uint256::zero()); // unique ID = 0
    let batch_number = &evmlogs[2].vals[3];
    assert_eq!(batch_number, &Uint256::zero());
    let index_in_batch = Uint256::from_bytes(&evmlogs[2].data[32..64]);
    assert_eq!(index_in_batch, Uint256::zero());
    let calldata_size = Uint256::from_bytes(&evmlogs[2].data[(7 * 32)..(8 * 32)]);
    assert_eq!(calldata_size, Uint256::from_u64(11));

    assert_eq!(
        evmlogs[6].vals[0],
        Uint256::from_bytes(
            &hex::decode("5baaa87db386365b5c161be377bc3d8e317e8d98d71a3ca7ed7d555340c8f767")
                .unwrap()
        )
    );
    assert_eq!(evmlogs[6].addr, Uint256::from_u64(100)); // log was emitted by ArbSys
    assert_eq!(evmlogs[6].vals[2], Uint256::one()); // unique ID = 1
    let batch_number = &evmlogs[6].vals[3];
    assert_eq!(batch_number, &Uint256::zero());
    let index_in_batch = Uint256::from_bytes(&evmlogs[6].data[32..64]);
    assert_eq!(index_in_batch, Uint256::one());
    let calldata_size = Uint256::from_bytes(&evmlogs[6].data[(7 * 32)..(8 * 32)]);
    assert_eq!(calldata_size, Uint256::from_u64(17));

    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // advance time so that sends are emitted

    let sends = machine.runtime_env.get_all_sends();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[0][0], 3u8); // send type
    assert_eq!(sends[0][1..33], contract.address.to_bytes_be()[0..32]);
    assert_eq!(sends[0][161..193], [0u8; 32]);
    assert_eq!(sends[0].len(), 204); // 11 bytes of calldata after 193 bytes of fields
    assert_eq!(sends[1][0], 3u8); // send type
    assert_eq!(sends[1][1..33], contract.address.to_bytes_be()[0..32]);
    assert_eq!(sends[1][161..193], [0u8; 32]);
    assert_eq!(sends[1].len(), 210); // 17 bytes of calldata after 193 bytes of fields

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(())
}

pub fn evm_test_create(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(100000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let (logs, sends) = pc_contract.call_function(
        my_addr.clone(),
        "testCreate",
        &[],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);
    assert!(logs[0].succeeded());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn evm_xcontract_call_using_batch(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    use std::convert::TryFrom;
    let mut rt_env = RuntimeEnvironment::default();

    let wallet = rt_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
    machine.start_at_zero();

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(100000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let mut batch = machine.runtime_env.new_batch();
    let tx_id_1 = pc_contract.add_function_call_to_batch(
        &mut batch,
        "deposit",
        &[],
        &mut machine,
        Uint256::from_usize(10000),
        &wallet,
    )?;
    let tx_id_2 = pc_contract.add_function_call_to_batch(
        &mut batch,
        "transferFib",
        vec![
            ethabi::Token::Address(ethereum_types::H160::from_slice(
                &my_addr.to_bytes_minimal(),
            )),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        &wallet,
    )?;

    machine
        .runtime_env
        .insert_batch_message(Uint256::from_usize(1025), &batch);

    let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
    let num_sends_before = machine.runtime_env.get_all_sends().len();
    let _arbgas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };
    let logs = machine.runtime_env.get_all_receipt_logs();
    let sends = machine.runtime_env.get_all_sends();
    let logs = &logs[num_logs_before..];
    let sends = &sends[num_sends_before..];

    assert_eq!(logs.len(), 2);
    assert_eq!(sends.len(), 0);

    assert!(logs[0].succeeded());
    assert_eq!(logs[0].get_request_id(), tx_id_1);
    let gas_used_so_far_1 = logs[0].get_gas_used_so_far();

    assert!(logs[1].succeeded());
    assert_eq!(logs[1].get_request_id(), tx_id_2);
    assert_eq!(
        gas_used_so_far_1.add(&logs[1].get_gas_used()),
        logs[1].get_gas_used_so_far()
    );

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn _evm_xcontract_call_using_compressed_batch(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    use std::convert::TryFrom;
    let mut rt_env = RuntimeEnvironment::default();

    let wallet = rt_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
    machine.start_at_zero();

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(100000),
    );
    machine
        .runtime_env
        ._advance_time(Uint256::from_u64(50), None, true);
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let mut batch = machine.runtime_env.new_batch();
    let tx_id_1 = pc_contract._add_function_call_to_compressed_batch(
        &mut batch,
        "deposit",
        &[],
        &mut machine,
        Uint256::from_usize(10000),
        &wallet,
    )?;
    let tx_id_2 = pc_contract._add_function_call_to_compressed_batch(
        &mut batch,
        "transferFib",
        vec![
            ethabi::Token::Address(ethereum_types::H160::from_slice(
                &my_addr.to_bytes_minimal(),
            )),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        &wallet,
    )?;

    machine
        .runtime_env
        .insert_batch_message(Uint256::from_usize(1025), &batch);

    let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
    let num_sends_before = machine.runtime_env.get_all_sends().len();
    let _arbgas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };
    let logs = machine.runtime_env.get_all_receipt_logs();
    let sends = machine.runtime_env.get_all_sends();
    let logs = &logs[num_logs_before..];
    let sends = &sends[num_sends_before..];

    assert_eq!(logs.len(), 2);
    assert_eq!(sends.len(), 0);

    assert!(logs[0].succeeded());
    assert_eq!(logs[0].get_request_id(), tx_id_1);
    let gas_used_so_far_1 = logs[0].get_gas_used_so_far();

    assert!(logs[1].succeeded());
    assert_eq!(logs[1].get_request_id(), tx_id_2);
    assert_eq!(
        gas_used_so_far_1.add(&logs[1].get_gas_used()),
        logs[1].get_gas_used_so_far()
    );

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn _evm_xcontract_call_using_compressed_batch_2(
    log_to: Option<&Path>,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    use std::convert::TryFrom;
    let mut rt_env = RuntimeEnvironment::default();

    let wallet = rt_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
    machine.start_at_zero();

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(100000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if fib_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Fibonacci contract");
    }

    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if pc_contract
        .deploy(
            &[ethabi::Token::Address(ethereum_types::H160::from_slice(
                &fib_contract.address.to_bytes_be()[12..],
            ))],
            &mut machine,
            Uint256::zero(),
            None,
            debug,
        )
        .is_err()
    {
        panic!("failed to deploy PaymentChannel contract");
    }

    let mut batch = machine.runtime_env.new_batch();
    let tx_id_1 = pc_contract._add_function_call_to_compressed_batch(
        &mut batch,
        "deposit",
        &[],
        &mut machine,
        Uint256::from_usize(10000),
        &wallet,
    )?;
    let tx_id_2 = pc_contract._add_function_call_to_compressed_batch(
        &mut batch,
        "transferFib",
        vec![
            ethabi::Token::Address(ethereum_types::H160::from_slice(
                &my_addr.to_bytes_minimal(),
            )),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        &wallet,
    )?;

    machine
        .runtime_env
        .insert_batch_message(Uint256::from_usize(1025), &batch);

    let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
    let num_sends_before = machine.runtime_env.get_all_sends().len();
    let _arbgas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };
    let logs = machine.runtime_env.get_all_receipt_logs();
    let sends = machine.runtime_env.get_all_sends();
    let logs = &logs[num_logs_before..];
    let sends = &sends[num_sends_before..];

    assert_eq!(logs.len(), 2);
    assert_eq!(sends.len(), 0);

    assert!(logs[0].succeeded());
    assert_eq!(logs[0].get_request_id(), tx_id_1);
    let gas_used_so_far_1 = logs[0].get_gas_used_so_far();

    assert!(logs[1].succeeded());
    assert_eq!(logs[1].get_request_id(), tx_id_2);
    assert_eq!(
        gas_used_so_far_1.add(&logs[1].get_gas_used()),
        logs[1].get_gas_used_so_far()
    );

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

pub fn evm_direct_deploy_add(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&[], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    }

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn _evm_pay_eoa_from_contract(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let add_contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&[], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };

    let payer = Uint256::from_u64(5386492);
    let recipient = Uint256::from_u64(5771838591);
    machine.runtime_env.insert_eth_deposit_message(
        payer.clone(),
        payer.clone(),
        Uint256::_from_eth(1000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let arbinfo = _ArbInfo::_new(debug);
    let balance_before = arbinfo._get_balance(&mut machine, &recipient).unwrap();
    assert!(balance_before.is_zero());

    let (receipts, _) = add_contract
        .call_function(
            payer,
            "payTo",
            &[ethabi::Token::Address(recipient.to_h160())],
            &mut machine,
            Uint256::one(),
            debug,
        )
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].succeeded());

    let balance_after = arbinfo._get_balance(&mut machine, &recipient).unwrap();
    assert_eq!(balance_after, Uint256::one());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn _evm_test_payment_in_constructor(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(10000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result =
                contract.deploy(&vec![], &mut machine, Uint256::from_u64(10000), None, debug);

            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };

    let result = contract.call_function(
        my_addr.clone(),
        "withdraw5000",
        vec![].as_ref(),
        &mut machine,
        Uint256::zero(),
        debug,
    );
    match result {
        Ok((logs, _sends)) => {
            assert_eq!(logs.len(), 1);
            assert!(logs[0].succeeded());
        }
        Err(e) => {
            panic!("{}", e.to_string());
        }
    }

    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // make sure the machine notices that time advanced

    let last_send = machine.runtime_env._get_last_send().unwrap();
    assert_eq!(last_send[0], 3u8);
    assert_eq!(last_send[1..33], contract.address.to_bytes_be());
    assert_eq!(last_send[33..65], Uint256::from_u64(1025).to_bytes_be());
    assert_eq!(last_send[161..193], Uint256::from_u64(5000).to_bytes_be());
    assert_eq!(last_send.len(), 193);

    if let Some(path) = log_to {
        let _ = machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn evm_test_arbsys(log_to: Option<&Path>, debug: bool) {
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_usize(10000),
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle this eth deposit message

    let contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&vec![], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };
    let result = contract.call_function(
        my_addr.clone(),
        "getSeqNum",
        vec![].as_ref(),
        &mut machine,
        Uint256::zero(),
        debug,
    );
    match result {
        Ok((logs, sends)) => {
            assert_eq!(logs.len(), 1);
            assert_eq!(sends.len(), 0);
            assert!(logs[0].succeeded());
            let decoded_result = contract
                .get_function("getSeqNum")
                .unwrap()
                .decode_output(&logs[0].get_return_data())
                .unwrap();
            assert_eq!(
                decoded_result[0],
                ethabi::Token::Uint(ethabi::Uint::try_from(2).unwrap())
            );
        }
        Err(e) => {
            panic!("{}", e.to_string());
        }
    }

    let result = contract.call_function(
        my_addr.clone(),
        "withdrawMyEth",
        vec![].as_ref(),
        &mut machine,
        Uint256::from_usize(5000),
        debug,
    );
    match result {
        Ok((logs, _sends)) => {
            assert_eq!(logs.len(), 1);
            assert!(logs[0].succeeded());
        }
        Err(e) => {
            panic!("{}", e.to_string());
        }
    }

    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // make sure the machine notices that time advanced

    let last_send = machine.runtime_env._get_last_send().unwrap();
    assert_eq!(last_send[0], 3u8);
    assert_eq!(last_send[1..33], contract.address.to_bytes_be());
    assert_eq!(last_send[33..65], my_addr.to_bytes_be());
    assert_eq!(last_send[161..193], Uint256::from_u64(5000).to_bytes_be());
    assert_eq!(last_send.len(), 193);

    if let Some(path) = log_to {
        let _ = machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn evm_direct_deploy_and_call_add(log_to: Option<&Path>, debug: bool) {
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    let contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&[], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };

    let result = contract.call_function(
        my_addr,
        "add",
        vec![
            ethabi::Token::Uint(ethabi::Uint::one()),
            ethabi::Token::Uint(ethabi::Uint::one()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        debug,
    );
    match result {
        Ok((logs, sends)) => {
            assert_eq!(logs.len(), 1);
            assert_eq!(sends.len(), 0);
            assert!(logs[0].succeeded());
            let decoded_result = contract
                .get_function("add")
                .unwrap()
                .decode_output(&logs[0].get_return_data())
                .unwrap();
            assert_eq!(
                decoded_result[0],
                ethabi::Token::Uint(ethabi::Uint::try_from(2).unwrap())
            );
        }
        Err(e) => {
            panic!("{}", e.to_string());
        }
    }

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn _evm_test_contract_call(log_to: Option<&Path>, debug: bool) {
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);
    let contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&[], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };

    for i in 0..4 {
        let result = contract._call_function_from_contract(
            my_addr.clone(),
            "add",
            vec![
                ethabi::Token::Uint(ethabi::Uint::one()),
                ethabi::Token::Uint(Uint256::from_u64(i).to_u256()),
            ]
            .as_ref(),
            &mut machine,
            Uint256::zero(),
            debug,
        );
        match result {
            Ok((logs, sends)) => {
                assert_eq!(logs.len(), 1);
                assert_eq!(sends.len(), 0);
                assert!(logs[0].succeeded());
                let decoded_result = contract
                    .get_function("add")
                    .unwrap()
                    .decode_output(&logs[0].get_return_data())
                    .unwrap();
                assert_eq!(
                    decoded_result[0],
                    ethabi::Token::Uint(ethabi::Uint::try_from(1 + i).unwrap())
                );
            }
            Err(e) => {
                panic!("{}", e.to_string());
            }
        }
    }

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn evm_direct_deploy_and_compressed_call_add(log_to: Option<&Path>, debug: bool) {
    use std::convert::TryFrom;
    let mut rt_env = RuntimeEnvironment::default();
    let wallet = rt_env.new_wallet();
    let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
    machine.start_at_zero();

    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());
    let contract = match AbiForContract::new_from_file(&test_contract_path("Add")) {
        Ok(mut contract) => {
            let result = contract.deploy(&[], &mut machine, Uint256::zero(), None, debug);
            if let Ok(contract_addr) = result {
                assert_ne!(contract_addr, Uint256::zero());
                contract
            } else {
                panic!("deploy failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };

    let result = contract.call_function_compressed(
        my_addr,
        "add",
        vec![
            ethabi::Token::Uint(ethabi::Uint::one()),
            ethabi::Token::Uint(ethabi::Uint::one()),
        ]
        .as_ref(),
        &mut machine,
        Uint256::zero(),
        &wallet,
        debug,
    );
    match result {
        Ok((logs, sends)) => {
            assert_eq!(logs.len(), 1);
            assert_eq!(sends.len(), 0);
            assert!(logs[0].succeeded());
            let decoded_result = contract
                .get_function("add")
                .unwrap()
                .decode_output(&logs[0].get_return_data())
                .unwrap();
            assert_eq!(
                decoded_result[0],
                ethabi::Token::Uint(ethabi::Uint::try_from(2).unwrap())
            );
        }
        Err(e) => {
            panic!("{}", e.to_string());
        }
    }

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

#[test]
fn evm_reverter_factory_test() {
    _evm_reverter_factory_test_impl();
}

fn _evm_reverter_factory_test_impl() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let _contract = match AbiForContract::new_from_file(&test_contract_path("ReverterFactory")) {
        Ok(mut contract) => {
            let result = contract.deploy(
                &[ethabi::Token::Uint(Uint256::one().to_u256())],
                &mut machine,
                Uint256::zero(),
                None,
                false,
            );
            if let Err(maybe_receipt) = result {
                if maybe_receipt.is_none() {
                    panic!("deploy failed without receipt");
                }
            } else {
                panic!("deploy succeeded but should have failed");
            }
        }
        Err(e) => {
            panic!("error loading contract: {:?}", e);
        }
    };
}

pub fn evm_payment_to_empty_address(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);
    let dest_addr = Uint256::from_u64(4242);

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_u64(20000),
    );
    let tx_id = machine.runtime_env.insert_tx_message(
        my_addr,
        Uint256::from_u64(1000000000),
        None,
        dest_addr,
        Uint256::from_u64(10000),
        &vec![],
        false,
    );

    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let receipts = machine.runtime_env.get_all_receipt_logs();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].get_request_id(), tx_id);
    assert!(receipts[1].succeeded());

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn evm_eval_sha256(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);

    let tx_id = machine.runtime_env.insert_tx_message(
        my_addr,
        Uint256::from_u64(10000000),
        None,
        Uint256::from_u64(2), // sha256 precompile
        Uint256::from_u64(0),
        &vec![0xCCu8],
        false,
    );

    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let receipts = machine.runtime_env.get_all_receipt_logs();
    println!("result code: {}", receipts[0].get_return_code());
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].get_request_id(), tx_id);
    assert!(receipts[0].succeeded());
    let return_data = receipts[0].get_return_data();
    let return_uint = Uint256::from_bytes(&return_data);
    assert_eq!(
        return_uint,
        Uint256::from_string_hex(
            "1dd8312636f6a0bf3d21fa2855e63072507453e93a5ced4301b364e91c9d87d6"
        )
        .unwrap()
    );

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

pub fn _evm_ecpairing_precompile(_log_to: Option<&Path>, debug: bool) {
    for (calldata, result) in &[
        // test vectors from geth: https://github.com/ethereum/go-ethereum/blob/2045a2bba3cd2f93fd913c692be146adabd8940c/core/vm/testdata/precompiles/bn256Pairing.json
        ("1c76476f4def4bb94541d57ebba1193381ffa7aa76ada664dd31c16024c43f593034dd2920f673e204fee2811c678745fc819b55d3e9d294e45c9b03a76aef41209dd15ebff5d46c4bd888e51a93cf99a7329636c63514396b4a452003a35bf704bf11ca01483bfa8b34b43561848d28905960114c8ac04049af4b6315a416782bb8324af6cfc93537a2ad1a445cfd0ca2a71acd7ac41fadbf933c2a51be344d120a2a4cf30c1bf9845f20c6fe39e07ea2cce61f0c9bb048165fe5e4de877550111e129f1cf1097710d41c4ac70fcdfa5ba2023c6ff1cbeac322de49d1b6df7c2032c61a830e3c17286de9462bf242fca2883585b93870a73853face6a6bf411198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("2eca0c7238bf16e83e7a1e6c5d49540685ff51380f309842a98561558019fc0203d3260361bb8451de5ff5ecd17f010ff22f5c31cdf184e9020b06fa5997db841213d2149b006137fcfb23036606f848d638d576a120ca981b5b1a5f9300b3ee2276cf730cf493cd95d64677bbb75fc42db72513a4c1e387b476d056f80aa75f21ee6226d31426322afcda621464d0611d226783262e21bb3bc86b537e986237096df1f82dff337dd5972e32a8ad43e28a78a96a823ef1cd4debe12b6552ea5f06967a1237ebfeca9aaae0d6d0bab8e28c198c5a339ef8a2407e31cdac516db922160fa257a5fd5b280642ff47b65eca77e626cb685c84fa6d3b6882a283ddd1198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("0f25929bcb43d5a57391564615c9e70a992b10eafa4db109709649cf48c50dd216da2f5cb6be7a0aa72c440c53c9bbdfec6c36c7d515536431b3a865468acbba2e89718ad33c8bed92e210e81d1853435399a271913a6520736a4729cf0d51eb01a9e2ffa2e92599b68e44de5bcf354fa2642bd4f26b259daa6f7ce3ed57aeb314a9a87b789a58af499b314e13c3d65bede56c07ea2d418d6874857b70763713178fb49a2d6cd347dc58973ff49613a20757d0fcc22079f9abd10c3baee245901b9e027bd5cfc2cb5db82d4dc9677ac795ec500ecd47deee3b5da006d6d049b811d7511c78158de484232fc68daf8a45cf217d1c2fae693ff5871e8752d73b21198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("2f2ea0b3da1e8ef11914acf8b2e1b32d99df51f5f4f206fc6b947eae860eddb6068134ddb33dc888ef446b648d72338684d678d2eb2371c61a50734d78da4b7225f83c8b6ab9de74e7da488ef02645c5a16a6652c3c71a15dc37fe3a5dcb7cb122acdedd6308e3bb230d226d16a105295f523a8a02bfc5e8bd2da135ac4c245d065bbad92e7c4e31bf3757f1fe7362a63fbfee50e7dc68da116e67d600d9bf6806d302580dc0661002994e7cd3a7f224e7ddc27802777486bf80f40e4ca3cfdb186bac5188a98c45e6016873d107f5cd131f3a3e339d0375e58bd6219347b008122ae2b09e539e152ec5364e7e2204b03d11d3caa038bfc7cd499f8176aacbee1f39e4e4afc4bc74790a4a028aff2c3d2538731fb755edefd8cb48d6ea589b5e283f150794b6736f670d6a1033f9b46c6f5204f50813eb85c8dc4b59db1c5d39140d97ee4d2b36d99bc49974d18ecca3e7ad51011956051b464d9e27d46cc25e0764bb98575bd466d32db7b15f582b2d5c452b36aa394b789366e5e3ca5aabd415794ab061441e51d01e94640b7e3084a07e02c78cf3103c542bc5b298669f211b88da1679b0b64a63b7e0e7bfe52aae524f73a55be7fe70c7e9bfc94b4cf0da1213d2149b006137fcfb23036606f848d638d576a120ca981b5b1a5f9300b3ee2276cf730cf493cd95d64677bbb75fc42db72513a4c1e387b476d056f80aa75f21ee6226d31426322afcda621464d0611d226783262e21bb3bc86b537e986237096df1f82dff337dd5972e32a8ad43e28a78a96a823ef1cd4debe12b6552ea5f", true),
        ("20a754d2071d4d53903e3b31a7e98ad6882d58aec240ef981fdf0a9d22c5926a29c853fcea789887315916bbeb89ca37edb355b4f980c9a12a94f30deeed30211213d2149b006137fcfb23036606f848d638d576a120ca981b5b1a5f9300b3ee2276cf730cf493cd95d64677bbb75fc42db72513a4c1e387b476d056f80aa75f21ee6226d31426322afcda621464d0611d226783262e21bb3bc86b537e986237096df1f82dff337dd5972e32a8ad43e28a78a96a823ef1cd4debe12b6552ea5f1abb4a25eb9379ae96c84fff9f0540abcfc0a0d11aeda02d4f37e4baf74cb0c11073b3ff2cdbb38755f8691ea59e9606696b3ff278acfc098fa8226470d03869217cee0a9ad79a4493b5253e2e4e3a39fc2df38419f230d341f60cb064a0ac290a3d76f140db8418ba512272381446eb73958670f00cf46f1d9e64cba057b53c26f64a8ec70387a13e41430ed3ee4a7db2059cc5fc13c067194bcc0cb49a98552fd72bd9edb657346127da132e5b82ab908f5816c826acb499e22f2412d1a2d70f25929bcb43d5a57391564615c9e70a992b10eafa4db109709649cf48c50dd2198a1f162a73261f112401aa2db79c7dab1533c9935c77290a6ce3b191f2318d198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("1c76476f4def4bb94541d57ebba1193381ffa7aa76ada664dd31c16024c43f593034dd2920f673e204fee2811c678745fc819b55d3e9d294e45c9b03a76aef41209dd15ebff5d46c4bd888e51a93cf99a7329636c63514396b4a452003a35bf704bf11ca01483bfa8b34b43561848d28905960114c8ac04049af4b6315a416782bb8324af6cfc93537a2ad1a445cfd0ca2a71acd7ac41fadbf933c2a51be344d120a2a4cf30c1bf9845f20c6fe39e07ea2cce61f0c9bb048165fe5e4de877550111e129f1cf1097710d41c4ac70fcdfa5ba2023c6ff1cbeac322de49d1b6df7c103188585e2364128fe25c70558f1560f4f9350baf3959e603cc91486e110936198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", false),
        ("", true),
        ("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", false),
        ("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d", true),
        ("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("105456a333e6d636854f987ea7bb713dfd0ae8371a72aea313ae0c32c0bf10160cf031d41b41557f3e7e3ba0c51bebe5da8e6ecd855ec50fc87efcdeac168bcc0476be093a6d2b4bbf907172049874af11e1b6267606e00804d3ff0037ec57fd3010c68cb50161b7d1d96bb71edfec9880171954e56871abf3d93cc94d745fa114c059d74e5b6c4ec14ae5864ebe23a71781d86c29fb8fb6cce94f70d3de7a2101b33461f39d9e887dbb100f170a2345dde3c07e256d1dfa2b657ba5cd030427000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000021a2c3013d2ea92e13c800cde68ef56a294b883f6ac35d25f587c09b1b3c635f7290158a80cd3d66530f74dc94c94adb88f5cdb481acca997b6e60071f08a115f2f997f3dbd66a7afe07fe7862ce239edba9e05c5afff7f8a1259c9733b2dfbb929d1691530ca701b4a106054688728c9972c8512e9789e9567aae23e302ccd75", true),
        ("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed275dc4a288d1afb3cbb1ac09187524c7db36395df7be3b99e673b13a075a65ec1d9befcd05a5323e6da4d435f3b617cdb3af83285c2df711ef39c01571827f9d", true),
        ("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002203e205db4f19b37b60121b83a7333706db86431c6d835849957ed8c3928ad7927dc7234fd11d3e8c36c59277c3e6f149d5cd3cfa9a62aee49f8130962b4b3b9195e8aa5b7827463722b8c153931579d3505566b4edf48d498e185f0509de15204bb53b8977e5f92a0bc372742c4830944a59b4fe6b1c0466e2a6dad122b5d2e030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd31a76dae6d3272396d0cbe61fced2bc532edac647851e3ac53ce1cc9c7e645a83198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c21800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa", true),
        ("105456a333e6d636854f987ea7bb713dfd0ae8371a72aea313ae0c32c0bf10160cf031d41b41557f3e7e3ba0c51bebe5da8e6ecd855ec50fc87efcdeac168bcc0476be093a6d2b4bbf907172049874af11e1b6267606e00804d3ff0037ec57fd3010c68cb50161b7d1d96bb71edfec9880171954e56871abf3d93cc94d745fa114c059d74e5b6c4ec14ae5864ebe23a71781d86c29fb8fb6cce94f70d3de7a2101b33461f39d9e887dbb100f170a2345dde3c07e256d1dfa2b657ba5cd030427000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000021a2c3013d2ea92e13c800cde68ef56a294b883f6ac35d25f587c09b1b3c635f7290158a80cd3d66530f74dc94c94adb88f5cdb481acca997b6e60071f08a115f2f997f3dbd66a7afe07fe7862ce239edba9e05c5afff7f8a1259c9733b2dfbb929d1691530ca701b4a106054688728c9972c8512e9789e9567aae23e302ccd75", true),
    ] {
        _evm_ecpairing_precompile_test_one(calldata, *result, debug);
    }
}

fn _evm_ecpairing_precompile_test_one(calldata: &str, result: bool, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);
    let calldata = hex::decode(calldata).unwrap();
    assert_eq!(calldata.len() % (6 * 32), 0);

    let tx_id = machine.runtime_env.insert_tx_message(
        my_addr,
        Uint256::from_u64(1000000000),
        None,
        Uint256::from_u64(8), // ecpairing precompile
        Uint256::from_u64(0),
        &calldata,
        false,
    );

    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let receipts = machine.runtime_env.get_all_receipt_logs();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].get_request_id(), tx_id);
    assert!(receipts[0].succeeded());
    let return_data = receipts[0].get_return_data();
    let return_uint = Uint256::from_bytes(&return_data);
    assert_eq!(return_uint == Uint256::one(), result);

    //if let Some(path) = log_to {
    //    machine.runtime_env.recorder.to_file(path, machine.get_total_gas_usage().to_u64().unwrap()).unwrap();
    //}
}

pub fn _evm_eval_ripemd160(log_to: Option<&Path>, debug: bool) {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);
    let tx_id = machine.runtime_env.insert_tx_message(
        my_addr,
        Uint256::from_u64(1000000000),
        None,
        Uint256::from_u64(3), // ripemd160 precompile
        Uint256::from_u64(0),
        &vec![0x61u8],
        false,
    );

    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let receipts = machine.runtime_env.get_all_receipt_logs();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].get_request_id(), tx_id);
    assert!(receipts[0].succeeded());
    let return_data = receipts[0].get_return_data();
    let return_uint = Uint256::from_bytes(&return_data);
    assert_eq!(
        return_uint,
        Uint256::from_string_hex(
            "0000000000000000000000000bdc9d2d256b3ee9daae347be6f4dc835a467ffe"
        )
        .unwrap()
    );

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }
}

#[test]
fn evm_bad_receipt_revert_test() {
    _evm_bad_receipt_revert_test_impl();
}

fn _evm_bad_receipt_revert_test_impl() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);

    let mut add_contract = AbiForContract::new_from_file(&test_contract_path("Add")).unwrap();
    let _ = add_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, false)
        .unwrap();
    let (receipts, _) = add_contract
        .call_function(
            my_addr.clone(),
            "add",
            &[
                ethabi::Token::Uint(Uint256::one().to_u256()),
                ethabi::Token::Uint(Uint256::one().to_u256()),
            ],
            &mut machine,
            Uint256::zero(),
            false,
        )
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].succeeded());
    let _ = machine.run(None);
    let total_receipts_before = machine.runtime_env.get_all_receipt_logs().len();

    let txid = machine.runtime_env.insert_tx_message(
        my_addr,
        Uint256::zero(),
        None,
        add_contract.address,
        Uint256::one(),
        &[],
        true,
    );
    assert!(txid != receipts[0].get_request_id());
    let _ = machine.run(None);

    let receipts = machine.runtime_env.get_all_receipt_logs();
    assert_eq!(receipts.len(), total_receipts_before + 1);
    assert_eq!(
        receipts[total_receipts_before].get_return_code(),
        Uint256::from_u64(10)
    );
}

#[test]
fn evm_test_constructor_recursion() {
    let _ = _test_constructor_recursion().unwrap();
}

pub fn _test_constructor_recursion() -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_usize(1025);

    let mut ccontract = AbiForContract::new_from_file(&_test_contract_path2(
        "ReverterFactory",
        "ConstructorCallback2",
    ))?;
    if ccontract
        .deploy(&[], &mut machine, Uint256::zero(), None, false)
        .is_err()
    {
        panic!("failed to deploy ConstructorCallback contract");
    }

    let (receipts, _) = ccontract
        .call_function(
            my_addr.clone(),
            "test",
            &[],
            &mut machine,
            Uint256::zero(),
            false,
        )
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].succeeded());

    Ok(())
}

pub fn make_logs_for_all_arbos_tests() {
    evm_direct_deploy_add(
        Some(Path::new("testlogs/evm_direct_deploy_add.aoslog")),
        false,
    );
    evm_direct_deploy_and_call_add(
        Some(Path::new("testlogs/evm_direct_deploy_and_call_add.aoslog")),
        false,
    );
    let _ = evm_test_arbsys_direct(
        Some(Path::new("testlogs/evm_test_arbsys_direct.aoslog")),
        false,
    );
    let _ = evm_test_function_table_access(
        Some(Path::new("testlogs/evm_test_function_table_access.aoslog")),
        false,
    );
    let _ = evm_xcontract_call_with_constructors(
        Some(Path::new(
            "testlogs/evm_xcontract_call_with_constructors.aoslog",
        )),
        false,
        false,
    );
    let _ = evm_xcontract_call_using_batch(
        Some(Path::new("testlogs/evm_xcontract_call_using_batch.aoslog")),
        false,
        false,
    );
    /*let _ = evm_xcontract_call_using_compressed_batch(
        Some(Path::new("testlogs/evm_xcontract_call_using_batch.aoslog")),
        false,
        false,
    );*/
    evm_direct_deploy_and_compressed_call_add(
        Some(Path::new(
            "testlogs/evm_direct_deploy_and_compressed_call_add.aoslog",
        )),
        false,
    );
    let _ = evm_test_create(
        Some(Path::new("testlogs/evm_test_create.aoslog")),
        false,
        false,
    );
    evm_test_arbsys(Some(Path::new("testlogs/evm_test_arbsys.aoslog")), false);
    evm_eval_sha256(Some(Path::new("testlogs/evm_eval_sha256.aoslog")), false);
    evm_payment_to_empty_address(
        Some(Path::new("testlogs/payment_to_empty_address.aoslog")),
        false,
    );
}
