/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::mavm::{Buffer, Value};
use crate::run::{load_from_file_and_env, ProfilerMode};
use crate::uint256::Uint256;
use ethers_core::rand::rngs::StdRng;
use ethers_core::rand::SeedableRng;
use ethers_core::types::TransactionRequest;
use ethers_core::utils::keccak256;
use ethers_signers::{Signer, Wallet};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::io::Read;
use std::sync::Arc;
use std::{collections::HashMap, fs::File, io, path::Path};

#[derive(Debug, Clone)]
pub struct RuntimeEnvironment {
    pub chain_id: u64,
    pub l1_inbox: Vec<Value>,
    pub current_block_num: Uint256,
    pub current_timestamp: Uint256,
    pub logs: Vec<Value>,
    pub sends: Vec<Vec<u8>>,
    pub next_inbox_seq_num: Uint256,
    pub caller_seq_nums: HashMap<Uint256, Uint256>,
    next_id: Uint256, // used to assign unique (but artificial) txids to messages
    pub recorder: RtEnvRecorder,
    compressor: TxCompressor,
    charging_policy: Option<(Uint256, Uint256, Uint256)>,
    num_wallets: u64,
    chain_init_message: Vec<u8>,
}

impl RuntimeEnvironment {
    pub fn new(charging_policy: Option<(Uint256, Uint256, Uint256)>) -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            charging_policy,
            None,
        )
    }

    pub fn _new_options() -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            None,
            None,
        )
    }

    pub fn _new_with_owner(owner: Option<Uint256>) -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            None,
            owner,
        )
    }

    pub fn new_with_blocknum_timestamp(
        blocknum: Uint256,
        timestamp: Uint256,
        charging_policy: Option<(Uint256, Uint256, Uint256)>,
        owner: Option<Uint256>,
    ) -> Self {
        let chain_id = 42161;
        let mut ret = RuntimeEnvironment {
            chain_id,
            l1_inbox: vec![],
            current_block_num: blocknum,
            current_timestamp: timestamp,
            logs: Vec::new(),
            sends: Vec::new(),
            next_inbox_seq_num: Uint256::zero(),
            caller_seq_nums: HashMap::new(),
            next_id: Uint256::zero(),
            recorder: RtEnvRecorder::new(),
            compressor: TxCompressor::new(),
            charging_policy: charging_policy.clone(),
            num_wallets: 0,
            chain_init_message: RuntimeEnvironment::get_params_bytes(owner, chain_id),
        };

        ret.send_chain_init_message();
        ret
    }

    pub fn send_chain_init_message(&mut self) {
        self.insert_l1_message(
            4,
            Uint256::zero(),
            &self.chain_init_message.clone(),
            None,
            None,
        );
    }

    fn get_params_bytes(owner: Option<Uint256>, chain_id: u64) -> Vec<u8> {
        let mut buf = Vec::new();
        let params_to_set = vec![
            ("ChainOwner", owner.clone().unwrap_or(Uint256::zero())),
            ("ChainID", Uint256::from_u64(chain_id)),
        ];
        for (name, val) in params_to_set {
            buf.extend(&keccak256(name.as_bytes()));
            buf.extend(val.to_bytes_be());
        }

        buf
    }

    pub fn _advance_time(
        &mut self,
        delta_blocks: Uint256,
        delta_timestamp: Option<Uint256>,
        send_heartbeat_message: bool,
    ) {
        self.current_block_num = self.current_block_num.add(&delta_blocks);
        self.current_timestamp = self
            .current_timestamp
            .add(&delta_timestamp.unwrap_or(Uint256::from_u64(13).mul(&delta_blocks)));
        if send_heartbeat_message {
            self.insert_l2_message(Uint256::zero(), &[6u8], false);
        }
    }

    pub fn new_wallet(&mut self) -> Wallet {
        let mut r = StdRng::seed_from_u64(42 + self.num_wallets);
        self.num_wallets = self.num_wallets + 1;
        Wallet::new(&mut r).set_chain_id(self.get_chain_id())
    }

    pub fn get_chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn insert_full_inbox_contents(&mut self, contents: Vec<Value>) {
        self.l1_inbox = contents;
    }

    pub fn insert_l1_message(
        &mut self,
        msg_type: u8,
        sender_addr: Uint256,
        msg: &[u8],
        block_num: Option<Uint256>,
        timestamp: Option<Uint256>,
    ) -> Uint256 {
        let l1_msg = Value::new_tuple(vec![
            Value::Int(Uint256::from_usize(msg_type as usize)),
            Value::Int(block_num.unwrap_or(self.current_block_num.clone())),
            Value::Int(timestamp.unwrap_or(self.current_timestamp.clone())),
            Value::Int(sender_addr),
            Value::Int(self.next_inbox_seq_num.clone()),
            Value::Int(self.get_gas_price()),
            Value::Int(Uint256::from_usize(msg.len())),
            Value::new_buffer(msg.to_vec()),
        ]);
        let msg_id =
            Uint256::avm_hash2(&Uint256::from_u64(self.chain_id), &self.next_inbox_seq_num);
        self.next_inbox_seq_num = self.next_inbox_seq_num.add(&Uint256::one());
        self.l1_inbox.push(l1_msg.clone());
        self.recorder.add_msg(l1_msg);

        msg_id
    }

    pub fn _insert_retryable_tx_message(
        &mut self,
        sender: Uint256,
        destination: Uint256,
        callvalue: Uint256,
        deposit: Uint256,
        max_submission_cost: Uint256,
        credit_back_address: Uint256,
        beneficiary: Uint256,
        max_gas_immed: Uint256,
        gas_price_immed: Uint256,
        calldata: &[u8],
    ) -> (Uint256, Uint256, Option<Uint256>) {
        let mut msg = vec![];
        msg.extend(destination.to_bytes_be());
        msg.extend(callvalue.to_bytes_be());
        msg.extend(deposit.to_bytes_be());
        msg.extend(max_submission_cost.to_bytes_be());
        msg.extend(credit_back_address.to_bytes_be());
        msg.extend(beneficiary.to_bytes_be());
        msg.extend(max_gas_immed.to_bytes_be());
        msg.extend(gas_price_immed.to_bytes_be());
        msg.extend(Uint256::from_usize(calldata.len()).to_bytes_be());
        msg.extend(calldata);

        let submit_req_id = self.insert_l1_message(9u8, sender, &msg, None, None);
        let mut buf = submit_req_id.to_bytes_be();
        let mut buf2 = buf.clone();
        buf.extend(&[0u8; 32]);
        buf2.extend(Uint256::one().to_bytes_be());
        (
            submit_req_id,
            Uint256::from_bytes(&keccak256(&buf)),
            if max_gas_immed != Uint256::zero() {
                Some(Uint256::from_bytes(&keccak256(&buf2)))
            } else {
                None
            },
        )
    }

    pub fn get_gas_price(&self) -> Uint256 {
        Uint256::_from_gwei(200)
    }

    pub fn insert_l2_message(
        &mut self,
        sender_addr: Uint256,
        msg: &[u8],
        is_buddy_deploy: bool,
    ) -> Uint256 {
        let default_id = self.insert_l1_message(
            if is_buddy_deploy { 5 } else { 3 },
            sender_addr.clone(),
            msg,
            None,
            None,
        );
        if !is_buddy_deploy && (msg[0] == 0) {
            Uint256::avm_hash2(
                &sender_addr,
                &Uint256::avm_hash2(
                    &Uint256::from_u64(self.chain_id),
                    &Uint256::from_bytes(&keccak256(msg)),
                ),
            )
        } else {
            default_id
        }
    }

    pub fn insert_l2_message_with_deposit(&mut self, sender_addr: Uint256, msg: &[u8]) -> Uint256 {
        if (msg[0] != 0u8) && (msg[0] != 1u8) {
            panic!();
        }
        let default_id = self.insert_l1_message(7, sender_addr.clone(), msg, None, None);
        if msg[0] == 0 {
            Uint256::avm_hash2(
                &sender_addr,
                &Uint256::avm_hash2(
                    &Uint256::from_u64(self.chain_id),
                    &hash_bytestack(_bytestack_from_bytes(msg)).unwrap(),
                ),
            )
        } else {
            default_id
        }
    }

    pub fn insert_tx_message(
        &mut self,
        sender_addr: Uint256,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        to_addr: Uint256,
        value: Uint256,
        data: &[u8],
        with_deposit: bool,
    ) -> Uint256 {
        let mut buf = vec![0u8];
        let seq_num = self.get_and_incr_seq_num(&sender_addr.clone());
        buf.extend(max_gas.to_bytes_be());
        buf.extend(gas_price_bid.to_bytes_be());
        buf.extend(seq_num.to_bytes_be());
        buf.extend(to_addr.to_bytes_be());
        buf.extend(value.to_bytes_be());
        buf.extend_from_slice(data);

        if with_deposit {
            self.insert_l2_message_with_deposit(sender_addr.clone(), &buf)
        } else {
            self.insert_l2_message(sender_addr.clone(), &buf, false)
        }
    }

    pub fn insert_tx_message_from_contract(
        &mut self,
        sender_addr: Uint256,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        to_addr: Uint256,
        value: Uint256,
        data: &[u8],
        with_deposit: bool,
    ) -> Uint256 {
        let mut buf = vec![1u8];
        buf.extend(max_gas.to_bytes_be());
        buf.extend(gas_price_bid.to_bytes_be());
        buf.extend(to_addr.to_bytes_be());
        buf.extend(value.to_bytes_be());
        buf.extend_from_slice(data);

        if with_deposit {
            self.insert_l2_message_with_deposit(sender_addr.clone(), &buf)
        } else {
            self.insert_l2_message(sender_addr.clone(), &buf, false)
        }
    }

    pub fn new_batch(&self) -> Vec<u8> {
        vec![3u8]
    }

    pub fn _new_sequencer_batch(&self, delay: Option<(Uint256, Uint256)>) -> Vec<u8> {
        let (delay_blocks, delay_seconds) = delay.unwrap_or((Uint256::one(), Uint256::one()));
        let (release_block_num, release_timestamp) = if (self.current_block_num <= delay_blocks)
            || (self.current_timestamp <= delay_seconds)
        {
            (Uint256::zero(), Uint256::zero())
        } else {
            (
                self.current_block_num.sub(&delay_blocks).unwrap(),
                self.current_timestamp.sub(&delay_seconds).unwrap(),
            )
        };
        let low_order_bnum = release_block_num.trim_to_u64() & 0xffff;
        let low_order_ts = release_timestamp.trim_to_u64() & 0xffffff;
        let ret = vec![
            5u8,
            (low_order_bnum >> 8) as u8,
            (low_order_bnum & 0xff) as u8,
            (low_order_ts >> 16) as u8,
            ((low_order_ts >> 8) & 0xff) as u8,
            (low_order_ts & 0xff) as u8,
        ];
        ret
    }

    pub fn make_compressed_and_signed_l2_message(
        &mut self,
        gas_price: Uint256,
        gas_limit: Uint256,
        to_addr: Uint256,
        value: Uint256,
        calldata: &[u8],
        wallet: &Wallet,
    ) -> (Vec<u8>, Vec<u8>) {
        let sender = Uint256::from_bytes(wallet.address().as_bytes());
        let mut result = vec![7u8, 0xffu8];
        let seq_num = self.get_and_incr_seq_num(&sender);
        result.extend(seq_num.rlp_encode());
        result.extend(gas_price.rlp_encode());
        result.extend(gas_limit.rlp_encode());
        result.extend(self.compressor.compress_address(to_addr.clone()));
        result.extend(self.compressor.compress_token_amount(value.clone()));
        result.extend(calldata);

        let tx_for_signing = TransactionRequest::new()
            .from(sender.to_h160())
            .to(to_addr.to_h160())
            .gas(gas_limit.to_u256())
            .gas_price(gas_price.to_u256())
            .value(value.to_u256())
            .data(calldata.to_vec())
            .nonce(seq_num.to_u256());
        let tx = wallet.sign_transaction(tx_for_signing).unwrap();

        result.extend(Uint256::from_u256(&tx.r).to_bytes_be());
        result.extend(Uint256::from_u256(&tx.s).to_bytes_be());
        result.extend(vec![(tx.v.as_u64() % 2) as u8]);

        (result, keccak256(tx.rlp().as_ref()).to_vec())
    }

    pub fn _make_compressed_tx_for_bls(
        &mut self,
        sender: &Uint256,
        gas_price: Uint256,
        gas_limit: Uint256,
        to_addr: Uint256,
        value: Uint256,
        calldata: &[u8],
    ) -> (Vec<u8>, Vec<u8>) {
        // returns (compressed tx to send, hash to sign)
        let mut result = self.compressor.compress_address(sender.clone());

        let mut buf = vec![0xffu8];
        let seq_num = self.get_and_incr_seq_num(&sender);
        buf.extend(seq_num.rlp_encode());
        buf.extend(gas_price.rlp_encode());
        buf.extend(gas_limit.rlp_encode());
        buf.extend(self.compressor.compress_address(to_addr.clone()));
        buf.extend(self.compressor.compress_token_amount(value.clone()));
        buf.extend(calldata);

        result.extend(Uint256::from_usize(buf.len()).rlp_encode());
        result.extend(buf);

        (
            result,
            TransactionRequest::new()
                .from(sender.to_h160())
                .to(to_addr.to_h160())
                .gas(gas_limit.to_u256())
                .gas_price(gas_price.to_u256())
                .value(value.to_u256())
                .data(calldata.to_vec())
                .nonce(seq_num.to_u256())
                .sighash(Some(self.chain_id))
                .as_bytes()
                .to_vec(),
        )
    }

    pub fn _insert_bls_batch(
        &mut self,
        senders: &[&Uint256],
        msgs: &[Vec<u8>],
        aggregated_sig: &[u8],
        batch_sender: &Uint256,
    ) {
        assert_eq!(senders.len(), msgs.len());
        let mut buf = vec![8u8];
        buf.extend(Uint256::from_usize(senders.len()).rlp_encode());
        assert_eq!(aggregated_sig.len(), 64);
        buf.extend(aggregated_sig);
        for i in 0..senders.len() {
            buf.extend(msgs[i].clone());
        }

        self.insert_l2_message(batch_sender.clone(), &buf, false);
    }

    pub fn _append_compressed_and_signed_tx_message_to_batch(
        &mut self,
        batch: &mut Vec<u8>,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        to_addr: Uint256,
        value: Uint256,
        calldata: Vec<u8>,
        wallet: &Wallet,
    ) -> Vec<u8> {
        let (msg, tx_id_bytes) = self.make_compressed_and_signed_l2_message(
            gas_price_bid,
            max_gas,
            to_addr,
            value,
            &calldata,
            wallet,
        );
        let msg_size: u64 = msg.len().try_into().unwrap();
        let rlp_encoded_len = Uint256::from_u64(msg_size).rlp_encode();
        batch.extend(rlp_encoded_len.clone());
        batch.extend(msg);
        tx_id_bytes
    }

    pub fn insert_batch_message(&mut self, sender_addr: Uint256, batch: &[u8]) {
        self.insert_l2_message(sender_addr, batch, false);
    }

    pub fn _insert_nonmutating_call_message(
        &mut self,
        sender_addr: Uint256,
        to_addr: Uint256,
        max_gas: Uint256,
        data: &[u8],
    ) {
        let mut buf = vec![2u8];
        buf.extend(max_gas.to_bytes_be());
        buf.extend(Uint256::zero().to_bytes_be()); // gas price = 0
        buf.extend(to_addr.to_bytes_be());
        buf.extend_from_slice(data);

        self.insert_l2_message(sender_addr, &buf, false);
    }

    pub fn insert_eth_deposit_message(
        &mut self,
        sender_addr: Uint256,
        payee: Uint256,
        amount: Uint256,
    ) {
        self.insert_tx_message_from_contract(
            sender_addr,
            Uint256::from_u64(100_000_000),
            Uint256::zero(),
            payee,
            amount,
            &[],
            true,
        );
    }

    pub fn get_and_incr_seq_num(&mut self, addr: &Uint256) -> Uint256 {
        let cur_seq_num = match self.caller_seq_nums.get(&addr) {
            Some(sn) => sn.clone(),
            None => Uint256::zero(),
        };
        self.caller_seq_nums
            .insert(addr.clone(), cur_seq_num.add(&Uint256::one()));
        cur_seq_num
    }

    pub fn get_from_inbox(&mut self) -> Option<Value> {
        if self.l1_inbox.is_empty() {
            None
        } else {
            Some(self.l1_inbox.remove(0))
        }
    }

    pub fn peek_at_inbox_head(&mut self) -> Option<Value> {
        if self.l1_inbox.is_empty() {
            None
        } else {
            Some(self.l1_inbox[0].clone())
        }
    }

    pub fn push_log(&mut self, log_item: Value) {
        self.logs.push(log_item.clone());
        self.recorder.add_log(log_item);
    }

    pub fn get_all_raw_logs(&self) -> Vec<Value> {
        self.logs.clone()
    }

    pub fn get_all_receipt_logs(&self) -> Vec<ArbosReceipt> {
        self.logs
            .clone()
            .into_iter()
            .map(|log| ArbosReceipt::new(log))
            .filter(|r| r.is_some())
            .map(|r| r.unwrap())
            .collect()
    }

    pub fn _get_all_block_summary_logs(&self) -> Vec<_ArbosBlockSummaryLog> {
        self.logs
            .clone()
            .into_iter()
            .map(|log| _ArbosBlockSummaryLog::_new(log))
            .filter(|r| r.is_some())
            .map(|r| r.unwrap())
            .collect()
    }

    pub fn push_send(&mut self, size: Uint256, buf: Buffer) {
        let contents = buf.as_bytes(size.to_usize().unwrap());
        self.sends.push(contents.clone());
        self.recorder.add_send(contents);
    }

    pub fn get_all_sends(&self) -> Vec<Vec<u8>> {
        self.logs
            .clone()
            .into_iter()
            .map(|log| get_send_contents(log))
            .filter(|r| r.is_some())
            .map(|r| r.unwrap())
            .collect()
    }

    pub fn _get_last_send(&self) -> Option<Vec<u8>> {
        let sends = self.get_all_sends();
        if sends.len() == 0 {
            None
        } else {
            Some(sends[0].clone())
        }
    }
}

