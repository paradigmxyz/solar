//! Native execution of MIR child messages through evm2's message API. No MIR
//! instructions are compiled, encoded as bytecode, or reconstructed from Lean.
//! The engine owns account/storage journals, child rollback and creation rules.

use super::super::{Input, Request, address, context, database, spec, word};
use alloy_primitives::{Address, Bytes, Log as EvmLog, LogData, U256};
use evm2::{
    BaseEvmTypes, Evm, EvmFeatures, Precompiles,
    bytecode::Bytecode,
    env::TxEnv,
    evm::{JournalEntry, StateCheckpoint, inspector::Inspector, registry::TxRegistry},
    interpreter::{
        Host as EvmHost, InstrStop, Interpreter, Message, MessageKind, MessageResult,
        derive_create_destination,
    },
    version::GasId,
};
use serde_json::{Value, json};
use solar_evmir::op;
use solar_mir_interpreter::{ExternalMessage, MessageError, MessageOutput, PendingChanges};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub(super) struct Native<'a> {
    request: &'a Request,
    input: &'a Input,
    engine: Evm<'static, BaseEvmTypes>,
    environment: TxEnv,
    owner: Address,
    creation_index: usize,
    builtin_index: usize,
    unsupported: Rc<RefCell<Option<&'static str>>>,
    checkpoint: StateCheckpoint,
    child_gas_spent: u64,
    message_gas: Vec<u64>,
}

struct CodeGuard {
    root: Address,
    unsupported: Rc<RefCell<Option<&'static str>>>,
    work: usize,
}

impl Inspector<BaseEvmTypes> for CodeGuard {
    fn call(
        &mut self,
        _: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message,
    ) -> Option<MessageResult> {
        if message.code_address == self.root {
            *self.unsupported.borrow_mut() = Some("MIR root runtime code is unknown");
            return Some(MessageResult::<BaseEvmTypes> {
                stop: InstrStop::FatalExternalError,
                ..Default::default()
            });
        }
        None
    }

    fn step(&mut self, interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        self.work += 1;
        if self.work > 1_000_000 {
            *self.unsupported.borrow_mut() = Some("native MIR child instruction work limit");
            interp.set_result(Err(InstrStop::FatalExternalError));
            return;
        }
        // Code introspection of the current MIR contract has no bytecode image.
        if matches!(interp.opcode(), op::EXTCODESIZE | op::EXTCODEHASH | op::EXTCODECOPY)
            && interp.stack().peek(0).is_some_and(|v| Address::from_word(v.into()) == self.root)
        {
            *self.unsupported.borrow_mut() = Some("MIR root runtime code is unknown");
            interp.set_result(Err(InstrStop::FatalExternalError));
        }
    }
}

impl<'a> Native<'a> {
    pub(super) fn new(request: &'a Request, input: &'a Input) -> Result<Self, String> {
        let owner = address(request, &input.address)?;
        let db = database(request, input, None)?;
        let initial = db.cache.clone();
        let spec = spec(&request.evm_version)?;
        let mut engine = Evm::new(
            spec,
            context::block(request, input)?,
            TxRegistry::new(),
            db,
            Precompiles::base(spec),
        );
        engine.overlay_db_mut().cache = initial;
        let mut warm = vec![owner, address(request, &input.caller)?];
        warm.extend(
            input
                .warm_addresses
                .iter()
                .map(|a| address(request, a))
                .collect::<Result<Vec<_>, _>>()?,
        );
        warm.extend((1..=9).map(Address::with_last_byte));
        if spec >= evm2::SpecId::CANCUN {
            warm.push(Address::with_last_byte(10));
        }
        if spec >= evm2::SpecId::PRAGUE {
            warm.extend((11..=17).map(Address::with_last_byte));
        }
        if spec >= evm2::SpecId::OSAKA {
            warm.push(Address::from_word(U256::from(256).into()));
        }
        if spec >= evm2::SpecId::SHANGHAI {
            warm.push(address(request, &input.block.coinbase)?);
        }
        for address in warm {
            engine.state_mut().prewarm(&address);
        }
        for (a, k) in &input.warm_slots {
            engine.state_mut().prewarm_storage_slot(&address(request, a)?, word(request, k)?);
        }
        for a in &input.accounts {
            for (k, v) in &a.transient {
                engine.state_mut().tstore(
                    &address(request, &a.address)?,
                    &word(request, k)?,
                    &word(request, v)?,
                );
            }
        }
        let unsupported = Rc::new(RefCell::new(None));
        engine.set_inspector(CodeGuard { root: owner, unsupported: unsupported.clone(), work: 0 });
        let checkpoint = engine.state().checkpoint();
        Ok(Self {
            request,
            input,
            engine,
            checkpoint,
            environment: context::transaction(request, input)?,
            owner,
            creation_index: 0,
            builtin_index: 0,
            child_gas_spent: 0,
            message_gas: Vec::new(),
            unsupported,
        })
    }

