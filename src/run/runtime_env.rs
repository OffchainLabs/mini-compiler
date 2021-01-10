/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::mavm::{Buffer, Value};
use crate::run::{load_from_file, ProfilerMode};
use crate::uint256::Uint256;
use ethers_core::rand::rngs::StdRng;
use ethers_core::rand::SeedableRng;
use ethers_core::types::TransactionRequest;
use ethers_core::utils::keccak256;
use ethers_signers::{Signer, Wallet};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::io::{Cursor, Read};
use std::rc::Rc;
use std::{collections::HashMap, fs::File, io, path::Path};

#[derive(Debug, Clone)]
pub struct RuntimeEnvironment {
    pub chain_id: u64,
    pub l1_inbox: Vec<Buffer>,
    pub current_block_num: Uint256,
    pub current_timestamp: Uint256,
    pub logs: Vec<Vec<u8>>,
    pub sends: Vec<Vec<u8>>,
    pub next_inbox_seq_num: Uint256,
    pub caller_seq_nums: HashMap<Uint256, Uint256>,
    next_id: Uint256, // used to assign unique (but artificial) txids to messages
    pub recorder: RtEnvRecorder,
    compressor: TxCompressor,
    charging_policy: Option<(Uint256, Uint256, Uint256)>,
    num_wallets: u64,
}

