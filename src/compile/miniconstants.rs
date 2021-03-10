/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved
 */

//!Creates a fixed list of globally accessible constants.

use crate::uint256::Uint256;
use std::collections::HashMap;

///Creates a fixed list of globally accessible constants.
pub fn init_constant_table() -> HashMap<String, Uint256> {
    let mut ret = HashMap::new();
    for (s, i) in &[
        // addresses of precompiled contracts
        ("Address_ArbSys", 100),
        ("Address_ArbAddressTable", 102),
        ("Address_ArbBLS", 103),
        ("Address_ArbFunctionTable", 104),
        ("Address_ArbosTest", 105),
        ("Address_ArbOwner", 107),
        ("Address_ArbGasInfo", 108),
        ("Address_ArbAggregator", 109),
        // addresses of dummy builtin contracts
        ("Address_ReservedForEthBridge", 200), // reserved for special EthBridge functionality
        // indices of EVM operations
        ("EvmOp_stop", 0),
        ("EvmOp_sha3", 1),
        ("EvmOp_address", 2),
        ("EvmOp_balance", 3),
        ("EvmOp_selfbalance", 4),
        ("EvmOp_origin", 5),
        ("EvmOp_caller", 6),
        ("EvmOp_callvalue", 7),
        ("EvmOp_calldataload", 8),
        ("EvmOp_calldatasize", 9),
        ("EvmOp_calldatacopy", 10),
        ("EvmOp_codesize", 11),
        ("EvmOp_codecopy", 12),
        ("EvmOp_extcodesize", 13),
        ("EvmOp_extcodecopy", 14),
        ("EvmOp_extcodehash", 15),
        ("EvmOp_returndatasize", 16),
        ("EvmOp_returndatacopy", 17),
        ("EvmOp_timestamp", 18),
        ("EvmOp_number", 19),
        ("EvmOp_msize", 20),
        ("EvmOp_mload", 21),
        ("EvmOp_mstore", 22),
        ("EvmOp_mstore8", 23),
        ("EvmOp_sload", 24),
        ("EvmOp_sstore", 25),
        ("EvmOp_getjumpaddr", 26),
        ("EvmOp_msize", 27),
        ("EvmOp_log0", 28),
        ("EvmOp_log1", 29),
        ("EvmOp_log2", 30),
        ("EvmOp_log3", 31),
        ("EvmOp_log4", 32),
        ("EvmOp_call", 33),
        ("EvmOp_callcode", 34),
        ("EvmOp_delegatecall", 35),
        ("EvmOp_staticcall", 36),
        ("EvmOp_revert", 37),
        ("EvmOp_revert_knownPc", 38),
        ("EvmOp_return", 39),
        ("EvmOp_selfdestruct", 40),
        ("EvmOp_create", 41),
        ("EvmOp_create2", 42),
        ("EvmOp_chainId", 43),
        ("EvmOp_gasprice", 44),
        ("EvmOp_gaslimit", 45),
        ("EvmOp_blockhash", 46),
        ("NumEvmOps", 47),
        // AVM instructions
        ("AVM_add", 0x01),
        ("AVM_mul", 0x02),
        ("AVM_sub", 0x03),
        ("AVM_div", 0x04),
        ("AVM_sdiv", 0x05),
        ("AVM_mod", 0x06),
        ("AVM_smod", 0x07),
        ("AVM_addmod", 0x08),
        ("AVM_mulmod", 0x09),
        ("AVM_exp", 0x0a),
        ("AVM_signextend", 0x0b),
        ("AVM_lt", 0x10),
        ("AVM_gt", 0x11),
        ("AVM_slt", 0x12),
        ("AVM_sgt", 0x13),
        ("AVM_eq", 0x14),
        ("AVM_iszero", 0x15),
        ("AVM_and", 0x16),
        ("AVM_or", 0x17),
        ("AVM_xor", 0x18),
        ("AVM_not", 0x19),
        ("AVM_byte", 0x1a),
        ("AVM_shl", 0x1b),
        ("AVM_shr", 0x1c),
        ("AVM_sar", 0x1d),
        ("AVM_hash", 0x20),
        ("AVM_type", 0x21),
        ("AVM_ethhash2", 0x22),
        ("AVM_keccakf", 0x23),
        ("AVM_sha256f", 0x24),
        ("AVM_pop", 0x30),
        ("AVM_spush", 0x31),
        ("AVM_rpush", 0x32),
        ("AVM_rset", 0x33),
        ("AVM_jump", 0x34),
        ("AVM_cjump", 0x35),
        ("AVM_stackempty", 0x36),
        ("AVM_pcpush", 0x37),
        ("AVM_auxpush", 0x38),
        ("AVM_auxpop", 0x39),
        ("AVM_auxstackempty", 0x3a),
        ("AVM_nop", 0x3b),
        ("AVM_errpush", 0x3c),
        ("AVM_errset", 0x3d),
        ("AVM_dup0", 0x40),
        ("AVM_dup1", 0x41),
        ("AVM_dup2", 0x42),
        ("AVM_swap1", 0x43),
        ("AVM_swap2", 0x44),
        ("AVM_tget", 0x50),
        ("AVM_tset", 0x51),
        ("AVM_tlen", 0x52),
        ("AVM_xget", 0x53),
        ("AVM_xset", 0x54),
        ("AVM_breakpoint", 0x60),
        ("AVM_log", 0x61),
        ("AVM_send", 0x70),
        ("AVM_inboxpeek", 0x71),
        ("AVM_inbox", 0x72),
        ("AVM_error", 0x73),
        ("AVM_halt", 0x74),
        ("AVM_setgas", 0x75),
        ("AVM_pushgas", 0x76),
        ("AVM_errcodepoint", 0x77),
        ("AVM_pushinsn", 0x78),
        ("AVM_pushinsnimm", 0x79),
        ("AVM_sideload", 0x7b),
        ("AVM_ecrecover", 0x80),
        ("AVM_ecadd", 0x81),
        ("AVM_ecmul", 0x82),
        ("AVM_ecpairing", 0x83),
        ("AVM_debugprint", 0x90),
        // L1 message types
        ("L1MessageType_ethDeposit", 0),
        ("L1MessageType_L2", 3),
        ("L1MessageType_chainInit", 4),
        // type 5 not used -- previously was for buddy contract deploy
        ("L1MessageType_endOfBlock", 6),
        ("L1MessageType_L2FundedByL1", 7),
        ("L1MessageType_rollupProtocolEvent", 8),
        // L2 message types
        ("L2MessageType_unsignedEOATx", 0),
        ("L2MessageType_unsignedContractTx", 1),
        ("L2MessageType_nonmutatingCall", 2),
        ("L2MessageType_batch", 3),
        ("L2MessageType_signedTx", 4),
        ("L2MessageType_sequencerBatch", 5),
        ("L2MessageType_heartbeat", 6),
        ("L2MessageType_signedCompressedTx", 7),
        ("L2MessageType_blsBatch", 8),
        // rollup protocol event types
        ("ProtoEvent_createNode", 0),
        ("ProtoEvent_confirmNode", 1),
        ("ProtoEvent_rejectNode", 2),
        ("ProtoEvent_newStake", 3),
        ("ProtoEvent_claimNode", 4),
        ("ProtoEvent_debug", 255),
        // tx result codes
        ("TxResultCode_success", 0),
        ("TxResultCode_revert", 1),
        ("TxResultCode_congestion", 2),
        ("TxResultCode_noGasFunds", 3),
        ("TxResultCode_insufficientBalance", 4),
        ("TxResultCode_badSequenceNum", 5),
        ("TxResultCode_formatError", 6),
        ("TxResultCode_cannotDeployAtAddress", 7),
        ("TxResultCode_exceededTxGasLimit", 8),
        ("TxResultCode_unknownFailure", 255),
        // EVM call types
        ("EVMCallType_call", 0),
        ("EVMCallType_callcode", 1),
        ("EVMCallType_delegatecall", 2),
        ("EVMCallType_staticcall", 3),
        ("EVMCallType_constructor", 4),
        // Arbitrum log item types
        ("LogType_txReceipt", 0),
        ("LogType_blockSummary", 1),
        ("LogType_send", 2),
        ("LogType_sendMerkleTree", 3),
        // outgoing message types
        ("SendType_withdrawETH", 0),
        ("SendType_sendTxToL1", 3),
        // type 5 not used -- was previously buddy contract result
        // AVM send types
        ("AVMSendType_batch", 0),
        // chain initialization options
        ("InitOption_setChargingParams", 2),
        ("InitOption_setDefaultAggregator", 3),
        // charging mechanism
        ("Charging_DefaultL1GasPerL2Tx", 3700),
        ("Charging_DefaultL1GasPerCalldataByte", 16),
        ("Charging_DefaultL1GasPerStorage", 2000),
        ("Charging_DefaultArbGasDivisor", 10000),
        ("Charging_AssumedBatchCostL1Gas", 50000),
        ("Charging_GasPoolDepthSeconds", 60),
        // fee customizability
        ("NetFee_defaultRateNumerator", 15),
        ("NetFee_defaultRateDenominator", 100),
        // sequencer constants
        ("Sequencer_maxDelaySeconds", 983040), // 30*128*256
        ("Sequencer_deltaSeconds", 0),
        // pluggable modules
        ("PluggableModuleID_rollupTracker", 0),
        ("PluggableModuleID_precompile_0x05", 1),
        // misc
        ("SecondsPerBlockNumerator", 2),
        ("SecondsPerBlockDenominator", 1),
        ("DefaultSpeedLimitPerSecond", 100_000_000),
        ("DefaultSecondsPerSend", 900), // 15 minutes
        ("Estimate_L1GasCostPerNode", 220000),
        ("Estimate_L1GasPrice", 100 * 1_000_000_000), // 100 gwei
    ] {
        ret.insert(s.to_string(), Uint256::from_u64(*i));
    }

    for (s, u) in &[
        ("Charging_DefaultNetworkFeeRecipient", "0"),
        ("Charging_DefaultCongestionFeeRecipient", "0"),
        (
            "SpecialAccount_gasAccountingReserve",
            // Keccak256 of "Arbitrum gas accounting reserve account"
            "af6cbc19f66dec07f790912226744d744f04b37b666b9343317df33a5114fb96",
        ),
        (
            "BLSSignatureDomainBase",
            // Keccak256 of "Arbitrum BLS signature domain"
            "73a92f91d473214defd5ffa91d036007eb2e6487fffaa551835e988fb24aaa2b",
        ),
        (
            "EVMLogTopicForL2ToL1Send",
            "99ecd3620b54462a4f03f96ee9a3618830bb7ed6baab03d81adad709b22d1322",
        ),
    ] {
        ret.insert(s.to_string(), Uint256::from_string_hex(u).unwrap());
    }
    ret
}