    pub(super) fn storage(&mut self, slot: U256) -> U256 {
        EvmHost::sload(&mut self.engine, &self.owner, &slot, false)
            .expect("in-memory storage")
            .value
    }

    pub(super) fn transient(&mut self, slot: U256) -> U256 {
        EvmHost::tload(&mut self.engine, &self.owner, &slot)
    }

    pub(super) fn balance(&mut self) -> U256 {
        self.engine
            .state_mut()
            .account_info_untracked(&self.owner)
            .expect("in-memory account")
            .map(|a| a.balance)
            .unwrap_or_default()
    }

    pub(super) fn account_read(&mut self, opcode: u8, address: U256) -> Option<U256> {
        let address = Address::from_word(address.into());
        if opcode != op::BALANCE && address == self.owner {
            return None;
        }
        let account =
            EvmHost::load_account(&mut self.engine, &address, opcode != op::BALANCE, false).ok()?;
        match opcode {
            op::BALANCE => Some(account.balance),
            op::EXTCODESIZE => Some(U256::from(account.code.original_bytes().len())),
            op::EXTCODEHASH => Some(U256::from_be_bytes(account.code_hash.0)),
            _ => None,
        }
    }

    pub(super) fn code(&mut self, address: U256) -> Option<Vec<u8>> {
        let address = Address::from_word(address.into());
        if address == self.owner {
            return None;
        }
        let account = EvmHost::load_account(&mut self.engine, &address, true, false).ok()?;
        Some(account.code.original_bytes().to_vec())
    }

    pub(super) fn flush(&mut self, pending: PendingChanges<'_>) {
        for &(key, value) in pending.storage {
            EvmHost::sstore(&mut self.engine, &self.owner, &key, &value, false)
                .expect("in-memory storage");
        }
        for &(key, value) in pending.transient {
            EvmHost::tstore(&mut self.engine, &self.owner, &key, &value);
        }
        for log in pending.logs {
            EvmHost::log(
                &mut self.engine,
                EvmLog {
                    address: self.owner,
                    data: LogData::new_unchecked(
                        log.topics.iter().map(|v| v.to_be_bytes::<32>().into()).collect(),
                        Bytes::copy_from_slice(&log.data),
                    ),
                },
            );
        }
    }