impl Default for RuntimeEnvironment {
    fn default() -> Self {
        RuntimeEnvironment::new(None)
    }
}

fn get_send_contents(log: Value) -> Option<Vec<u8>> {
    if let Value::Tuple(tup) = log {
        if let Value::Int(kind) = &tup[0] {
            if kind == &Uint256::from_u64(2) {
                let sz = if let Value::Int(usz) = &tup[3] {
                    usz.to_usize().unwrap()
                } else {
                    panic!()
                };
                let buf = if let Value::Buffer(buf) = &tup[4] {
                    buf
                } else {
                    panic!()
                };
                Some(buf.as_bytes(sz))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

// TxCompressor assumes that all client traffic uses it.
// For example, it assumes nobody else affects ArbOS's address compression table.
// This is fine for testing but wouldn't work in a less controlled setting.
#[derive(Debug, Clone)]
pub struct TxCompressor {
    address_map: HashMap<Vec<u8>, Vec<u8>>,
    next_index: u64,
}

impl TxCompressor {
    pub fn new() -> Self {
        TxCompressor {
            address_map: HashMap::new(),
            next_index: 1,
        }
    }

    pub fn compress_address(&mut self, addr: Uint256) -> Vec<u8> {
        if let Some(rlp_bytes) = self.address_map.get(&addr.to_bytes_be()) {
            rlp_bytes.clone()
        } else {
            let addr_bytes = addr.to_bytes_be();
            self.address_map.insert(
                addr_bytes.clone(),
                Uint256::from_u64(self.next_index).rlp_encode(),
            );
            self.next_index = 1 + self.next_index;
            let mut ret = vec![148u8];
            ret.extend(addr_bytes[12..32].to_vec());
            ret
        }
    }

    pub fn compress_token_amount(&self, amt: Uint256) -> Vec<u8> {
        generic_compress_token_amount(amt)
    }
}

pub fn generic_compress_token_amount(mut amt: Uint256) -> Vec<u8> {
    if amt.is_zero() {
        amt.rlp_encode()
    } else {
        let mut num_zeroes = 0;
        let ten = Uint256::from_u64(10);
        loop {
            if amt.modulo(&ten).unwrap().is_zero() {
                num_zeroes = 1 + num_zeroes;
                amt = amt.div(&ten).unwrap();
            } else {
                let mut result = amt.rlp_encode();
                result.extend(vec![num_zeroes as u8]);
                return result;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArbosReceipt {
    request: Value,
    request_id: Uint256,
    return_code: Uint256,
    return_data: Vec<u8>,
    evm_logs: Vec<EvmLog>,
    gas_used: Uint256,
    gas_price_wei: Uint256,
    pub provenance: ArbosRequestProvenance,
    gas_so_far: Uint256,     // gas used so far in L1 block, including this tx
    index_in_block: Uint256, // index of this tx in L1 block
    logs_so_far: Uint256,    // EVM logs emitted so far in L1 block, NOT including this tx
    fee_stats: Vec<Vec<Uint256>>,
}

#[derive(Clone, Debug)]
pub struct ArbosRequestProvenance {
    l1_sequence_num: Uint256,
    parent_request_id: Option<Uint256>,
    index_in_parent: Option<Uint256>,
}

impl ArbosReceipt {
    pub fn new(arbos_log: Value) -> Option<Self> {
        if let Value::Tuple(tup) = arbos_log {
            if !(tup[0] == Value::Int(Uint256::zero())) {
                return None;
            }
            let (return_code, return_data, evm_logs) =
                ArbosReceipt::unpack_return_info(&tup[2]).unwrap();
            let (gas_used, gas_price_wei) = ArbosReceipt::unpack_gas_info(&tup[3]).unwrap();
            let (gas_so_far, index_in_block, logs_so_far) =
                ArbosReceipt::unpack_cumulative_info(&tup[4]).unwrap();
            Some(ArbosReceipt {
                request: tup[1].clone(),
                request_id: if let Value::Tuple(subtup) = &tup[1] {
                    if let Value::Int(ui) = &subtup[5] {
                        ui.clone()
                    } else {
                        panic!()
                    }
                } else {
                    panic!();
                },
                return_code,
                return_data,
                evm_logs: EvmLog::new_vec(evm_logs),
                gas_used,
                gas_price_wei,
                provenance: if let Value::Tuple(stup) = &tup[1] {
                    if let Value::Tuple(subtup1) = &stup[7] {
                        if let Value::Tuple(subtup) = &subtup1[0] {
                            ArbosRequestProvenance {
                                l1_sequence_num: if let Value::Int(ui) = &subtup[0] {
                                    ui.clone()
                                } else {
                                    panic!();
                                },
                                parent_request_id: if let Value::Int(ui) = &subtup[1] {
                                    if ui.is_zero() {
                                        Some(ui.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    panic!();
                                },
                                index_in_parent: if let Value::Int(ui) = &subtup[2] {
                                    if ui.is_zero() {
                                        Some(ui.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    panic!();
                                },
                            }
                        } else {
                            panic!();
                        }
                    } else {
                        panic!();
                    }
                } else {
                    panic!();
                },
                gas_so_far,
                index_in_block,
                logs_so_far,
                fee_stats: if let Value::Tuple(t2) = &tup[5] {
                    vec![
                        if let Value::Tuple(t3) = &t2[0] {
                            t3.iter()
                                .map(|v| {
                                    if let Value::Int(ui) = v {
                                        ui.clone()
                                    } else {
                                        panic!()
                                    }
                                })
                                .collect()
                        } else {
                            panic!()
                        },
                        if let Value::Tuple(t3) = &t2[1] {
                            t3.iter()
                                .map(|v| {
                                    if let Value::Int(ui) = v {
                                        ui.clone()
                                    } else {
                                        panic!()
                                    }
                                })
                                .collect()
                        } else {
                            panic!()
                        },
                        if let Value::Tuple(t3) = &t2[2] {
                            t3.iter()
                                .map(|v| {
                                    if let Value::Int(ui) = v {
                                        ui.clone()
                                    } else {
                                        panic!()
                                    }
                                })
                                .collect()
                        } else {
                            panic!()
                        },
                    ]
                } else {
                    panic!()
                },
            })
        } else {
            panic!("ArbOS log item was not a Tuple");
        }
    }

    fn unpack_return_info(val: &Value) -> Option<(Uint256, Vec<u8>, Value)> {
        if let Value::Tuple(tup) = val {
            let return_code = if let Value::Int(ui) = &tup[0] {
                ui
            } else {
                return None;
            };
            let return_data = size_buffer_tuple_to_bytes(&tup[1])?;
            Some((return_code.clone(), return_data, tup[2].clone()))
        } else {
            None
        }
    }

    fn unpack_gas_info(val: &Value) -> Option<(Uint256, Uint256)> {
        if let Value::Tuple(tup) = val {
            Some((
                if let Value::Int(ui) = &tup[0] {
                    ui.clone()
                } else {
                    return None;
                },
                if let Value::Int(ui) = &tup[1] {
                    ui.clone()
                } else {
                    return None;
                },
            ))
        } else {
            None
        }
    }

    fn unpack_cumulative_info(val: &Value) -> Option<(Uint256, Uint256, Uint256)> {
        if let Value::Tuple(tup) = val {
            Some((
                if let Value::Int(ui) = &tup[0] {
                    ui.clone()
                } else {
                    return None;
                },
                if let Value::Int(ui) = &tup[1] {
                    ui.clone()
                } else {
                    return None;
                },
                if let Value::Int(ui) = &tup[2] {
                    ui.clone()
                } else {
                    return None;
                },
            ))
        } else {
            None
        }
    }

    pub fn get_request(&self) -> Value {
        self.request.clone()
    }

    pub fn get_request_id(&self) -> Uint256 {
        self.request_id.clone()
    }

    pub fn _get_block_number(&self) -> Uint256 {
        if let Value::Tuple(tup) = self.get_request() {
            if let Value::Int(bn) = &tup[1] {
                return bn.clone();
            }
        }
        panic!("Malformed request info in tx receipt");
    }

    pub fn get_return_code(&self) -> Uint256 {
        self.return_code.clone()
    }

    pub fn _get_return_code_text(&self) -> String {
        match self.get_return_code().to_u64().unwrap() {
            0 => "success",
            1 => "transaction reverted",
            2 => "dropped due to L2 congestion",
            3 => "insufficient funds for ArbGas",
            4 => "insufficient balance for callvalue",
            5 => "bad sequence number",
            6 => "message format error",
            7 => "cannot deploy at address",
            8 => "exceeded tx gas limit",
            9 => "below minimum ArbGas for contract tx",
            _ => "unknown error",
        }
        .to_string()
    }

    pub fn succeeded(&self) -> bool {
        self.get_return_code() == Uint256::zero()
    }

    pub fn get_return_data(&self) -> Vec<u8> {
        self.return_data.clone()
    }

    pub fn _get_evm_logs(&self) -> Vec<EvmLog> {
        self.evm_logs.clone()
    }

    pub fn get_gas_used(&self) -> Uint256 {
        self.gas_used.clone()
    }

    pub fn get_gas_used_so_far(&self) -> Uint256 {
        self.gas_so_far.clone()
    }

    pub fn _get_fee_stats(&self) -> Vec<Vec<Uint256>> {
        self.fee_stats.clone()
    }
}

fn size_buffer_tuple_to_bytes(val: &Value) -> Option<Vec<u8>> {
    if let Value::Tuple(tup) = val {
        if let (Value::Int(usz), Value::Buffer(buf)) = (&tup[0], &tup[1]) {
            Some(buf.as_bytes(usz.to_usize().unwrap()))
        } else {
            println!("sizebuffertuple got {}", val);
            None
        }
    } else {
        println!("sizebuffertuple got {}", val);
        None
    }
}

pub struct _ArbosBlockSummaryLog {
    pub block_num: Uint256,
    pub timestamp: Uint256,
    pub gas_limit: Uint256,
    stats_this_block: Arc<Vec<Value>>,
    stats_all_time: Arc<Vec<Value>>,
    gas_summary: _BlockGasAccountingSummary,
}

impl _ArbosBlockSummaryLog {
    pub fn _new(arbos_log: Value) -> Option<Self> {
        if let Value::Tuple(tup) = arbos_log {
            if tup[0] != Value::Int(Uint256::one()) {
                return None;
            }
            let block_num = if let Value::Int(bn) = &tup[1] {
                bn
            } else {
                return None;
            };
            let timestamp = if let Value::Int(ts) = &tup[2] {
                ts
            } else {
                return None;
            };
            let gas_limit = if let Value::Int(gl) = &tup[3] {
                gl
            } else {
                return None;
            };
            let stats_this_block = if let Value::Tuple(t2) = &tup[4] {
                t2
            } else {
                return None;
            };
            let stats_all_time = if let Value::Tuple(t2) = &tup[5] {
                t2
            } else {
                return None;
            };
            let gas_summary = if let Value::Tuple(t2) = &tup[6] {
                t2
            } else {
                return None;
            };
            Some(_ArbosBlockSummaryLog {
                block_num: block_num.clone(),
                timestamp: timestamp.clone(),
                gas_limit: gas_limit.clone(),
                stats_this_block: stats_this_block.clone(),
                stats_all_time: stats_all_time.clone(),
                gas_summary: _BlockGasAccountingSummary::_new(gas_summary.to_vec()),
            })
        } else {
            None
        }
    }
}

pub struct _BlockGasAccountingSummary {
    gas_price_estimate: Uint256,
    gas_pool: Uint256,
    wei_pool: Uint256,
    wei_shortfall: Uint256,
    total_wei_paid_to_validators: Uint256,
    payout_address: Uint256,
}

impl _BlockGasAccountingSummary {
    pub fn _new(tup: Vec<Value>) -> Self {
        _BlockGasAccountingSummary {
            gas_price_estimate: if let Value::Int(ui) = &tup[0] {
                ui.clone()
            } else {
                panic!();
            },
            gas_pool: if let Value::Int(ui) = &tup[1] {
                ui.clone()
            } else {
                panic!();
            },
            wei_pool: if let Value::Int(ui) = &tup[2] {
                ui.clone()
            } else {
                panic!();
            },
            wei_shortfall: if let Value::Int(ui) = &tup[3] {
                ui.clone()
            } else {
                panic!();
            },
            total_wei_paid_to_validators: if let Value::Int(ui) = &tup[4] {
                ui.clone()
            } else {
                panic!();
            },
            payout_address: if let Value::Int(ui) = &tup[5] {
                ui.clone()
            } else {
                panic!();
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvmLog {
    pub addr: Uint256,
    pub data: Vec<u8>,
    pub vals: Vec<Uint256>,
}

impl EvmLog {
    pub fn new(val: Value) -> Self {
        if let Value::Tuple(tup) = val {
            EvmLog {
                addr: if let Value::Int(ui) = &tup[0] {
                    ui.clone()
                } else {
                    panic!()
                },
                data: size_buffer_tuple_to_bytes(&tup[1]).unwrap(),
                vals: tup[2..]
                    .iter()
                    .map(|v| {
                        if let Value::Int(ui) = v {
                            ui.clone()
                        } else {
                            panic!()
                        }
                    })
                    .collect(),
            }
        } else {
            panic!("invalid EVM log format");
        }
    }

    pub fn new_vec(val: Value) -> Vec<Self> {
        if let Value::Tuple(tup) = val {
            if tup.len() == 0 {
                vec![]
            } else {
                let mut rest = EvmLog::new_vec(tup[1].clone());
                let last = EvmLog::new(tup[0].clone());
                rest.push(last);
                rest
            }
        } else {
            panic!()
        }
    }
}

pub fn _bytestack_from_bytes(b: &[u8]) -> Value {
    Value::new_tuple(vec![
        Value::Int(Uint256::from_usize(b.len())),
        _bytestack_from_bytes_2(b, Value::none()),
    ])
}

fn _bytestack_from_bytes_2(b: &[u8], so_far: Value) -> Value {
    let size = b.len();
    if size > 32 {
        _bytestack_from_bytes_2(
            &b[32..],
            Value::new_tuple(vec![_bytestack_build_uint(&b[..32]), so_far]),
        )
    } else {
        Value::new_tuple(vec![_bytestack_build_uint(b), so_far])
    }
}

fn _bytestack_build_uint(b: &[u8]) -> Value {
    let mut ui = Uint256::zero();
    for j in (0..32) {
        if j < b.len() {
            ui = ui
                .mul(&Uint256::from_usize(256))
                .add(&Uint256::from_usize(b[j] as usize));
        } else {
            ui = ui.mul(&Uint256::from_usize(256));
        }
    }
    Value::Int(ui)
}

pub fn hash_bytestack(bs: Value) -> Option<Uint256> {
    if let Value::Tuple(tup) = bs {
        if let Value::Int(ui) = &tup[0] {
            let mut acc: Uint256 = ui.clone();
            let mut pair = &tup[1];
            while (!(*pair == Value::none())) {
                if let Value::Tuple(tup2) = pair {
                    if let Value::Int(ui2) = &tup2[0] {
                        acc = Uint256::avm_hash2(&acc, &ui2);
                        pair = &tup2[1];
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            Some(acc)
        } else {
            None
        }
    } else {
        None
    }
}

#[test]
fn test_hash_bytestack() {
    let buf = hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142").unwrap();
    let h = hash_bytestack(_bytestack_from_bytes(&buf)).unwrap();
    assert_eq!(
        h,
        Uint256::from_string_hex(
            "4fc384a19926e9ff7ec8f2376a0d146dc273031df1db4d133236d209700e4780"
        )
        .unwrap()
    );
}

pub fn _bytes_from_bytestack(bs: Value) -> Option<Vec<u8>> {
    if let Value::Tuple(tup) = bs {
        if let Value::Int(ui) = &tup[0] {
            if let Some(nbytes) = ui.to_usize() {
                return _bytes_from_bytestack_2(tup[1].clone(), nbytes);
            }
        }
    }
    None
}

fn _bytes_from_bytestack_2(cell: Value, nbytes: usize) -> Option<Vec<u8>> {
    if nbytes == 0 {
        Some(vec![])
    } else if let Value::Tuple(tup) = cell {
        assert_eq!((tup.len(), nbytes), (2, nbytes));
        if let Value::Int(mut int_val) = tup[0].clone() {
            let _256 = Uint256::from_usize(256);
            if (nbytes % 32) == 0 {
                let mut sub_arr = match _bytes_from_bytestack_2(tup[1].clone(), nbytes - 32) {
                    Some(arr) => arr,
                    None => {
                        return None;
                    }
                };
                let mut this_arr = vec![0u8; 32];
                for i in 0..32 {
                    let rem = int_val.modulo(&_256).unwrap().to_usize().unwrap(); // safe because denom != 0 and result fits in usize
                    this_arr[31 - i] = rem as u8;
                    int_val = int_val.div(&_256).unwrap(); // safe because denom != 0
                }
                sub_arr.append(&mut this_arr);
                Some(sub_arr)
            } else {
                let mut sub_arr = match _bytes_from_bytestack_2(tup[1].clone(), 32 * (nbytes / 32))
                {
                    Some(arr) => arr,
                    None => {
                        return None;
                    }
                };
                let this_size = nbytes % 32;
                let mut this_arr = vec![0u8; this_size];
                for _ in 0..(32 - this_size) {
                    int_val = int_val.div(&_256).unwrap(); // safe because denom != 0
                }
                for i in 0..this_size {
                    let rem = int_val.modulo(&_256).unwrap().to_usize().unwrap(); // safe because denom != 0 and result fits in usize
                    this_arr[this_size - 1 - i] = rem as u8;
                    int_val = int_val.div(&_256).unwrap(); // safe because denom != 0
                }
                sub_arr.append(&mut this_arr);
                Some(sub_arr)
            }
        } else {
            None
        }
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtEnvRecorder {
    format_version: u64,
    inbox: Vec<Value>,
    logs: Vec<Value>,
    sends: Vec<Vec<u8>>,
    total_gas: u64,
}

impl RtEnvRecorder {
    fn new() -> Self {
        RtEnvRecorder {
            format_version: 1,
            inbox: vec![],
            logs: Vec::new(),
            sends: Vec::new(),
            total_gas: 0,
        }
    }

    fn add_msg(&mut self, msg: Value) {
        self.inbox.push(msg);
    }

    fn add_log(&mut self, log_item: Value) {
        self.logs.push(log_item);
    }

    fn add_send(&mut self, send_item: Vec<u8>) {
        self.sends.push(send_item);
    }

    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_file(&mut self, path: &Path, total_gas: u64) -> Result<(), io::Error> {
        self.total_gas = total_gas;
        let mut file = File::create(path).map(|f| Box::new(f) as Box<dyn io::Write>)?;
        writeln!(file, "{}", self.to_json_string()?)
    }

    pub fn replay_and_compare(
        &self,
        require_same_gas: bool,
        debug: bool,
        profiler_mode: ProfilerMode,
        trace_file: Option<&str>,
    ) -> bool {
        // returns true iff result matches
        let mut rt_env = RuntimeEnvironment::default();
        rt_env.insert_full_inbox_contents(self.inbox.clone());
        let mut machine = load_from_file_and_env(Path::new("arb_os/arbos.mexe"), rt_env);
        if let Some(trace_file_name) = trace_file {
            machine.add_trace_writer(trace_file_name);
        }
        machine.start_at_zero();
        if debug {
            let _ = machine.debug(None);
        } else if (profiler_mode != ProfilerMode::Never) {
            let profile_data = machine.profile_gen(vec![], profiler_mode);
            profile_data.profiler_session();
        } else {
            let _ = machine.run(None);
        }
        let logs_expected = if require_same_gas {
            self.logs.clone()
        } else {
            self.logs
                .clone()
                .into_iter()
                .filter_map(strip_var_from_log)
                .collect()
        };
        let logs_seen = if require_same_gas {
            machine.runtime_env.recorder.logs.clone()
        } else {
            machine
                .runtime_env
                .recorder
                .logs
                .clone()
                .into_iter()
                .filter_map(strip_var_from_log)
                .collect()
        };
        if !(logs_expected == logs_seen) {
            print_output_differences("log", machine.runtime_env.recorder.logs, self.logs.clone());
            return false;
        }
        if !(self.sends == machine.runtime_env.recorder.sends) {
            print_output_differences_bytevec(
                "send",
                machine.runtime_env.recorder.sends,
                self.sends.clone(),
            );
            return false;
        }

        return true;
    }
}

fn strip_var_from_log(log: Value) -> Option<Value> {
    // strip from a log item all info that might legitimately vary as ArbOS evolves (e.g. gas usage)
    if let Value::Tuple(tup) = log.clone() {
        if let Value::Int(item_type) = tup[0].clone() {
            if item_type == Uint256::zero() {
                // Tx receipt log item
                Some(Value::new_tuple(vec![
                    tup[0].clone(),
                    tup[1].clone(),
                    tup[2].clone(),
                    tup[3].clone(),
                    // skip tup[3] because it's all about gas usage
                    zero_item_in_tuple(tup[5].clone(), 0),
                ]))
            } else if item_type == Uint256::one() {
                // block summary log item
                Some(Value::new_tuple(vec![
                    tup[0].clone(),
                    tup[1].clone(),
                    tup[2].clone(),
                    tup[3].clone(),
                    // skip tup[4] thru tup[5] because they're all about gas usage
                    tup[6].clone(),
                ]))
            } else if item_type == Uint256::from_u64(2) {
                Some(log)
            } else if item_type == Uint256::from_u64(3) {
                None
            } else {
                panic!("unrecognized log item type {}", item_type);
            }
        } else {
            panic!("log item type is not integer: {}", tup[0]);
        }
    } else {
        panic!("malformed log item");
    }
}

fn zero_item_in_tuple(in_val: Value, index: usize) -> Value {
    if let Value::Tuple(tup) = in_val {
        Value::new_tuple(
            tup.iter()
                .enumerate()
                .map(|(i, v)| {
                    if i == index {
                        Value::Int(Uint256::zero())
                    } else {
                        v.clone()
                    }
                })
                .collect(),
        )
    } else {
        panic!("malformed inner tuple in log item");
    }
}

fn print_output_differences(kind: &str, seen: Vec<Value>, expected: Vec<Value>) {
    if seen.len() != expected.len() {
        println!(
            "{} mismatch: expected {}, got {}",
            kind,
            expected.len(),
            seen.len()
        );
        return;
    } else {
        for i in 0..(seen.len()) {
            if !(seen[i] == expected[i]) {
                println!("{} {} mismatch:", kind, i);
                println!("expected: {}", expected[i]);
                println!("seen: {}", seen[i]);
                return;
            }
        }
    }
}

fn print_output_differences_bytevec(kind: &str, seen: Vec<Vec<u8>>, expected: Vec<Vec<u8>>) {
    if seen.len() != expected.len() {
        println!(
            "{} mismatch: expected {}, got {}",
            kind,
            expected.len(),
            seen.len()
        );
        return;
    } else {
        for i in 0..(seen.len()) {
            if !(seen[i] == expected[i]) {
                println!("{} {} mismatch:", kind, i);
                println!("expected: {:?}", expected[i]);
                println!("seen: {:?}", seen[i]);
                return;
            }
        }
    }
}

pub fn replay_from_testlog_file(
    filename: &str,
    require_same_gas: bool,
    debug: bool,
    profiler_mode: ProfilerMode,
    trace_file: Option<&str>,
) -> std::io::Result<bool> {
    let mut file = File::open(filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // need to be tricky about how we deserialize, to work around serde_json's recursion limit
    let mut deserializer = serde_json::Deserializer::from_str(&contents);
    deserializer.disable_recursion_limit();
    let deserializer = serde_stacker::Deserializer::new(&mut deserializer);
    let json_value = serde_json::Value::deserialize(deserializer).unwrap();
    let res: Result<RtEnvRecorder, serde_json::error::Error> = serde_json::from_value(json_value);

    match res {
        Ok(recorder) => {
            let success =
                recorder.replay_and_compare(require_same_gas, debug, profiler_mode, trace_file);
            println!("{}", if success { "success" } else { "mismatch " });
            Ok(success)
        }
        Err(e) => panic!("json parsing failed: {}", e),
    }
}

#[test]
fn test_rust_bytestacks() {
    let before =
        "The quick brown fox jumped over the lazy dog. Lorem ipsum and all that.".as_bytes();
    let bs = _bytestack_from_bytes(before);
    let after = _bytes_from_bytestack(bs);
    assert_eq!(after, Some(before.to_vec()));
}