impl RuntimeEnvironment {
    pub fn new(
        chain_address: Uint256,
        charging_policy: Option<(Uint256, Uint256, Uint256)>,
    ) -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            chain_address,
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            charging_policy,
            None,
            None,
        )
    }

    pub fn _new_options(
        chain_address: Uint256,
        sequencer_info: Option<(Uint256, Uint256, Uint256)>,
    ) -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            chain_address,
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            None,
            sequencer_info,
            None,
        )
    }

    pub fn _new_with_owner(chain_address: Uint256, owner: Option<Uint256>) -> Self {
        RuntimeEnvironment::new_with_blocknum_timestamp(
            chain_address,
            Uint256::from_u64(100_000),
            Uint256::from_u64(10_000_000),
            None,
            None,
            owner,
        )
    }

    pub fn new_with_blocknum_timestamp(
        chain_address: Uint256,
        blocknum: Uint256,
        timestamp: Uint256,
        charging_policy: Option<(Uint256, Uint256, Uint256)>,
        sequencer_info: Option<(Uint256, Uint256, Uint256)>,
        owner: Option<Uint256>,
    ) -> Self {
        let mut ret = RuntimeEnvironment {
            chain_id: chain_address.trim_to_u64() & 0xffffffffffff, // truncate to 48 bits
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
        };

        ret.insert_l1_message(
            4,
            chain_address,
            &RuntimeEnvironment::get_params_bytes(charging_policy, sequencer_info, owner),
        );
        ret
    }

    fn get_params_bytes(
        charging_policy: Option<(Uint256, Uint256, Uint256)>,
        sequencer_info: Option<(Uint256, Uint256, Uint256)>,
        owner: Option<Uint256>,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend(Uint256::from_u64(3 * 60 * 60 * 1000).to_bytes_be()); // grace period in ticks
        buf.extend(Uint256::from_u64(100_000_000 / 1000).to_bytes_be()); // arbgas speed limit per tick
        buf.extend(Uint256::from_u64(10_000_000_000).to_bytes_be()); // max execution steps
        buf.extend(Uint256::from_u64(1000).to_bytes_be()); // base stake amount in wei
        buf.extend(Uint256::zero().to_bytes_be()); // staking token address (zero means ETH)
        buf.extend(owner.clone().unwrap_or(Uint256::zero()).to_bytes_be()); // owner address

        if let Some((base_gas_price, storage_charge, pay_fees_to)) = charging_policy.clone() {
            buf.extend(&[0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 2u8]); // option ID = 2
            buf.extend(&[0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 96u8]); // option payload size = 96 bytes
            buf.extend(base_gas_price.to_bytes_be());
            buf.extend(storage_charge.to_bytes_be());
            buf.extend(pay_fees_to.to_bytes_be());
        }

        buf.extend(owner.unwrap_or(Uint256::zero()).to_bytes_be()); // owner address

        if let Some((seq_addr, delay_blocks, delay_time)) = sequencer_info {
            buf.extend(&[0u8; 8]);
            buf.extend(&96u64.to_be_bytes());
            buf.extend(seq_addr.to_bytes_be());
            buf.extend(delay_blocks.to_bytes_be());
            buf.extend(delay_time.to_bytes_be());
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

    pub fn insert_full_inbox_contents(&mut self, contents: Vec<Buffer>) {
        self.l1_inbox = contents;
    }

    pub fn insert_l1_message(&mut self, msg_type: u8, sender_addr: Uint256, msg: &[u8]) -> Uint256 {
        let mut l1_msg = Uint256::from_u64(msg_type as u64).to_bytes_be();
        l1_msg.extend(self.current_block_num.to_bytes_be());
        l1_msg.extend(self.current_timestamp.to_bytes_be());
        l1_msg.extend(sender_addr.to_bytes_be());
        l1_msg.extend(self.next_inbox_seq_num.to_bytes_be());
        l1_msg.extend(Uint256::from_usize(msg.len()).to_bytes_be());
        l1_msg.extend(msg);
        let msg_id =
            Uint256::avm_hash2(&Uint256::from_u64(self.chain_id), &self.next_inbox_seq_num);
        self.next_inbox_seq_num = self.next_inbox_seq_num.add(&Uint256::one());

        let buf = Buffer::new(l1_msg.clone());
        self.l1_inbox.push(buf.clone());
        self.recorder.add_msg(buf);

        msg_id
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
        );
        if msg[0] == 0 {
            Uint256::avm_hash2(
                &sender_addr,
                &Uint256::avm_hash2(
                    &Uint256::from_u64(self.chain_id),
                    &hash_bytestack(bytestack_from_bytes(msg)).unwrap(),
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
        let default_id = self.insert_l1_message(7, sender_addr.clone(), msg);
        if msg[0] == 0 {
            Uint256::avm_hash2(
                &sender_addr,
                &Uint256::avm_hash2(
                    &Uint256::from_u64(self.chain_id),
                    &hash_bytestack(bytestack_from_bytes(msg)).unwrap(),
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

    pub fn insert_buddy_deploy_message(
        &mut self,
        sender_addr: Uint256,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        value: Uint256,
        data: &[u8],
    ) -> Uint256 {
        let mut buf = vec![1u8];
        buf.extend(max_gas.to_bytes_be());
        buf.extend(gas_price_bid.to_bytes_be());
        buf.extend(Uint256::zero().to_bytes_be()); // destination address 0
        buf.extend(value.to_bytes_be());
        buf.extend_from_slice(data);

        self.insert_l2_message(sender_addr.clone(), &buf, true)
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

    pub fn make_signed_l2_message(
        &mut self,
        sender_addr: Uint256,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        to_addr: Uint256,
        value: Uint256,
        calldata: Vec<u8>,
        wallet: &Wallet,
    ) -> (Vec<u8>, Vec<u8>) {
        let seq_num = self.get_and_incr_seq_num(&sender_addr);
        let tx_for_signing = TransactionRequest::new()
            .from(sender_addr.to_h160())
            .to(to_addr.to_h160())
            .gas(max_gas.to_u256())
            .gas_price(gas_price_bid.to_u256())
            .value(value.to_u256())
            .data(calldata)
            .nonce(seq_num.to_u256());
        let tx = wallet.sign_transaction(tx_for_signing).unwrap();

        let rlp_buf = tx.rlp().as_ref().to_vec();
        let mut buf = vec![4u8];
        buf.extend(rlp_buf.clone());
        (buf, keccak256(&rlp_buf).to_vec())
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

    pub fn append_signed_tx_message_to_batch(
        &mut self,
        batch: &mut Vec<u8>,
        sender_addr: Uint256,
        max_gas: Uint256,
        gas_price_bid: Uint256,
        to_addr: Uint256,
        value: Uint256,
        calldata: Vec<u8>,
        wallet: &Wallet,
    ) -> Vec<u8> {
        let (msg, tx_id_bytes) = self.make_signed_l2_message(
            sender_addr,
            max_gas,
            gas_price_bid,
            to_addr,
            value,
            calldata,
            wallet,
        );
        let msg_size: u64 = msg.len().try_into().unwrap();
        let rlp_encoded_len = Uint256::from_u64(msg_size).rlp_encode();
        batch.extend(rlp_encoded_len.clone());
        batch.extend(msg);
        tx_id_bytes
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

    pub fn insert_erc20_deposit_message(
        &mut self,
        sender_addr: Uint256,
        token_addr: Uint256,
        payee: Uint256,
        amount: Uint256,
    ) {
        let mut buf = token_addr.to_bytes_be();
        buf.extend(payee.to_bytes_be());
        buf.extend(amount.to_bytes_be());

        self.insert_l1_message(1, sender_addr, &buf);
    }

    pub fn insert_erc721_deposit_message(
        &mut self,
        sender_addr: Uint256,
        token_addr: Uint256,
        payee: Uint256,
        amount: Uint256,
    ) {
        let mut buf = token_addr.to_bytes_be();
        buf.extend(payee.to_bytes_be());
        buf.extend(amount.to_bytes_be());

        self.insert_l1_message(2, sender_addr, &buf);
    }

    pub fn insert_eth_deposit_message(
        &mut self,
        sender_addr: Uint256,
        payee: Uint256,
        amount: Uint256,
    ) {
        let mut buf = payee.to_bytes_be();
        buf.extend(amount.to_bytes_be());

        self.insert_l1_message(0, sender_addr, &buf);
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

    pub fn get_from_inbox(&mut self) -> Option<Buffer> {
        if self.l1_inbox.is_empty() {
            None
        } else {
            Some(self.l1_inbox.remove(0))
        }
    }

    pub fn peek_at_inbox_head(&mut self) -> Option<Buffer> {
        if self.l1_inbox.is_empty() {
            None
        } else {
            Some(self.l1_inbox[0].clone())
        }
    }

    pub fn push_log(&mut self, size: Uint256, buf: Buffer) {
        let mut res = vec![];
        for i in 0..size.to_usize().unwrap() {
            res.push(buf.read_byte(i));
        }
        self.logs.push(res.clone());
        self.recorder.add_log(res);
    }

    pub fn get_all_raw_logs(&self) -> Vec<Vec<u8>> {
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
        let mut res = vec![];
        for i in 0..size.to_usize().unwrap() {
            res.push(buf.read_byte(i));
        }
        self.sends.push(res.clone());
        self.recorder.add_send(res);
    }

    pub fn get_all_sends(&self) -> Vec<Vec<u8>> {
        self.logs
            .clone()
            .into_iter()
            .map(|log| get_send_from_log(log))
            .filter(|r| r.is_some())
            .map(|r| r.unwrap())
            .collect()
    }
}

pub fn get_send_from_log(log: Vec<u8>) -> Option<Vec<u8>> {
    let mut rd = Cursor::new(log);
    let kind = Uint256::read(&mut rd);
    if kind == Uint256::from_u64(2) {
        let size = Uint256::read(&mut rd).to_usize().unwrap();
        Some(read_bytes(&mut rd, size))
    } else {
        None
    }
}

pub fn get_send_contents(send: Vec<u8>) -> Vec<u8> {
    send
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
}

#[derive(Clone, Debug)]
pub struct ArbosRequestProvenance {
    l1_sequence_num: Uint256,
    parent_request_id: Option<Uint256>,
    index_in_parent: Option<Uint256>,
}

fn read_bytes(cursor: &mut Cursor<Vec<u8>>, num: usize) -> Vec<u8> {
    let mut ret = vec![];
    let mut b = [0u8];
    for _ in 0..num {
        cursor.read(&mut b).unwrap();
        ret.push(b[0]);
    }
    ret
}

impl ArbosReceipt {
    pub fn new(arbos_log: Vec<u8>) -> Option<Self> {
        let mut rd = Cursor::new(arbos_log);

        let log_type = Uint256::read(&mut rd);
        if !log_type.is_zero() {
            return None;
        }

        // read incoming request info
        let l1_type = Uint256::read(&mut rd);
        let l1_blocknum = Uint256::read(&mut rd);
        let l1_timestamp = Uint256::read(&mut rd);
        let l1_sender = Uint256::read(&mut rd);
        let l1_request_id = Uint256::read(&mut rd);
        let l2_message_len = Uint256::read(&mut rd);
        let l2_message = read_bytes(&mut rd, l2_message_len.to_usize().unwrap());
        let l1_request = Value::new_tuple(vec![
            Value::Int(l1_type),
            Value::Int(l1_blocknum),
            Value::Int(l1_timestamp),
            Value::Int(l1_sender),
            Value::Int(l1_request_id.clone()),
            Value::Int(l2_message_len),
            Value::new_buffer(l2_message),
        ]);

        // read tx result info
        let return_code = Uint256::read(&mut rd);
        let return_data_size = Uint256::read(&mut rd);
        let mut return_data = vec![0u8; return_data_size.to_usize().unwrap()];
        rd.read(&mut return_data).unwrap();

        // read EVM logs
        let num_evm_logs = Uint256::read(&mut rd);
        let mut evm_logs = vec![];
        for _ in 0..num_evm_logs.to_usize().unwrap() {
            evm_logs.push(EvmLog::read(&mut rd));
        }

        // read ArbGas info
        let gas_used = Uint256::read(&mut rd);
        let gas_price_wei = Uint256::read(&mut rd);

        // read provenance info
        let l1_sequence_num = Uint256::read(&mut rd);
        let parent_request_id = Uint256::read(&mut rd);
        let index_in_parent = Uint256::read(&mut rd);

        // read cumulative info in block
        let gas_so_far = Uint256::read(&mut rd);
        let index_in_block = Uint256::read(&mut rd);
        let logs_so_far = Uint256::read(&mut rd);

        Some(ArbosReceipt {
            request: l1_request,
            request_id: l1_request_id,
            return_code,
            return_data,
            evm_logs,
            gas_used,
            gas_price_wei,
            provenance: ArbosRequestProvenance {
                l1_sequence_num,
                parent_request_id: if parent_request_id.is_zero() {
                    None
                } else {
                    Some(parent_request_id.clone())
                },
                index_in_parent: if parent_request_id.is_zero() {
                    None
                } else {
                    Some(index_in_parent)
                },
            },
            gas_so_far,
            index_in_block,
            logs_so_far,
        })
    }

    fn _unpack_return_info(val: &Value) -> Option<(Uint256, Vec<u8>, Value)> {
        if let Value::Tuple(tup) = val {
            let return_code = if let Value::Int(ui) = &tup[0] {
                ui
            } else {
                return None;
            };
            let return_data = _bytes_from_bytestack(tup[1].clone())?;
            Some((return_code.clone(), return_data, tup[2].clone()))
        } else {
            None
        }
    }

    fn _unpack_gas_info(val: &Value) -> Option<(Uint256, Uint256)> {
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

    fn _unpack_cumulative_info(val: &Value) -> Option<(Uint256, Uint256, Uint256)> {
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
}

pub struct _ArbosBlockSummaryLog {
    pub block_num: Uint256,
    pub timestamp: Uint256,
    pub gas_limit: Uint256,
    stats_this_block: Rc<Vec<Value>>,
    stats_all_time: Rc<Vec<Value>>,
    gas_summary: _BlockGasAccountingSummary,
}

impl _ArbosBlockSummaryLog {
    pub fn _new(arbos_log: Vec<u8>) -> Option<Self> {
        let mut rd = Cursor::new(arbos_log);
        let log_type = Uint256::read(&mut rd);
        if log_type != Uint256::one() {
            return None;
        }

        let block_num = Uint256::read(&mut rd);
        let timestamp = Uint256::read(&mut rd);
        let gas_limit = Uint256::read(&mut rd);
        let stats_this_block = Rc::new(_read_block_stats(&mut rd));
        let stats_all_time = Rc::new(_read_block_stats(&mut rd));
        let gas_summary = _BlockGasAccountingSummary::_read(&mut rd);
        let _prev_block_num = Uint256::read(&mut rd);

        Some(Self {
            block_num,
            timestamp,
            gas_limit,
            stats_this_block,
            stats_all_time,
            gas_summary,
        })
    }
}

fn _read_block_stats(rd: &mut Cursor<Vec<u8>>) -> Vec<Value> {
    let total_gas_used = Uint256::read(rd);
    let num_tx = Uint256::read(rd);
    let num_evm_logs = Uint256::read(rd);
    let num_logs = Uint256::read(rd);
    let num_sends = Uint256::read(rd);

    vec![total_gas_used, num_tx, num_evm_logs, num_logs, num_sends]
        .iter()
        .map(|u| Value::Int(u.clone()))
        .collect()
}

pub struct _BlockGasAccountingSummary {
    gas_price_estimate: Uint256,
    gas_pool: Uint256,
    signed_net_reserve: Uint256,
    total_wei_paid_to_validators: Uint256,
    payout_address: Uint256,
}

impl _BlockGasAccountingSummary {
    pub fn _read(rd: &mut Cursor<Vec<u8>>) -> Self {
        let gas_price_estimate = Uint256::read(rd);
        let gas_pool = Uint256::read(rd);
        let signed_net_reserve = Uint256::read(rd);
        let total_wei_paid_to_validators = Uint256::read(rd);
        let payout_address = Uint256::read(rd);
        _BlockGasAccountingSummary {
            gas_price_estimate,
            gas_pool,
            signed_net_reserve,
            total_wei_paid_to_validators,
            payout_address,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvmLog {
    addr: Uint256,
    data: Vec<u8>,
    vals: Vec<Uint256>,
}

impl EvmLog {
    pub fn _new(val: Value) -> Self {
        if let Value::Tuple(tup) = val {
            EvmLog {
                addr: if let Value::Int(ui) = &tup[0] {
                    ui.clone()
                } else {
                    panic!()
                },
                data: _bytes_from_bytestack(tup[1].clone()).unwrap(),
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

    pub fn read(rd: &mut Cursor<Vec<u8>>) -> Self {
        let addr = Uint256::read(rd);
        let data_len = Uint256::read(rd).to_usize().unwrap();
        let data = read_bytes(rd, data_len);
        let num_topics = Uint256::read(rd).to_usize().unwrap();
        let mut vals = vec![];
        for _ in 0..num_topics {
            vals.push(Uint256::read(rd));
        }
        EvmLog { addr, data, vals }
    }
}

pub fn bytestack_from_bytes(b: &[u8]) -> Value {
    Value::new_tuple(vec![
        Value::Int(Uint256::from_usize(b.len())),
        bytestack_from_bytes_2(b, Value::none()),
    ])
}

fn bytestack_from_bytes_2(b: &[u8], so_far: Value) -> Value {
    let size = b.len();
    if size > 32 {
        bytestack_from_bytes_2(
            &b[32..],
            Value::new_tuple(vec![bytestack_build_uint(&b[..32]), so_far]),
        )
    } else {
        Value::new_tuple(vec![bytestack_build_uint(b), so_far])
    }
}

fn bytestack_build_uint(b: &[u8]) -> Value {
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
    let h = hash_bytestack(bytestack_from_bytes(&buf)).unwrap();
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
    inbox: Vec<Vec<u8>>,
    logs: Vec<Vec<u8>>,
    sends: Vec<Vec<u8>>,
}

impl RtEnvRecorder {
    fn new() -> Self {
        RtEnvRecorder {
            format_version: 1,
            inbox: vec![],
            logs: Vec::new(),
            sends: Vec::new(),
        }
    }

    fn add_msg(&mut self, msg: Buffer) {
        let size = msg.read_word(5 * 32).to_u64().unwrap();
        let mut buf = vec![];
        for i in 0..size {
            buf.push(msg.read_byte(i as usize));
        }
        self.inbox.push(buf);
    }

    fn add_log(&mut self, log_item: Vec<u8>) {
        self.logs.push(log_item);
    }

    fn add_send(&mut self, send_item: Vec<u8>) {
        self.sends.push(send_item);
    }

    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_file(&self, path: &Path) -> Result<(), io::Error> {
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
        let mut rt_env = RuntimeEnvironment::new(Uint256::from_usize(1111), None);
        rt_env.insert_full_inbox_contents(
            self.inbox.iter().map(|b| Buffer::new(b.to_vec())).collect(),
        );
        let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"), rt_env);
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
                .map(strip_var_from_log)
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
                .map(strip_var_from_log)
                .collect()
        };
        if !(logs_expected == logs_seen) {
            print_output_differences("log", machine.runtime_env.recorder.logs, self.logs.clone());
            return false;
        }
        if !(self.sends == machine.runtime_env.recorder.sends) {
            print_output_differences(
                "send",
                machine.runtime_env.recorder.sends,
                self.sends.clone(),
            );
            return false;
        }
        return true;
    }
}

fn strip_var_from_log(log: Vec<u8>) -> Vec<u8> {
    log
}
/* Replacing this with a no-op, because it would have to work very differently under the new format.
fn strip_var_from_log(log: Value) -> Value {
    // strip from a log item all info that might legitimately vary as ArbOS evolves (e.g. gas usage)
    if let Value::Tuple(tup) = log.clone() {
        if let Value::Int(item_type) = tup[0].clone() {
            if item_type == Uint256::zero() {
                // Tx receipt log item
                Value::new_tuple(vec![
                    tup[0].clone(),
                    tup[1].clone(),
                    tup[2].clone(),
                    // skip tup[3] because it's all about gas usage
                    zero_item_in_tuple(tup[4].clone(), 0),
                ])
            } else if item_type == Uint256::one() {
                // block summary log item
                Value::new_tuple(vec![
                    tup[0].clone(),
                    tup[1].clone(),
                    tup[2].clone(),
                    // skip tup[3] because it's all about gas usage
                    zero_item_in_tuple(tup[4].clone(), 0),
                    zero_item_in_tuple(tup[5].clone(), 0),
                ])
            } else if item_type == Uint256::from_u64(2) {
                log
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
*/

fn _zero_item_in_tuple(in_val: Value, index: usize) -> Value {
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

fn print_output_differences(kind: &str, seen: Vec<Vec<u8>>, expected: Vec<Vec<u8>>) {
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

// used to be a test
fn _logfile_replay_tests() {
    for entry in std::fs::read_dir(Path::new("./replayTests")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        assert_eq!(
            replay_from_testlog_file(
                &("./replayTests/".to_owned() + name.to_str().unwrap()),
                false,
                false,
                ProfilerMode::Never,
                None,
            )
            .unwrap(),
            true
        );
    }
}

#[test]
fn test_rust_bytestacks() {
    let before =
        "The quick brown fox jumped over the lazy dog. Lorem ipsum and all that.".as_bytes();
    let bs = bytestack_from_bytes(before);
    let after = _bytes_from_bytestack(bs);
    assert_eq!(after, Some(before.to_vec()));
}
