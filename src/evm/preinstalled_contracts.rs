use super::*;
use crate::compile::miniconstants::init_constant_table;
use crate::run::{load_from_file, Machine, RuntimeEnvironment};
use crate::uint256::Uint256;
use crate::upload::CodeUploader;
use ethers_core::utils::keccak256;
use ethers_signers::{Signer, Wallet};
use std::path::Path;

pub struct _ArbInfo {
    pub contract_abi: AbiForContract,
    debug: bool,
}

impl _ArbInfo {
    pub fn _new(debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbInfo")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(101));
        _ArbInfo {
            contract_abi,
            debug,
        }
    }

    pub fn _get_balance(
        &self,
        machine: &mut Machine,
        addr: &Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::from_u64(1112),
            "getBalance",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[ethabi::ParamType::Uint(256)],
                &receipts[0].get_return_data(),
            )?;
            match return_vals[0] {
                ethabi::Token::Uint(ui) => Ok(Uint256::from_u256(&ui)),
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }
}

pub struct _ArbOwner<'a> {
    pub contract_abi: AbiForContract,
    wallet: &'a Wallet,
    my_address: Uint256,
    debug: bool,
}

impl<'a> _ArbOwner<'a> {
    pub fn _new(wallet: &'a Wallet, debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbOwner")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(107));
        _ArbOwner {
            contract_abi,
            wallet,
            my_address: Uint256::from_bytes(wallet.address().as_bytes()),
            debug,
        }
    }

