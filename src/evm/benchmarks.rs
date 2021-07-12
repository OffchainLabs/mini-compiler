/*
* Copyright 2020, Offchain Labs, Inc. All rights reserved.
*/

use crate::evm::abi::deploy_add;
use crate::run::load_from_file;
use crate::uint256::Uint256;
use std::path::Path;

pub fn make_benchmarks() -> Result<(), ethabi::Error> {
    let benchmarks: Vec<(
        fn(u64, &Path) -> Result<u64, ethabi::Error>,
        u64,
        &str,
        &str,
    )> = vec![
        (benchmark_boot, 1, "boot ArbOS", "boot"),
        (benchmark_add, 100, "100 null txs", "nulltx_100"),
        (benchmark_add, 1000, "1000 null txs", "nulltx_1000"),
        (
            benchmark_add_batched,
            100,
            "100 signed batched null txs",
            "nulltx_batch_100",
        ),
        (
            benchmark_add_batched,
            500,
            "500 signed batched null txs",
            "nulltx_batch_500",
        ),
    ];

    for (bm_func, iterations, description, filename) in benchmarks {
        let filename = format!("benchmarks/{}.aoslog", filename);
        let gas_used = bm_func(iterations, Path::new(&filename))?;
        println!("ArbGas for {}: {}", description, gas_used)
    }
    Ok(())
}

pub fn benchmark_boot(_iterations: u64, log_to: &Path) -> Result<u64, ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero(false);

    let gas_used = machine.run(None);
    machine
        .runtime_env
        .recorder
        .to_file(log_to, machine.get_total_gas_usage().to_u64().unwrap())
        .unwrap();
    Ok(gas_used)
}

pub fn benchmark_add(iterations: u64, log_to: &Path) -> Result<u64, ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero(false);

    let my_addr = Uint256::from_u64(1025);
    let contract = deploy_add(&mut machine)?;

    for _ in 0..iterations {
        let _result = contract
            .call_function(
                my_addr.clone(),
                "add",
                vec![
                    ethabi::Token::Uint(ethabi::Uint::one()),
                    ethabi::Token::Uint(ethabi::Uint::one()),
                ]
                .as_ref(),
                &mut machine,
                Uint256::zero(),
                false,
            )
            .unwrap();
    }

    machine
        .runtime_env
        .recorder
        .to_file(log_to, machine.get_total_gas_usage().to_u64().unwrap())
        .unwrap();
    Ok(machine.get_total_gas_usage().to_u64().unwrap())
}

pub fn benchmark_add_batched(iterations: u64, log_to: &Path) -> Result<u64, ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero(false);

    let wallet = machine.runtime_env.new_wallet();

    let contract = deploy_add(&mut machine)?;

    let mut batch = machine.runtime_env.new_batch();

    for _ in 0..iterations {
        let _tx_id = contract
            .add_function_call_to_batch(
                &mut batch,
                "add",
                vec![
                    ethabi::Token::Uint(ethabi::Uint::one()),
                    ethabi::Token::Uint(ethabi::Uint::one()),
                ]
                .as_ref(),
                &mut machine,
                Uint256::zero(),
                &wallet,
            )
            .unwrap();
    }

    machine
        .runtime_env
        .insert_batch_message(Uint256::from_usize(1025), &batch);

    machine.run(None);

    machine
        .runtime_env
        .recorder
        .to_file(log_to, machine.get_total_gas_usage().to_u64().unwrap())
        .unwrap();
    Ok(machine.get_total_gas_usage().to_u64().unwrap())
}