    pub(super) fn message_with_memory(
        &mut self,
        message: ExternalMessage<'_>,
        pending: PendingChanges<'_>,
        memory: (u64, u64),
    ) -> Result<MessageOutput, MessageError> {
        self.flush(pending);
        let mut native = Message::<BaseEvmTypes> {
            depth: 1,
            caller_is_static: self.input.is_static,
            ..Default::default()
        };
        let available = self
            .input
            .message_gas
            .as_ref()
            .map(|budgets| {
                budgets
                    .get(self.message_gas.len())
                    .copied()
                    .ok_or(MessageError::Unsupported("missing MIR caller gas budget"))
            })
            .transpose()?;
        let params = self.engine.version().gas_params;
        let features = self.engine.version().features;
        let mut charge = params.memory_cost((memory.1 / 32) as usize)
            - params.memory_cost((memory.0 / 32) as usize);
        let mut retained = 0;
        let creation = matches!(message, ExternalMessage::Create { .. });
        match message {
            ExternalMessage::Call { opcode, gas, address, value, input } => {
                if gas.is_some_and(|gas| gas > U256::from(30_000_000)) {
                    return Err(MessageError::Unsupported(
                        "MIR requested child gas exceeds 30000000",
                    ));
                }
                if gas.is_none()
                    && (available.is_none() || !self.engine.feature(EvmFeatures::EIP150))
                {
                    return Err(MessageError::Unsupported(
                        "implicit address-call gas requires caller pricing",
                    ));
                }
                let target = Address::from_word(address.into());
                let account = EvmHost::load_account(&mut self.engine, &target, true, false)
                    .expect("in-memory account");
                // evm2's owning gas schedule prices memory, account access and forwarding.
                // Legacy CALL base prices precede Berlin's warm/cold schedule (EIP-150/2929).
                charge += if self.engine.feature(EvmFeatures::EIP2929) {
                    u64::from(params.get(GasId::WarmStorageReadCost))
                        + if account.is_cold { params.cold_account_additional_cost() } else { 0 }
                } else if self.engine.feature(EvmFeatures::EIP150) {
                    700
                } else {
                    40
                };
                let transfers = matches!(opcode, op::CALL | op::CALLCODE) && !value.is_zero();
                if transfers {
                    charge += u64::from(params.get(GasId::TransferValueCost));
                }
                if opcode == op::CALL
                    && (!self.engine.feature(EvmFeatures::EIP161) || transfers)
                    && EvmHost::target_is_empty_for_new_account_gas(
                        &mut self.engine,
                        &target,
                        features,
                    )
                    .expect("in-memory account")
                {
                    charge += u64::from(params.get(GasId::NewAccountCost));
                }
                let mut code = account.code;
                let mut code_address = target;
                if self.engine.feature(EvmFeatures::EIP7702)
                    && let Some(delegated) = code.eip7702_address()
                {
                    let delegated_account =
                        EvmHost::load_account(&mut self.engine, &delegated, true, false)
                            .expect("in-memory delegated account");
                    charge += u64::from(params.get(GasId::WarmStorageReadCost))
                        + if delegated_account.is_cold {
                            params.cold_account_additional_cost()
                        } else {
                            0
                        };
                    code = delegated_account.code;
                    code_address = delegated;
                    native.disable_precompiles = true;
                }
                let direct = matches!(opcode, op::CALL | op::STATICCALL);
                native.kind = match opcode {
                    op::CALL => MessageKind::Call,
                    op::CALLCODE => MessageKind::CallCode,
                    op::DELEGATECALL => MessageKind::DelegateCall,
                    _ => MessageKind::StaticCall,
                };
                native.destination = if direct { target } else { self.owner };
                native.code_address = code_address;
                native.caller = if opcode == op::DELEGATECALL {
                    super::super::address(self.request, &self.input.caller)
                        .expect("validated address")
                } else {
                    self.owner
                };
                native.value = value;
                native.code = code;
                native.input = Bytes::copy_from_slice(input);
                let mut forwarded = gas.map(|gas| gas.to::<u64>()).unwrap_or_default();
                if let Some(available) = available {
                    let remaining = available.checked_sub(charge).ok_or(MessageError::OutOfGas)?;
                    if self.engine.feature(EvmFeatures::EIP150) {
                        let cap = params.call_stipend_reduction(remaining);
                        forwarded = if gas.is_some() { forwarded.min(cap) } else { cap };
                    } else if forwarded > remaining {
                        return Err(MessageError::OutOfGas);
                    }
                    retained = remaining - forwarded;
                }
                if code_address == self.owner {
                    return Err(MessageError::Unsupported("MIR root runtime code is unknown"));
                }
                native.gas_limit = forwarded
                    + if transfers { u64::from(params.get(GasId::CallStipend)) } else { 0 };
            }
            ExternalMessage::Create { value, salt, initcode } => {
                let gas = if let Some(available) = available {
                    charge += if salt.is_some() {
                        params.create2_cost(initcode.len())
                    } else {
                        u64::from(params.get(GasId::Create))
                    };
                    if self.engine.feature(EvmFeatures::EIP3860) {
                        charge += params.initcode_cost(initcode.len());
                    }
                    let remaining = available.checked_sub(charge).ok_or(MessageError::OutOfGas)?;
                    let forwarded = if self.engine.feature(EvmFeatures::EIP150) {
                        params.call_stipend_reduction(remaining)
                    } else {
                        remaining
                    };
                    retained = remaining - forwarded;
                    forwarded
                } else {
                    *self
                        .input
                        .creation_gas
                        .get(self.creation_index)
                        .ok_or(MessageError::Unsupported("missing MIR creation gas budget"))?
                };
                self.creation_index += 1;
                native.kind =
                    if salt.is_some() { MessageKind::Create2 } else { MessageKind::Create };
                native.caller = self.owner;
                native.value = value;
                native.salt = salt.unwrap_or_default().into();
                native.code = Bytecode::new_legacy(Bytes::copy_from_slice(initcode));
                native.input = Bytes::copy_from_slice(initcode);
                native.gas_limit = gas;
                let nonce = self
                    .engine
                    .state_mut()
                    .account_info_untracked(&self.owner)
                    .expect("in-memory creator")
                    .map(|a| a.nonce)
                    .unwrap_or_default();
                native.destination = derive_create_destination(
                    native.kind,
                    &self.owner,
                    &native.salt,
                    initcode,
                    nonce,
                );
                native.code_address = native.destination;
            }
        }
        let result = EvmHost::execute_message(&mut self.engine, &self.environment, &mut native);
        if let Some(reason) = *self.unsupported.borrow() {
            return Err(MessageError::Unsupported(reason));
        }
        self.child_gas_spent = self
            .child_gas_spent
            .saturating_add(native.gas_limit.saturating_sub(result.gas.remaining()));
        if self.child_gas_spent > 30_000_000 {
            return Err(MessageError::Unsupported("native MIR child gas work limit"));
        }
        if available.is_some() {
            self.message_gas.push(retained + result.gas.remaining());
        }
        let value = if creation {
            result.created_address_for_parent()
        } else {
            U256::from(u8::from(result.stop.is_success()))
        };
        let data = if creation && result.stop != InstrStop::Revert || result.stop.is_halt() {
            Vec::new()
        } else {
            result.output.to_vec()
        };
        Ok(MessageOutput { value, return_data: data })
    }