    pub fn _set_chain_parameter(
        &self,
        machine: &mut Machine,
        param_name: &str,
        value: Uint256,
        force_owner: bool, // force the message to come from address zero, which is an owner
    ) -> Result<(), ethabi::Error> {
        let param_id = _param_id_from_name(param_name);
        let (receipts, _sends) = self.contract_abi._call_function_from_contract(
            if force_owner {
                Uint256::zero()
            } else {
                Uint256::from_u64(42894528) // any old address
            },
            "setChainParameter",
            &[
                ethabi::Token::Uint(param_id.to_u256()),
                ethabi::Token::Uint(value.to_u256()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _get_chain_parameter(
        &self,
        machine: &mut Machine,
        param_name: &str,
        force_owner: bool, // force the message to come from address zero, which is an owner
    ) -> Result<Uint256, ethabi::Error> {
        let param_id = _param_id_from_name(param_name);
        let (receipts, _sends) = self.contract_abi._call_function_from_contract(
            if force_owner {
                Uint256::zero()
            } else {
                Uint256::from_u64(980509782534089) // any old address
            },
            "getChainParameter",
            &[ethabi::Token::Uint(param_id.to_u256())],
            machine,
            Uint256::zero(),
            false,
        )?;
        let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
        let num_sends_before = machine.runtime_env.get_all_sends().len();
        let _arbgas_used = if self.debug {
            machine.debug(None)
        } else {
            machine.run(None)
        };
        let logs = machine.runtime_env.get_all_receipt_logs();
        let sends = machine.runtime_env.get_all_sends();

        if (logs.len() != num_logs_before + 2) || (sends.len() != num_sends_before) {
            return Err(ethabi::Error::from("wrong number of receipts or sends"));
        }

        if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _set_gas_accounting_params(
        &self,
        machine: &mut Machine,
        speed_limit: Uint256,
        gas_pool_max: Uint256,
        tx_gas_limit: Uint256,
    ) -> Result<(), ethabi::Error> {
        self._set_chain_parameter(machine, "SpeedLimitPerSecond", speed_limit, true)?;
        self._set_chain_parameter(machine, "GasPoolMax", gas_pool_max, true)?;
        self._set_chain_parameter(machine, "TxGasLimit", tx_gas_limit, true)
    }

    pub fn _give_ownership(
        &self,
        machine: &mut Machine,
        new_owner: Uint256,
        force_owner: bool,
    ) -> Result<(), ethabi::Error> {
        self._set_chain_parameter(machine, "ChainOwner", new_owner, force_owner)
    }

    pub fn _add_to_reserve_funds(
        &self,
        machine: &mut Machine,
        amount: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "addToReserveFunds",
            &[],
            machine,
            amount,
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_fees_enabled(
        &self,
        machine: &mut Machine,
        enabled: bool,
        force_owner: bool, // force the message to come from address zero, which is an owner
    ) -> Result<(), ethabi::Error> {
        self._set_chain_parameter(
            machine,
            "FeesEnabled",
            if enabled {
                Uint256::one()
            } else {
                Uint256::zero()
            },
            force_owner,
        )
    }

    pub fn _get_fee_recipients(
        &self,
        machine: &mut Machine,
    ) -> Result<(Uint256, Uint256), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getFeeRecipient",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if !receipts[0].succeeded() {
            return Err(ethabi::Error::from("reverted"));
        }

        let return_vals = ethabi::decode(
            &[ethabi::ParamType::Address, ethabi::ParamType::Address],
            &receipts[0].get_return_data(),
        )?;

        match (&return_vals[0], &return_vals[1]) {
            (ethabi::Token::Address(addr1), ethabi::Token::Address(addr2)) => Ok((
                Uint256::from_bytes(addr1.as_bytes()),
                Uint256::from_bytes(addr2.as_bytes()),
            )),
            _ => panic!(),
        }
    }

    pub fn _set_fee_recipients(
        &self,
        machine: &mut Machine,
        recipient1: Uint256,
        recipient2: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "setFeeRecipient",
            &[
                ethabi::Token::Address(recipient1.to_h160()),
                ethabi::Token::Address(recipient2.to_h160()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_seconds_per_send(
        &self,
        machine: &mut Machine,
        seconds_per_send: Uint256,
    ) -> Result<(), ethabi::Error> {
        self._set_chain_parameter(machine, "SecondsPerSend", seconds_per_send, true)
    }

    pub fn _change_sequencer(
        &self,
        machine: &mut Machine,
        sequencer_addr: Uint256,
        delay_seconds: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "changeSequencer",
            &[
                ethabi::Token::Address(sequencer_addr.to_h160()),
                ethabi::Token::Uint(delay_seconds.to_u256()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _start_code_upload(&self, machine: &mut Machine) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function_compressed(
            self.my_address.clone(),
            "startCodeUpload",
            &[],
            machine,
            Uint256::zero(),
            self.wallet,
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _continue_code_upload(
        &self,
        machine: &mut Machine,
        marshalled_code: Vec<u8>,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "continueCodeUpload",
            &[ethabi::Token::Bytes(marshalled_code)],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_uploaded_code_hash(&self, machine: &mut Machine) -> Result<Uint256, ethabi::Error> {
        let (receipts, _) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getUploadedCodeHash",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;
        Ok(Uint256::from_bytes(
            &receipts[receipts.len() - 1].get_return_data(),
        ))
    }

    pub fn _finish_code_upload_as_arbos_upgrade(
        &self,
        machine: &mut Machine,
        new_code_hash: Uint256,
        previous_upgrade_code_hash: Uint256,
    ) -> Result<bool, ethabi::Error> {
        let (receipts, _) = self.contract_abi.call_function(
            self.my_address.clone(),
            "finishCodeUploadAsArbosUpgrade",
            &[
                ethabi::Token::FixedBytes(new_code_hash.to_bytes_be()),
                ethabi::Token::FixedBytes(previous_upgrade_code_hash.to_bytes_be()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        assert_eq!(receipts.len(), 1);

        Ok(receipts[0].succeeded())
    }

    pub fn _finish_code_upload_as_pluggable(
        &self,
        machine: &mut Machine,
        id: Uint256,
        keep_state: bool,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "finishCodeUploadAsPluggable",
            &[
                ethabi::Token::Uint(id.to_u256()),
                ethabi::Token::Bool(keep_state),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _deploy_contract(
        &self,
        machine: &mut Machine,
        constructor_data: &[u8],
        deemed_address: Uint256,
        deemed_nonce: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "deployContract",
            &[
                ethabi::Token::Bytes(constructor_data.to_vec()),
                ethabi::Token::Address(deemed_address.to_h160()),
                ethabi::Token::Uint(deemed_nonce.to_u256()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_fair_gas_price_sender(
        &self,
        machine: &mut Machine,
        addr: Uint256,
        set_to: bool,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "setFairGasPriceSender",
            &[
                ethabi::Token::Address(addr.to_h160()),
                ethabi::Token::Bool(set_to),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _is_fair_gas_price_sender(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<bool, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "isFairGasPriceSender",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(!Uint256::from_bytes(&receipts[0].get_return_data()).is_zero())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_all_fair_gas_price_senders(
        &self,
        machine: &mut Machine,
    ) -> Result<Vec<Uint256>, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "getAllFairGasPriceSenders",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let ret_data = receipts[0].get_return_data();
            let mut ret = vec![];
            let mut offset = 64;
            while (offset < ret_data.len()) {
                ret.push(Uint256::from_bytes(&ret_data[offset..offset + 32]));
                offset += 32;
            }
            Ok(ret)
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _allow_all_senders(&self, machine: &mut Machine) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "allowAllSenders",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _allow_only_owner_to_send(&self, machine: &mut Machine) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "allowOnlyOwnerToSend",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _is_allowed_sender(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<bool, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "isAllowedSender",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(!Uint256::from_bytes(&receipts[0].get_return_data()).is_zero())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _add_allowed_sender(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<bool, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "addAllowedSender",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()) != Uint256::zero())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _remove_allowed_sender(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<bool, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "removeAllowedSender",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()) != Uint256::zero())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_all_allowed_senders(
        &self,
        machine: &mut Machine,
    ) -> Result<Vec<Uint256>, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "getAllAllowedSenders",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let decoded =
                ethabi::decode(&[ethabi::ParamType::Bytes], &*receipts[0].get_return_data())?;

            match &decoded[0] {
                ethabi::Token::Bytes(ret_data) => {
                    let mut ret = vec![];
                    let mut offset = 0;
                    while (offset < ret_data.len()) {
                        ret.push(Uint256::from_bytes(&ret_data[offset..offset + 32]));
                        offset += 32;
                    }
                    Ok(ret)
                }
                _ => Err(ethabi::Error::from("invalid returndata encoding")),
            }
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_l1_gas_price_estimate(
        &self,
        machine: &mut Machine,
        price_in_gwei: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "setL1GasPriceEstimate",
            &[ethabi::Token::Uint(price_in_gwei.to_u256())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }
}

fn _param_id_from_name(name: &str) -> Uint256 {
    Uint256::from_bytes(&keccak256(name.as_bytes()))
}

pub struct _ArbGasInfo<'a> {
    pub contract_abi: AbiForContract,
    _wallet: &'a Wallet,
    my_address: Uint256,
    debug: bool,
}

impl<'a> _ArbGasInfo<'a> {
    pub fn _new(wallet: &'a Wallet, debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbGasInfo")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(108));
        _ArbGasInfo {
            contract_abi,
            _wallet: wallet,
            my_address: Uint256::from_bytes(wallet.address().as_bytes()),
            debug,
        }
    }

    pub fn _get_prices_in_wei_with_aggregator(
        &self,
        machine: &mut Machine,
        aggregator: Uint256,
    ) -> Result<(Uint256, Uint256, Uint256, Uint256, Uint256, Uint256), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getPricesInWeiWithAggregator",
            &[ethabi::Token::Address(aggregator.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                ],
                &receipts[0].get_return_data(),
            )?;

            match (
                &return_vals[0],
                &return_vals[1],
                &return_vals[2],
                &return_vals[3],
                &return_vals[4],
                &return_vals[5],
            ) {
                (
                    ethabi::Token::Uint(ui0),
                    ethabi::Token::Uint(ui1),
                    ethabi::Token::Uint(ui2),
                    ethabi::Token::Uint(ui3),
                    ethabi::Token::Uint(ui4),
                    ethabi::Token::Uint(ui5),
                ) => Ok((
                    Uint256::from_u256(&ui0),
                    Uint256::from_u256(&ui1),
                    Uint256::from_u256(&ui2),
                    Uint256::from_u256(&ui3),
                    Uint256::from_u256(&ui4),
                    Uint256::from_u256(&ui5),
                )),
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _get_prices_in_arbgas_with_aggregator(
        &self,
        machine: &mut Machine,
        aggregator: Uint256,
    ) -> Result<(Uint256, Uint256, Uint256), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getPricesInArbGasWithAggregator",
            &[ethabi::Token::Address(aggregator.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                ],
                &receipts[0].get_return_data(),
            )?;

            match (&return_vals[0], &return_vals[1], &return_vals[2]) {
                (ethabi::Token::Uint(ui0), ethabi::Token::Uint(ui1), ethabi::Token::Uint(ui2)) => {
                    Ok((
                        Uint256::from_u256(&ui0),
                        Uint256::from_u256(&ui1),
                        Uint256::from_u256(&ui2),
                    ))
                }
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _get_gas_accounting_params(
        &self,
        machine: &mut Machine,
    ) -> Result<(Uint256, Uint256, Uint256), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getGasAccountingParams",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                ],
                &receipts[0].get_return_data(),
            )?;

            match (&return_vals[0], &return_vals[1], &return_vals[2]) {
                (ethabi::Token::Uint(ui0), ethabi::Token::Uint(ui1), ethabi::Token::Uint(ui2)) => {
                    Ok((
                        Uint256::from_u256(&ui0),
                        Uint256::from_u256(&ui1),
                        Uint256::from_u256(&ui2),
                    ))
                }
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _get_l1_gas_price_estimate(
        &self,
        machine: &mut Machine,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            self.my_address.clone(),
            "getL1GasPriceEstimate",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }

    pub fn _set_l1_gas_price_estimate(
        &self,
        machine: &mut Machine,
        price_wei: Uint256,
        caller: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, _sends) = self.contract_abi.call_function(
            caller,
            "setL1GasPriceEstimate",
            &[ethabi::Token::Uint(price_wei.to_u256())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if receipts.len() != 1 {
            return Err(ethabi::Error::from("wrong number of receipts"));
        }

        if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from(format!(
                "tx failed: {}",
                receipts[0]._get_return_code_text()
            )))
        }
    }
}

pub struct _ArbAggregator {
    pub contract_abi: AbiForContract,
    debug: bool,
}

impl _ArbAggregator {
    pub fn _new(debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbAggregator")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(109));
        _ArbAggregator {
            contract_abi,
            debug,
        }
    }

    pub fn _get_preferred_aggregator(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<(Uint256, bool), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getPreferredAggregator",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[ethabi::ParamType::Address, ethabi::ParamType::Bool],
                &receipts[0].get_return_data(),
            )?;

            match (&return_vals[0], &return_vals[1]) {
                (ethabi::Token::Address(r1), ethabi::Token::Bool(r2)) => {
                    Ok((Uint256::from_bytes(r1.as_bytes()), *r2))
                }
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_preferred_aggregator(
        &self,
        machine: &mut Machine,
        addr: Uint256,
        sender: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            sender,
            "setPreferredAggregator",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_default_aggregator(&self, machine: &mut Machine) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getDefaultAggregator",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            let return_vals = ethabi::decode(
                &[ethabi::ParamType::Address],
                &receipts[0].get_return_data(),
            )?;

            match &return_vals[0] {
                ethabi::Token::Address(r1) => Ok(Uint256::from_bytes(r1.as_bytes())),
                _ => panic!(),
            }
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_default_aggregator(
        &self,
        machine: &mut Machine,
        addr: Uint256,
        sender: Option<Uint256>, // default sender is address zero
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            if let Some(s) = sender {
                s
            } else {
                Uint256::zero()
            },
            "setDefaultAggregator",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_fee_collector(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::from_u64(10892),
            "getFeeCollector",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_fee_collector(
        &self,
        machine: &mut Machine,
        agg_addr: Uint256,
        new_collector: Uint256,
        sender: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            sender,
            "setFeeCollector",
            &[
                ethabi::Token::Address(agg_addr.to_h160()),
                ethabi::Token::Address(new_collector.to_h160()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_tx_base_fee(
        &self,
        machine: &mut Machine,
        agg_addr: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "getTxBaseFee",
            &[ethabi::Token::Address(agg_addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _set_tx_base_fee(
        &self,
        machine: &mut Machine,
        agg_addr: Uint256,
        fee: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(),
            "setTxBaseFee",
            &[
                ethabi::Token::Address(agg_addr.to_h160()),
                ethabi::Token::Uint(fee.to_u256()),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }
}

pub struct _ArbReplayableTx {
    pub contract_abi: AbiForContract,
    debug: bool,
}

impl _ArbReplayableTx {
    pub fn _new(debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbRetryableTx")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(110));
        _ArbReplayableTx {
            contract_abi,
            debug,
        }
    }

    pub fn _redeem(&self, machine: &mut Machine, txid: Uint256) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "redeem",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() < 1) || (receipts.len() > 2) || (sends.len() != 0) {
            println!("{} receipts, {} sends", receipts.len(), sends.len());
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[receipts.len() - 1].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_timeout(
        &self,
        machine: &mut Machine,
        txid: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getTimeout",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_lifetime(&self, machine: &mut Machine) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getLifetime",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_keepalive_price(
        &self,
        machine: &mut Machine,
        txid: Uint256,
    ) -> Result<(Uint256, Uint256), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getKeepalivePrice",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            let return_data = receipts[0].get_return_data();
            Ok((
                Uint256::from_bytes(&return_data[0..32]),
                Uint256::from_bytes(&return_data[32..64]),
            ))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _keepalive(
        &self,
        machine: &mut Machine,
        txid: Uint256,
        payment: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "keepalive",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            payment,
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_beneficiary(
        &self,
        machine: &mut Machine,
        txid: Uint256,
    ) -> Result<Uint256, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getBeneficiary",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(Uint256::from_bytes(&receipts[0].get_return_data()))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _cancel(
        &self,
        machine: &mut Machine,
        txid: Uint256,
        sender: Uint256,
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            sender,
            "cancel",
            &[ethabi::Token::FixedBytes(txid.to_bytes_be())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }
}

pub struct _ArbStatistics {
    pub contract_abi: AbiForContract,
    debug: bool,
}

impl _ArbStatistics {
    pub fn _new(debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbStatistics")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(111));
        _ArbStatistics {
            contract_abi,
            debug,
        }
    }

    pub fn _get_stats(
        &self,
        machine: &mut Machine,
    ) -> Result<(Uint256, Uint256, Uint256, Uint256, Uint256, Uint256), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getStats",
            &[],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            println!("{} receipts, {} sends", receipts.len(), sends.len());
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            let retdata = receipts[0].get_return_data();
            Ok((
                Uint256::from_bytes(&retdata[0..32]),
                Uint256::from_bytes(&retdata[32..64]),
                Uint256::from_bytes(&retdata[64..96]),
                Uint256::from_bytes(&retdata[96..128]),
                Uint256::from_bytes(&retdata[128..160]),
                Uint256::from_bytes(&retdata[160..192]),
            ))
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }
}

#[test]
fn test_payment_to_self() {
    let _ = _evm_payment_to_self(None, false).unwrap();
}

pub fn _evm_payment_to_self(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::from_u64(20000),
    );
    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let arbinfo = _ArbInfo::_new(false);
    let balance = arbinfo._get_balance(&mut machine, &my_addr)?;
    assert_eq!(balance, Uint256::from_u64(20000));

    let tx_id = machine.runtime_env.insert_tx_message(
        my_addr.clone(),
        Uint256::from_u64(1000000000),
        None,
        my_addr.clone(),
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
    let last_rcpt = receipts.len() - 1;
    assert_eq!(receipts[last_rcpt].get_request_id(), tx_id);
    assert!(receipts[last_rcpt].succeeded());

    let new_balance = arbinfo._get_balance(&mut machine, &my_addr)?;
    assert_eq!(new_balance, Uint256::from_u64(20000));

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
fn _test_upgrade_arbos_to_different_version() {
    _test_upgrade_arbos_over_itself_impl().unwrap();
}

fn _test_upgrade_arbos_over_itself_impl() -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos_before.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    machine.runtime_env.insert_eth_deposit_message(
        Uint256::zero(),
        Uint256::zero(),
        Uint256::_from_eth(100000),
    );
    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::_from_eth(100000),
    );
    let deployer_addr = Uint256::from_u64(1025);
    machine.runtime_env.insert_eth_deposit_message(
        deployer_addr.clone(),
        deployer_addr.clone(),
        Uint256::_from_eth(100000),
    );
    let _ = machine.run(None);

    let mut add_contract = AbiForContract::new_from_file(&test_contract_path("Add"))?;
    if add_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, false)
        .is_err()
    {
        panic!("failed to deploy Add contract");
    }

    let arbowner = _ArbOwner::_new(&wallet, false);

    let arbsys_orig_binding = ArbSys::new(&wallet, false);
    assert_eq!(
        arbsys_orig_binding._arbos_version(&mut machine)?,
        Uint256::from_u64(35),
    );

    arbowner._give_ownership(&mut machine, my_addr, true)?;

    let mexe_path = Path::new("arb_os/arbos-upgrade.mexe");
    let _previous_upgrade_hash = _try_upgrade(&arbowner, &mut machine, &mexe_path, None)?.unwrap();

    let wallet2 = machine.runtime_env.new_wallet();
    let arbsys = ArbSys::new(&wallet2, false);
    let arbos_version = arbsys._arbos_version(&mut machine)?;
    assert_eq!(
        arbos_version,
        *init_constant_table(Some(Path::new("arb_os/constants.json")))
            .unwrap()
            .get("ArbosVersionNumber")
            .unwrap()
    );
    let arbos_version_orig = arbsys_orig_binding._arbos_version(&mut machine)?;
    assert_eq!(arbos_version, arbos_version_orig);

    Ok(())
}

fn _try_upgrade(
    arbowner: &_ArbOwner,
    machine: &mut Machine,
    mexe_path: &Path,
    previous_upgrade_hash: Option<Uint256>,
) -> Result<Option<Uint256>, ethabi::Error> {
    let uploader = CodeUploader::_new_from_file(mexe_path);
    arbowner._start_code_upload(machine)?;

    let mut accum = vec![];
    for buf in uploader.instructions {
        accum.extend(buf);
        if (accum.len() > 3000) {
            arbowner._continue_code_upload(machine, accum)?;
            accum = vec![];
        }
    }
    if (accum.len() > 0) {
        arbowner._continue_code_upload(machine, accum)?;
    }

    let expected_code_hash = arbowner._get_uploaded_code_hash(machine)?;
    Ok(
        if arbowner._finish_code_upload_as_arbos_upgrade(
            machine,
            expected_code_hash.clone(),
            previous_upgrade_hash.unwrap_or(Uint256::zero()),
        )? {
            Some(expected_code_hash)
        } else {
            None
        },
    )
}

#[test]
pub fn test_gas_charging_underfunded() {
    match _evm_run_with_gas_charging(None, Uint256::_from_gwei(20), false, false) {
        Ok(result) => assert_eq!(result, false),
        Err(e) => panic!("error {}", e),
    }
}

#[test]
pub fn test_gas_charging_fully_funded() {
    match _evm_run_with_gas_charging(None, Uint256::_from_eth(1000), false, false) {
        Ok(result) => assert_eq!(result, true),
        Err(e) => panic!("error {}", e),
    }
}

pub fn _evm_run_with_gas_charging(
    log_to: Option<&Path>,
    funding: Uint256,
    debug: bool,
    _profile: bool,
) -> Result<bool, ethabi::Error> {
    // returns Ok(true) if success, Ok(false) if insufficient gas money, Err otherwise
    use std::convert::TryFrom;
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    machine
        .runtime_env
        .insert_eth_deposit_message(my_addr.clone(), my_addr.clone(), funding);
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    }; // handle these ETH deposit messages

    println!("First deploy ...");
    let mut fib_contract = AbiForContract::new_from_file(&test_contract_path("Fibonacci"))?;
    if let Err(receipt) = fib_contract.deploy(&[], &mut machine, Uint256::zero(), None, debug) {
        if receipt.unwrap().get_return_code() == Uint256::from_u64(3) {
            return Ok(false);
        } else {
            panic!("unexpected failure deploying Fibonacci contract");
        }
    }

    println!("Second deploy ...");
    let mut pc_contract = AbiForContract::new_from_file(&test_contract_path("PaymentChannel"))?;
    if let Err(receipt) = pc_contract.deploy(
        &[ethabi::Token::Address(ethereum_types::H160::from_slice(
            &fib_contract.address.to_bytes_be()[12..],
        ))],
        &mut machine,
        Uint256::zero(),
        None,
        debug,
    ) {
        if receipt.unwrap().get_return_code() == Uint256::from_u64(3) {
            return Ok(false);
        } else {
            panic!("unexpected failure deploying PaymentChannel contract");
        }
    }

    // turn on gas charging
    let arbowner = _ArbOwner::_new(&wallet, false);
    arbowner._set_fees_enabled(&mut machine, true, true)?;
    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, false);

    println!("Function call ...");
    let (logs, sends) = pc_contract.call_function(
        my_addr.clone(),
        "deposit",
        &[],
        &mut machine,
        Uint256::_from_eth(1),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);

    if !logs[0].succeeded() {
        if logs[0].get_return_code() == Uint256::from_u64(3) {
            return Ok(false);
        } else {
            panic!();
        }
    }

    let (logs, sends) = pc_contract.call_function(
        my_addr,
        "transferFib",
        &[
            ethabi::Token::Address(ethabi::Address::from_low_u64_be(1025)),
            ethabi::Token::Uint(ethabi::Uint::try_from(1).unwrap()),
        ],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(logs.len(), 1);
    assert_eq!(sends.len(), 0);

    if !logs[0].succeeded() {
        if logs[0].get_return_code() == Uint256::from_u64(3) {
            return Ok(false);
        } else {
            panic!();
        }
    }

    if let Some(path) = log_to {
        machine
            .runtime_env
            .recorder
            .to_file(path, machine.get_total_gas_usage().to_u64().unwrap())
            .unwrap();
    }

    Ok(true)
}

#[test]
fn test_arbowner() {
    match _evm_test_arbowner(None, false) {
        Ok(()) => {}
        Err(e) => panic!("{:?}", e),
    }
}

pub fn _evm_test_arbowner(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, debug);

    arbowner._give_ownership(&mut machine, my_addr, true)?;

    arbowner._start_code_upload(&mut machine)?;

    let mcode = vec![0x90u8, 1u8, 0u8, 42u8]; // debugprint(42)
    arbowner._continue_code_upload(&mut machine, mcode)?;

    let expected_code_hash = arbowner._get_uploaded_code_hash(&mut machine)?;
    assert!(arbowner._finish_code_upload_as_arbos_upgrade(
        &mut machine,
        expected_code_hash,
        Uint256::zero(),
    )?);

    arbowner._set_seconds_per_send(&mut machine, Uint256::from_u64(10))?;

    arbowner._set_gas_accounting_params(
        &mut machine,
        Uint256::from_u64(100_000_000),
        Uint256::from_u64(6_000_000_000),
        Uint256::from_u64(1_000_000_000),
    )?;

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
fn test_arb_fair_gas_price_list() {
    _evm_test_arb_fair_gas_price_list();
}

fn _evm_test_arb_fair_gas_price_list() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let _my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, false);

    let addr1 = Uint256::from_u64(13853);
    let addr2 = Uint256::from_u64(813915);
    let addr3 = Uint256::from_u64(88);

    assert_eq!(
        arbowner
            ._get_all_fair_gas_price_senders(&mut machine)
            .unwrap()
            .len(),
        0
    );
    assert!(!arbowner
        ._is_fair_gas_price_sender(&mut machine, addr2.clone())
        .unwrap());
    arbowner
        ._set_fair_gas_price_sender(&mut machine, addr1.clone(), true)
        .unwrap();
    assert!(arbowner
        ._is_fair_gas_price_sender(&mut machine, addr1.clone())
        .unwrap());
    assert!(!arbowner
        ._is_fair_gas_price_sender(&mut machine, addr2.clone())
        .unwrap());
    arbowner
        ._set_fair_gas_price_sender(&mut machine, addr3.clone(), true)
        .unwrap();
    assert!(arbowner
        ._is_fair_gas_price_sender(&mut machine, addr3.clone())
        .unwrap());

    let lis = arbowner
        ._get_all_fair_gas_price_senders(&mut machine)
        .unwrap();
    assert_eq!(lis.len(), 2);
    assert!(
        ((lis[0] == addr1.clone()) && (lis[1] == addr3.clone()))
            || ((lis[0] == addr3.clone()) && (lis[1] == addr1.clone()))
    );
}

#[test]
fn test_run_tx_with_extra_gas() {
    match _evm_test_tx_with_extra_gas(None, false) {
        Ok(()) => {}
        Err(e) => panic!("{:?}", e),
    }
}

pub fn _evm_test_tx_with_extra_gas(
    log_to: Option<&Path>,
    debug: bool,
) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, debug);

    let mut add_contract = AbiForContract::new_from_file(&test_contract_path("Add"))?;
    let constructor_data = if let Some(constructor) = add_contract.contract.constructor() {
        match constructor.encode_input(add_contract.code_bytes.clone(), &[]) {
            Ok(aug_code) => aug_code,
            Err(e) => {
                panic!("couldn't encode data for constructor: {:?}", e);
            }
        }
    } else {
        add_contract.code_bytes.clone()
    };

    let deploy_addr = arbowner._deploy_contract(
        &mut machine,
        &constructor_data,
        Uint256::from_u64(41389147891),
        Uint256::from_u64(417813478913111),
    )?;

    add_contract.bind_interface_to_address(deploy_addr.clone());

    let (receipts, _) = add_contract.call_function(
        my_addr.clone(),
        "add",
        &[
            ethabi::Token::Uint(Uint256::one().to_u256()),
            ethabi::Token::Uint(Uint256::one().to_u256()),
        ],
        &mut machine,
        Uint256::zero(),
        debug,
    )?;
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].succeeded());
    assert_eq!(
        receipts[0].get_return_data(),
        Uint256::from_u64(2).to_bytes_be()
    );

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
fn test_arbgasinfo() {
    match _evm_test_arbgasinfo(None, false) {
        Ok(()) => {}
        Err(e) => panic!("{:?}", e),
    }
}

pub fn _evm_test_arbgasinfo(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, debug);
    let arbgasinfo = _ArbGasInfo::_new(&wallet, debug);
    let arbaggregator = _ArbAggregator::_new(debug);

    machine.runtime_env.insert_eth_deposit_message(
        my_addr.clone(),
        my_addr.clone(),
        Uint256::_from_eth(100),
    );
    let _ = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let (l2tx, l1calldata, storage, basegas, conggas, totalgas) =
        arbgasinfo._get_prices_in_wei_with_aggregator(&mut machine, Uint256::zero())?;
    assert!(l2tx.is_zero());
    assert!(l1calldata.is_zero());
    assert!(storage.is_zero());
    assert!(basegas.is_zero());
    assert!(conggas.is_zero());
    assert_eq!(basegas.add(&conggas), totalgas);

    arbowner._set_fees_enabled(&mut machine, true, true)?;
    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);

    let (l2tx, l1calldata, storage, basegas, conggas, totalgas) =
        arbgasinfo._get_prices_in_wei_with_aggregator(&mut machine, Uint256::zero())?;
    println!(
        "L2 tx {}, L1 calldata {}, L2 storage {}, base gas {}, congestion gas {}, total gas {}",
        l2tx, l1calldata, storage, basegas, conggas, totalgas
    );
    assert_eq!(l2tx, Uint256::from_u64(690000000000000));
    assert_eq!(l1calldata, Uint256::from_u64(172500000000));
    assert_eq!(storage, Uint256::from_u64(300000000000000));
    assert_eq!(basegas, Uint256::from_u64(1500000000));
    assert!(conggas.is_zero());
    assert_eq!(basegas.add(&conggas), totalgas);

    let (l2tx, l1calldata, storage) =
        arbgasinfo._get_prices_in_arbgas_with_aggregator(&mut machine, Uint256::zero())?;
    println!(
        "L2 tx / ag {}, L1 calldata / ag {}, L2 storage / ag {}",
        l2tx, l1calldata, storage
    );
    assert_eq!(l2tx, Uint256::from_u64(460000));
    assert_eq!(l1calldata, Uint256::from_u64(115));
    assert_eq!(storage, Uint256::from_u64(200000));

    let (speed_limit, gas_pool_max, tx_gas_limit) =
        arbgasinfo._get_gas_accounting_params(&mut machine)?;
    println!(
        "speed limit {}, pool max {}, tx gas limit {}",
        speed_limit, gas_pool_max, tx_gas_limit
    );
    assert_eq!(speed_limit, Uint256::from_u64(400_000));
    assert_eq!(gas_pool_max, Uint256::from_u64(288_000_000));
    assert_eq!(tx_gas_limit, Uint256::from_u64(8_000_000));

    let agg_addr = Uint256::from_u64(777);
    let fee = arbaggregator
        ._get_tx_base_fee(&mut machine, agg_addr.clone())
        .unwrap();
    assert_eq!(fee, Uint256::from_u64(4000));

    let new_fee = Uint256::from_u64(8913);
    arbaggregator
        ._set_tx_base_fee(&mut machine, agg_addr.clone(), new_fee.clone())
        .unwrap();

    let fee = arbaggregator
        ._get_tx_base_fee(&mut machine, agg_addr.clone())
        .unwrap();
    assert_eq!(fee, new_fee);

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
fn test_arbgas_oracle() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, false);
    let arbgasinfo = _ArbGasInfo::_new(&wallet, false);

    arbowner
        ._set_chain_parameter(&mut machine, "GasPriceOracle", my_addr.clone(), true)
        .unwrap();

    let gpest = Uint256::_from_gwei(73);
    arbgasinfo
        ._set_l1_gas_price_estimate(&mut machine, gpest.clone(), my_addr.clone())
        .unwrap();

    let gasprice = arbgasinfo._get_l1_gas_price_estimate(&mut machine).unwrap();
    assert_eq!(gpest, gasprice);
}

pub fn _evm_test_rate_control(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());
    let arbowner = _ArbOwner::_new(&wallet, debug);

    arbowner._give_ownership(&mut machine, my_addr, true)?;

    let const_table = init_constant_table(Some(Path::new("arb_os/constants.json"))).unwrap();

    let (r1, r2) = arbowner._get_fee_recipients(&mut machine)?;
    assert_eq!(&r1, const_table.get("NetFee_defaultRecipient").unwrap());
    assert_eq!(
        &r2,
        const_table.get("CongestionFee_defaultRecipient").unwrap()
    );

    let new_r1 = r1.add(&Uint256::one());
    let new_r2 = r2.add(&Uint256::one());
    arbowner._set_fee_recipients(&mut machine, new_r1.clone(), new_r2.clone())?;
    let (updated_r1, updated_r2) = arbowner._get_fee_recipients(&mut machine)?;
    assert_eq!(new_r1, updated_r1);
    assert_eq!(new_r2, updated_r2);

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
fn test_rollup_tracker() {
    _do_rollup_tracker_ops();
}

pub fn _do_rollup_tracker_ops() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let owner = Uint256::from_bytes(wallet.address().as_bytes());
    let arbowner = _ArbOwner::_new(&wallet, false);
    arbowner
        ._give_ownership(&mut machine, owner.clone(), true)
        .unwrap();

    let my_addr = Uint256::from_u64(11025);
    let claimer = Uint256::from_u64(4242);

    machine.runtime_env.insert_eth_deposit_message(
        owner.clone(),
        owner.clone(),
        Uint256::_from_eth(1),
    );
    machine.runtime_env.insert_eth_deposit_message(
        claimer.clone(),
        claimer.clone(),
        Uint256::_from_eth(1),
    );
    let _ = machine.run(None);

    arbowner
        ._add_to_reserve_funds(&mut machine, Uint256::_from_eth(1))
        .unwrap();

    _insert_create_node(
        &mut machine.runtime_env,
        &Uint256::one(),
        &Uint256::zero(),
        None,
        &Uint256::from_u64(10),
        my_addr.clone(),
    );
    _insert_create_node(
        &mut machine.runtime_env,
        &Uint256::from_u64(2),
        &Uint256::one(),
        None,
        &Uint256::from_u64(10),
        my_addr.clone(),
    );

    _insert_claim_node(&mut machine.runtime_env, &Uint256::from_u64(2), &claimer);

    _insert_create_node(
        &mut machine.runtime_env,
        &Uint256::from_u64(3),
        &Uint256::one(),
        None,
        &Uint256::from_u64(10),
        my_addr.clone(),
    );

    _insert_create_node(
        &mut machine.runtime_env,
        &Uint256::from_u64(4),
        &Uint256::from_u64(2),
        None,
        &Uint256::from_u64(10),
        my_addr.clone(),
    );

    _insert_new_stake(
        &mut machine.runtime_env,
        &Uint256::from_u64(4),
        &claimer,
        None,
    );

    _insert_rollup_debug(&mut machine.runtime_env);
    let _ = machine.run(None);

    _insert_confirm_node(&mut machine.runtime_env, &Uint256::zero());
    _insert_confirm_node(&mut machine.runtime_env, &Uint256::one());
    _insert_confirm_node(&mut machine.runtime_env, &Uint256::from_u64(2));
    _insert_reject_node(&mut machine.runtime_env, &Uint256::from_u64(3));
    _insert_confirm_node(&mut machine.runtime_env, &Uint256::from_u64(4));

    machine
        .runtime_env
        ._advance_time(Uint256::from_u64(100), None, true);

    _insert_create_node(
        &mut machine.runtime_env,
        &Uint256::from_u64(5),
        &Uint256::from_u64(4),
        None,
        &Uint256::from_u64(10),
        my_addr.clone(),
    );

    let _ = machine.run(None);

    // There isn't really a result we can check here, so this test just confirms that nothing crashes.
}

pub fn _insert_create_node(
    rt_env: &mut RuntimeEnvironment,
    height_l2: &Uint256,
    prev: &Uint256,
    height_l1: Option<&Uint256>,
    deadline_l1_delta: &Uint256,
    asserter: Uint256,
) {
    let height_l1 = &height_l1.unwrap_or(&rt_env.current_block_num);
    let mut buf = vec![0u8];
    buf.extend(height_l2.to_bytes_be());
    buf.extend(prev.to_bytes_be());
    buf.extend(height_l1.to_bytes_be());
    buf.extend(height_l1.add(deadline_l1_delta).to_bytes_be());
    buf.extend(asserter.to_bytes_be());
    rt_env.insert_l1_message(8u8, Uint256::zero(), &buf, None, None);
}

pub fn _insert_confirm_node(rt_env: &mut RuntimeEnvironment, height_l2: &Uint256) {
    let mut buf = vec![1u8];
    buf.extend(height_l2.to_bytes_be());
    rt_env.insert_l1_message(8u8, Uint256::zero(), &buf, None, None);
}

pub fn _insert_reject_node(rt_env: &mut RuntimeEnvironment, height_l2: &Uint256) {
    let mut buf = vec![2u8];
    buf.extend(height_l2.to_bytes_be());
    rt_env.insert_l1_message(8u8, Uint256::zero(), &buf, None, None);
}

pub fn _insert_new_stake(
    rt_env: &mut RuntimeEnvironment,
    height_l2: &Uint256,
    staker: &Uint256,
    stake_time: Option<Uint256>,
) {
    let mut buf = vec![3u8];
    buf.extend(height_l2.to_bytes_be());
    buf.extend(staker.to_bytes_be());
    buf.extend(
        stake_time
            .unwrap_or(rt_env.current_block_num.clone())
            .to_bytes_be(),
    );
    rt_env.insert_l1_message(8u8, Uint256::zero(), &buf, None, None);
}

pub fn _insert_claim_node(rt_env: &mut RuntimeEnvironment, height_l2: &Uint256, claimer: &Uint256) {
    let mut buf = vec![4u8];
    buf.extend(height_l2.to_bytes_be());
    buf.extend(claimer.to_bytes_be());
    rt_env.insert_l1_message(8u8, Uint256::zero(), &buf, None, None);
}

pub fn _insert_rollup_debug(rt_env: &mut RuntimeEnvironment) {
    rt_env.insert_l1_message(8u8, Uint256::zero(), &[255u8], None, None);
}

#[test]
fn test_arbaggregator() {
    match _evm_test_arbaggregator(None, false) {
        Ok(()) => {}
        Err(e) => panic!("{:?}", e),
    }
}

pub fn _evm_test_arbaggregator(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let wallet = machine.runtime_env.new_wallet();
    let my_addr = Uint256::from_bytes(wallet.address().as_bytes());

    let arbagg = _ArbAggregator::_new(debug);

    assert_eq!(
        arbagg._get_fee_collector(&mut machine, my_addr.clone())?,
        my_addr.clone()
    );

    let pref_agg = arbagg._get_preferred_aggregator(&mut machine, my_addr.clone())?;
    assert_eq!(pref_agg, (Uint256::zero(), true));

    let new_pref_agg = Uint256::from_u64(4242);
    arbagg._set_preferred_aggregator(&mut machine, new_pref_agg.clone(), my_addr.clone())?;
    let pref_agg = arbagg._get_preferred_aggregator(&mut machine, my_addr.clone())?;
    assert_eq!(pref_agg, (new_pref_agg, false));

    let def_agg = arbagg._get_default_aggregator(&mut machine)?;
    assert_eq!(def_agg, Uint256::zero());

    let new_def_agg = Uint256::from_u64(9696);
    arbagg._set_default_aggregator(&mut machine, new_def_agg.clone(), None)?;
    let def_agg = arbagg._get_default_aggregator(&mut machine)?;
    assert_eq!(def_agg, new_def_agg);

    assert!(arbagg
        ._set_default_aggregator(
            &mut machine,
            Uint256::from_u64(12345),
            Some(my_addr.clone())
        )
        .is_err());

    assert_eq!(
        arbagg._get_fee_collector(&mut machine, my_addr.clone())?,
        my_addr.clone()
    );

    let new_collector = Uint256::from_u64(1298031);

    assert!(arbagg
        ._set_fee_collector(
            &mut machine,
            my_addr.clone(),
            new_collector.clone(),
            new_collector.clone()
        )
        .is_err());
    assert_eq!(
        arbagg._get_fee_collector(&mut machine, my_addr.clone())?,
        my_addr.clone()
    );

    assert!(arbagg
        ._set_fee_collector(
            &mut machine,
            my_addr.clone(),
            new_collector.clone(),
            my_addr.clone()
        )
        .is_ok());
    assert_eq!(
        arbagg._get_fee_collector(&mut machine, my_addr.clone())?,
        new_collector.clone()
    );

    let newer_collector = Uint256::from_u64(589713578913);
    assert!(arbagg
        ._set_fee_collector(
            &mut machine,
            my_addr.clone(),
            newer_collector.clone(),
            my_addr.clone()
        )
        .is_err());
    assert!(arbagg
        ._set_fee_collector(
            &mut machine,
            my_addr.clone(),
            newer_collector.clone(),
            new_collector.clone()
        )
        .is_ok());
    assert_eq!(
        arbagg._get_fee_collector(&mut machine, my_addr.clone())?,
        newer_collector.clone()
    );

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
fn test_retryable() {
    match _test_retryable(None, false) {
        Ok(()) => {}
        Err(e) => panic!("{}", e),
    }
}

pub fn _test_retryable(log_to: Option<&Path>, debug: bool) -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1234);

    let mut add_contract = AbiForContract::new_from_file(&test_contract_path("Add"))?;
    if add_contract
        .deploy(&[], &mut machine, Uint256::zero(), None, debug)
        .is_err()
    {
        panic!("failed to deploy Add contract");
    }

    let beneficiary = Uint256::from_u64(9185);

    let (_, txid, _) = add_contract._send_retryable_tx(
        my_addr.clone(),
        "add",
        &[
            ethabi::Token::Uint(Uint256::one().to_u256()),
            ethabi::Token::Uint(Uint256::one().to_u256()),
        ],
        &mut machine,
        Uint256::zero(),
        Uint256::zero(),
        Uint256::zero(),
        None,
        Some(beneficiary.clone()),
        None,
        None,
    )?;
    assert!(txid != Uint256::zero());
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let arb_replayable = _ArbReplayableTx::_new(debug);
    let timeout = arb_replayable._get_timeout(&mut machine, txid.clone())?;
    assert!(timeout > machine.runtime_env.current_timestamp);

    let (keepalive_price, reprice_time) =
        arb_replayable._get_keepalive_price(&mut machine, txid.clone())?;
    assert_eq!(keepalive_price, Uint256::zero());
    assert!(reprice_time > machine.runtime_env.current_timestamp);

    let keepalive_ret = arb_replayable._keepalive(&mut machine, txid.clone(), keepalive_price)?;

    let new_timeout = arb_replayable._get_timeout(&mut machine, txid.clone())?;
    assert_eq!(keepalive_ret, new_timeout);
    assert!(new_timeout > timeout);

    arb_replayable._redeem(&mut machine, txid.clone())?;

    let new_timeout = arb_replayable._get_timeout(&mut machine, txid.clone())?;
    assert_eq!(new_timeout, Uint256::zero()); // verify that txid has been removed

    // make another one, and have the beneficiary cancel it
    let (_, txid, _) = add_contract._send_retryable_tx(
        my_addr.clone(),
        "add",
        &[
            ethabi::Token::Uint(Uint256::one().to_u256()),
            ethabi::Token::Uint(Uint256::one().to_u256()),
        ],
        &mut machine,
        Uint256::zero(),
        Uint256::zero(),
        Uint256::zero(),
        None,
        Some(beneficiary.clone()),
        None,
        None,
    )?;
    assert!(txid != Uint256::zero());
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let out_beneficiary = arb_replayable._get_beneficiary(&mut machine, txid.clone())?;
    assert_eq!(out_beneficiary, beneficiary);

    arb_replayable._cancel(&mut machine, txid.clone(), beneficiary.clone())?;

    assert_eq!(
        arb_replayable._get_timeout(&mut machine, txid)?,
        Uint256::zero()
    ); // verify txid no longer exists

    let amount_to_pay = Uint256::from_u64(1_000_000);
    let _txid = machine.runtime_env._insert_retryable_tx_message(
        my_addr.clone(),
        Uint256::from_u64(7890245789245), // random non-contract address
        amount_to_pay.clone(),
        amount_to_pay.clone(),
        Uint256::zero(),
        my_addr.clone(),
        my_addr.clone(),
        Uint256::zero(),
        Uint256::zero(),
        &[],
    );
    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };
    let all_logs = machine.runtime_env.get_all_receipt_logs();
    let last_log = &all_logs[all_logs.len() - 1];
    assert!(last_log.succeeded());

    let (_submitid, txid, maybe_redeemid) = add_contract._send_retryable_tx(
        my_addr.clone(),
        "add",
        &[
            ethabi::Token::Uint(Uint256::one().to_u256()),
            ethabi::Token::Uint(Uint256::one().to_u256()),
        ],
        &mut machine,
        Uint256::zero(),
        Uint256::_from_eth(100),
        Uint256::zero(),
        None,
        Some(beneficiary.clone()),
        Some(Uint256::from_u64(1_000_000)),
        Some(Uint256::_from_gwei(5)),
    )?;
    assert!(txid != Uint256::zero());
    assert!(maybe_redeemid.is_some());
    let redeemid = maybe_redeemid.unwrap();

    let _gas_used = if debug {
        machine.debug(None)
    } else {
        machine.run(None)
    };

    let receipts = machine.runtime_env.get_all_receipt_logs();
    let last_receipt = receipts[receipts.len() - 1].clone();
    assert_eq!(last_receipt.get_return_code(), Uint256::zero());
    assert_eq!(last_receipt.get_request_id(), redeemid);

    let second_to_last = receipts[receipts.len() - 2].clone();
    assert!(second_to_last.succeeded());
    assert_eq!(second_to_last.get_request_id(), txid);

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
fn test_arb_statistics() {
    assert!(_test_arb_stats().is_ok());
}

fn _test_arb_stats() -> Result<(), ethabi::Error> {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let arbstats = _ArbStatistics::_new(false);

    let (arb_blocknum, num_accounts, storage, _arbgas, txs, contracts) =
        arbstats._get_stats(&mut machine)?;

    assert_eq!(arb_blocknum, Uint256::from_u64(1));
    assert_eq!(num_accounts, Uint256::from_u64(22));
    assert_eq!(storage, Uint256::from_u64(0));
    // assert_eq!(_arbgas, Uint256::from_u64(1_490_972));  // disable this because it will vary over versions
    assert_eq!(txs, Uint256::from_u64(0));
    assert_eq!(contracts, Uint256::from_u64(19));
    Ok(())
}

#[test]
fn test_allowed_senders() {
    _evm_test_allowed_senders();
}

fn _evm_test_allowed_senders() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let wallet = machine.runtime_env.new_wallet();

    let arbowner = _ArbOwner::_new(&wallet, false);

    let addr1 = Uint256::from_u64(123489121);
    let addr2 = Uint256::from_u64(1348098777777);

    assert!(arbowner
        ._is_allowed_sender(&mut machine, addr1.clone())
        .unwrap());

    assert!(arbowner._allow_only_owner_to_send(&mut machine).is_ok());
    assert!(!arbowner
        ._is_allowed_sender(&mut machine, addr1.clone())
        .unwrap());
    assert!(arbowner
        ._is_allowed_sender(&mut machine, Uint256::zero())
        .unwrap());

    assert!(arbowner._allow_all_senders(&mut machine).is_ok());
    assert!(arbowner
        ._is_allowed_sender(&mut machine, addr1.clone())
        .unwrap());

    assert!(arbowner._allow_only_owner_to_send(&mut machine).is_ok());
    assert!(arbowner
        ._add_allowed_sender(&mut machine, addr1.clone())
        .is_ok());
    assert!(arbowner
        ._is_allowed_sender(&mut machine, addr1.clone())
        .unwrap());
    assert!(!arbowner
        ._is_allowed_sender(&mut machine, addr2.clone())
        .unwrap());

    assert!(arbowner
        ._add_allowed_sender(&mut machine, addr2.clone())
        .is_ok());
    let serialized = arbowner._get_all_allowed_senders(&mut machine).unwrap();
    assert_eq!(serialized.len(), 2);
    assert!(
        ((serialized[0] == addr1.clone()) && (serialized[1] == addr2.clone()))
            || ((serialized[0] == addr2.clone()) && (serialized[1] == addr1.clone()))
    );

    assert!(arbowner
        ._remove_allowed_sender(&mut machine, addr1.clone())
        .is_ok());
    assert!(!arbowner
        ._is_allowed_sender(&mut machine, addr1.clone())
        .unwrap());
    assert!(arbowner
        ._is_allowed_sender(&mut machine, addr2.clone())
        .unwrap());
}

pub struct ArbosTest {
    pub contract_abi: AbiForContract,
    debug: bool,
}

impl ArbosTest {
    pub fn new(debug: bool) -> Self {
        let mut contract_abi =
            AbiForContract::new_from_file(&builtin_contract_path("ArbosTest")).unwrap();
        contract_abi.bind_interface_to_address(Uint256::from_u64(105));
        ArbosTest {
            contract_abi,
            debug,
        }
    }

    pub fn _install_account_and_call(
        &self,
        machine: &mut Machine,
        addr: Uint256,
        balance: Uint256,
        nonce: Uint256,
        code: Vec<u8>,
        storage: Vec<u8>,
        calldata: Vec<u8>,
    ) -> Result<Vec<u8>, ethabi::Error> {
        self.install_account(
            machine,
            addr.clone(),
            balance.clone(),
            nonce.clone(),
            Some(code),
            Some(storage),
        )?;
        let _ = machine.runtime_env.get_and_incr_seq_num(&Uint256::zero());
        let _ = machine.runtime_env.get_and_incr_seq_num(&Uint256::zero());
        self.call(machine, Uint256::zero(), addr.clone(), calldata, balance)?;
        self._get_marshalled_storage(machine, addr)
    }

    pub fn install_account(
        &self,
        machine: &mut Machine,
        addr: Uint256,
        balance: Uint256,
        nonce: Uint256,
        code: Option<Vec<u8>>,
        storage: Option<Vec<u8>>,
    ) -> Result<(), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "installAccount",
            &[
                ethabi::Token::Address(addr.to_h160()),
                ethabi::Token::Bool((code == None) && (storage == None)),
                ethabi::Token::Uint(balance.to_u256()),
                ethabi::Token::Uint(nonce.to_u256()),
                ethabi::Token::Bytes(code.unwrap_or(vec![])),
                ethabi::Token::Bytes(storage.unwrap_or(vec![])),
            ],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            Ok(())
        } else {
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn call(
        &self,
        machine: &mut Machine,
        caller_addr: Uint256,
        callee_addr: Uint256,
        calldata: Vec<u8>,
        callvalue: Uint256,
    ) -> Result<Vec<u8>, ethabi::Error> {
        machine.runtime_env.insert_eth_deposit_message(
            Uint256::zero(),
            caller_addr.clone(),
            callvalue.clone(),
        );
        let _tx_id = machine.runtime_env.insert_tx_message(
            caller_addr,
            Uint256::from_usize(1_000_000_000),
            None,
            callee_addr,
            callvalue,
            &calldata,
            false,
        );
        let num_logs_before = machine.runtime_env.get_all_receipt_logs().len();
        let num_sends_before = machine.runtime_env.get_all_sends().len();
        let _arbgas_used = if self.debug {
            machine.debug(None)
        } else {
            machine.run(None)
        };
        let logs = machine.runtime_env.get_all_receipt_logs();
        let sends = machine.runtime_env.get_all_sends();

        if (logs.len() != num_logs_before + 2) || (sends.len() != num_sends_before) {
            return Err(ethabi::Error::from("wrong number of receipts or sends"));
        }
        if !logs[num_logs_before + 1].succeeded() {
            println!(
                "arbosTest.run revert code {}",
                logs[num_logs_before + 1].get_return_code()
            );
            return Err(ethabi::Error::from("reverted"));
        }

        Ok(logs[num_logs_before + 1].get_return_data())
    }

    pub fn get_account_info(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<(Uint256, Uint256, Vec<u8>), ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getAccountInfo",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            let return_data = receipts[0].get_return_data();
            Ok((
                Uint256::from_bytes(&return_data[0..32]),
                Uint256::from_bytes(&return_data[32..64]),
                return_data[64..].to_vec(),
            ))
        } else {
            println!(
                "arbosTest.run revert code {}",
                receipts[0].get_return_code()
            );
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _get_marshalled_storage(
        &self,
        machine: &mut Machine,
        addr: Uint256,
    ) -> Result<Vec<u8>, ethabi::Error> {
        let (receipts, sends) = self.contract_abi.call_function(
            Uint256::zero(), // send from address zero
            "getMarshalledStorage",
            &[ethabi::Token::Address(addr.to_h160())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;

        if (receipts.len() != 1) || (sends.len() != 0) {
            Err(ethabi::Error::from("wrong number of receipts or sends"))
        } else if receipts[0].succeeded() {
            return Ok(receipts[0].get_return_data()[64..].to_vec());
        } else {
            println!(
                "arbosTest.run revert code {}",
                receipts[0].get_return_code()
            );
            Err(ethabi::Error::from("reverted"))
        }
    }

    pub fn _burn_arb_gas(
        &self,
        machine: &mut Machine,
        sender_addr: Uint256,
        amount: Uint256,
    ) -> Result<u64, ethabi::Error> {
        let (receipts, _) = self.contract_abi.call_function(
            sender_addr, // send from address zero
            "burnArbGas",
            &[ethabi::Token::Uint(amount.to_u256())],
            machine,
            Uint256::zero(),
            self.debug,
        )?;
        assert_eq!(receipts.len(), 1);
        Ok(receipts[0].get_return_code().to_u64().unwrap())
    }
}

#[test]
fn test_eventual_congestion_reject() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let wallet = machine.runtime_env.new_wallet();
    let my_address = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, false);
    let _ = arbowner
        ._set_fees_enabled(&mut machine, true, true)
        .unwrap();
    machine.runtime_env.insert_eth_deposit_message(
        my_address.clone(),
        my_address.clone(),
        Uint256::_from_eth(1000),
    );
    let _ = machine.run(None);
    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);

    let arbtest = ArbosTest::new(false);
    for _ in 0..1200 {
        let res_code = match arbtest._burn_arb_gas(
            &mut machine,
            my_address.clone(),
            Uint256::from_u64(2_000_000),
        ) {
            Ok(rc) => rc,
            Err(_) => panic!(),
        };
        if res_code == 2 {
            return;
        } // we hit congestion, as expected
        assert_eq!(res_code, 0); // we should report success, if haven't hit congestion yet
    }
    assert!(false);
}

#[test]
fn test_congestion_price_adjustment() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let wallet = machine.runtime_env.new_wallet();
    let my_address = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, false);
    let arbgasinfo = _ArbGasInfo::_new(&wallet, false);
    let _ = arbowner
        ._set_fees_enabled(&mut machine, true, true)
        .unwrap();
    machine.runtime_env.insert_eth_deposit_message(
        my_address.clone(),
        my_address.clone(),
        Uint256::_from_eth(1000),
    );
    let _ = machine.run(None);
    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, true);

    let _randomish_address = Uint256::from_u64(1583913081);
    assert_eq!(
        arbgasinfo
            //._get_prices_in_wei(&mut machine, randomish_address.clone())  preserve this for later integration
            ._get_prices_in_wei_with_aggregator(&mut machine, my_address.clone())
            .unwrap()
            .4,
        Uint256::zero()
    );

    let arbtest = ArbosTest::new(false);
    let mut seen_any_congestion = false;
    for _ in 0..1200 {
        let res_code = match arbtest._burn_arb_gas(
            &mut machine,
            my_address.clone(),
            Uint256::from_u64(2_000_000),
        ) {
            Ok(rc) => rc,
            Err(_) => panic!(),
        };
        assert!((!seen_any_congestion && (res_code == 0)) || (res_code == 2)); // success or congestion
        if (res_code == 2) {
            seen_any_congestion = true;
        }
    }
    assert!(seen_any_congestion);

    // the chain should be congested now
    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, false);
    let _ = machine.run(None);

    let prices = arbgasinfo
        //._get_prices_in_wei(&mut machine, randomish_address.clone())  preserve this for later integration
        ._get_prices_in_wei_with_aggregator(&mut machine, my_address.clone())
        .unwrap();
    assert!(prices.4 > Uint256::zero());

    machine
        .runtime_env
        ._advance_time(Uint256::from_u64(48), Some(Uint256::from_u64(720)), false);
    let prices2 = arbgasinfo
        //._get_prices_in_wei(&mut machine, randomish_address.clone())  preserve this for later integration
        ._get_prices_in_wei_with_aggregator(&mut machine, my_address.clone())
        .unwrap();
    assert_eq!(prices2.4, Uint256::zero());
}

#[test]
fn test_set_gas_price_estimate() {
    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();
    let wallet = machine.runtime_env.new_wallet();
    let my_address = Uint256::from_bytes(wallet.address().as_bytes());

    let arbowner = _ArbOwner::_new(&wallet, false);
    let arbgasinfo = _ArbGasInfo::_new(&wallet, false);

    machine.runtime_env.insert_eth_deposit_message(
        my_address.clone(),
        my_address.clone(),
        Uint256::_from_eth(1000),
    );
    let _ = machine.run(None);

    arbowner
        ._set_fees_enabled(&mut machine, true, true)
        .unwrap();

    let new_gas_price = Uint256::from_u64(37);
    let new_storage_price = new_gas_price.mul(&Uint256::from_u64(2_000_000_000_000));

    let storage_price = arbgasinfo
        ._get_prices_in_wei_with_aggregator(&mut machine, my_address.clone())
        .unwrap()
        .2;
    assert!(storage_price != new_storage_price);

    arbowner
        ._set_l1_gas_price_estimate(&mut machine, new_gas_price)
        .unwrap();

    machine
        .runtime_env
        ._advance_time(Uint256::one(), None, false);

    let storage_price = arbgasinfo
        ._get_prices_in_wei_with_aggregator(&mut machine, my_address.clone())
        .unwrap()
        .2;
    assert_eq!(storage_price, new_storage_price);
}