    pub(super) fn finish(
        &mut self,
        keep_transient: bool,
        keep_world: bool,
    ) -> Result<Value, String> {
        if !keep_world {
            let features = self.engine.version().features;
            self.engine.state_mut().rollback(self.checkpoint.clone(), features);
        }
        let mut addresses = self
            .input
            .accounts
            .iter()
            .map(|a| address(self.request, &a.address))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let initial_addresses = addresses.clone();
        let mut persistent = BTreeMap::<Address, BTreeSet<U256>>::new();
        let mut transient = BTreeMap::<Address, BTreeSet<U256>>::new();
        let mut warm_slots = BTreeSet::new();
        for a in &self.input.accounts {
            let owner = address(self.request, &a.address)?;
            persistent.entry(owner).or_default().extend(
                a.storage
                    .iter()
                    .map(|(k, _)| word(self.request, k))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            transient.entry(owner).or_default().extend(
                a.transient
                    .iter()
                    .map(|(k, _)| word(self.request, k))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        for (a, k) in &self.input.warm_slots {
            warm_slots.insert((address(self.request, a)?, word(self.request, k)?));
        }
        for event in self.engine.state().journal() {
            match event {
                JournalEntry::AccountChange { address, .. } => {
                    addresses.insert(*address);
                }
                JournalEntry::StorageChange { address, key, .. }
                | JournalEntry::StorageWarmed { address, key } => {
                    addresses.insert(*address);
                    persistent.entry(*address).or_default().insert(*key);
                    warm_slots.insert((*address, *key));
                }
                JournalEntry::TransientStorageChange { address, key, .. } => {
                    addresses.insert(*address);
                    transient.entry(*address).or_default().insert(*key);
                }
                _ => {}
            }
        }
        let mut accounts = Vec::new();
        let mut warm_addresses = BTreeSet::new();
        addresses.extend(
            self.input
                .warm_addresses
                .iter()
                .map(|a| address(self.request, a))
                .collect::<Result<Vec<_>, _>>()?,
        );
        addresses.insert(address(self.request, &self.input.caller)?);
        addresses.extend((1..=17).map(Address::with_last_byte));
        addresses.insert(Address::from_word(U256::from(256).into()));
        addresses.insert(address(self.request, &self.input.block.coinbase)?);
        for owner in addresses {
            let mut handle =
                self.engine.state_mut().account(&owner, false).expect("in-memory account");
            if handle.is_warm() {
                warm_addresses.insert(owner);
            }
            let (created, destroyed) = (handle.is_created(), handle.is_destructed());
            let code =
                handle.load_code().expect("in-memory account code").original_byte_slice().to_vec();
            drop(handle);
            let Some(info) =
                self.engine.state_mut().account_info_untracked(&owner).expect("in-memory account")
            else {
                continue;
            };
            if info.is_empty() && !initial_addresses.contains(&owner)
                || !keep_transient && destroyed
            {
                continue;
            }
            let mut slots = Vec::new();
            for &key in persistent.get(&owner).into_iter().flatten() {
                let value = self
                    .engine
                    .state_mut()
                    .storage_slot_untracked(&owner, &key)
                    .expect("in-memory storage");
                if !value.is_zero() {
                    slots.push([key.to_string(), value.to_string()]);
                }
            }
            let mut cells = Vec::new();
            if keep_transient {
                for &key in transient.get(&owner).into_iter().flatten() {
                    let value = self.engine.state_mut().tload(&owner, &key);
                    if !value.is_zero() {
                        cells.push([key.to_string(), value.to_string()]);
                    }
                }
            }
            accounts.push(json!([
                super::super::number(owner),
                info.nonce.to_string(),
                info.balance.to_string(),
                code,
                slots,
                cells,
                keep_transient && created,
                keep_transient && destroyed
            ]));
        }
        let logs = self
            .engine
            .state()
            .logs()
            .iter()
            .map(|log| {
                json!([
                    super::super::number(log.address),
                    log.data
                        .topics()
                        .iter()
                        .map(|t| U256::from_be_slice(t.as_slice()).to_string())
                        .collect::<Vec<_>>(),
                    log.data.data.to_vec()
                ])
            })
            .collect::<Vec<_>>();
        let observed_warm = if keep_transient {
            warm_addresses.into_iter().map(super::super::number).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let slots = if keep_transient {
            warm_slots
                .into_iter()
                .filter(|(a, k)| {
                    self.engine.state_mut().storage(a).is_warm(k)
                        || self.engine.state().prewarm_set().is_storage_warm(a, k)
                })
                .map(|(a, k)| [super::super::number(a), k.to_string()])
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let boundary_budgets = keep_transient.then_some([self.creation_index, self.builtin_index]);
        Ok(json!({"accounts":accounts,"logs":logs,"warm_addresses":observed_warm,
            "warm_slots":slots,"boundary_budgets":boundary_budgets,"message_gas":self.message_gas}))
    }

    pub(super) fn destroy(
        &mut self,
        beneficiary: U256,
        pending: PendingChanges<'_>,
    ) -> Result<(), &'static str> {
        self.flush(pending);
        let beneficiary = Address::from_word(beneficiary.into());
        EvmHost::selfdestruct(&mut self.engine, &self.owner, &beneficiary, false)
            .map(|_| ())
            .map_err(|_| "native MIR selfdestruct failed")
    }

    pub(super) fn builtin_gas(&mut self) -> Option<U256> {
        let budget = *self.input.builtin_gas.get(self.builtin_index)?;
        self.builtin_index += 1;
        Some(U256::from(budget))
    }
}
