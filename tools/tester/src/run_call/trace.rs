//! What a call did in the EVM, recorded for the interpreter checks to compare against.

use super::CALLER;
use alloy_primitives::{Address, B256, Log, U256, hex};
use evm2::{
    BaseEvmTypes, Evm,
    env::{BlockEnv, TxEnv},
    evm::{AccountInfo, inspector::Inspector},
    interpreter::{InstrStop, Interpreter},
};
use solar_evmir::op;
use std::{cell::Cell, collections::HashMap, rc::Rc};

/// An account as the called contract sees it.
struct Account {
    balance: U256,
    code: Vec<u8>,
    code_hash: B256,
}

/// The chain state the called contract saw, captured from the EVM just before the call.
pub(super) struct Chain {
    contract: Address,
    pub(super) storage: HashMap<U256, U256>,
    accounts: HashMap<Address, Account>,
}

impl Chain {
    /// Captures the state of `evm`, whose next transaction calls `contract`.
    ///
    /// The accepted-state overlay holds every account the deployment and `setUp()` touched,
    /// which includes every account the call can observe.
    pub(super) fn capture(evm: &Evm<'_, BaseEvmTypes>, contract: Address) -> Self {
        let cache = &evm.overlay_db().cache;
        let accounts = cache
            .accounts
            .iter()
            .filter_map(|(address, info)| Some((*address, info.as_ref()?)))
            .map(|(address, info): (Address, &AccountInfo)| {
                let code = info.code.as_ref().or_else(|| cache.contracts.get(&info.code_hash));
                let code = code.map(|code| code.original_byte_slice().to_vec()).unwrap_or_default();
                (address, Account { balance: info.balance, code, code_hash: info.code_hash })
            })
            .collect();
        Self { contract, storage: storage(evm, contract), accounts }
    }

    /// Returns the code deployed at the called contract.
    pub(super) fn deployed_code(&self) -> Option<&[u8]> {
        self.accounts.get(&self.contract).map(|account| &account.code[..])
    }
}

/// Returns the persistent storage of `contract` in `evm`.
fn storage(evm: &Evm<'_, BaseEvmTypes>, contract: Address) -> HashMap<U256, U256> {
    let storage = evm.overlay_db().cache.storage.get(&contract);
    storage.map_or_else(HashMap::new, |storage| {
        storage.slots.iter().map(|(&slot, &value)| (slot, value)).collect()
    })
}

/// Records the first value the EVM stores at the free memory pointer's slot and the last opcode the
/// EVM ran.
///
/// The stored value is the heap start only when the entry code sets the free memory pointer, so
/// the MIR check uses it only for a module without backend memory records, which carry the heap
/// starts the backend chose.
#[derive(Clone, Default)]
pub(super) struct Observer {
    heap_start: Rc<Cell<Option<U256>>>,
    last_opcode: Rc<Cell<Option<u8>>>,
}

impl Inspector<BaseEvmTypes> for Observer {
    fn step(&mut self, interpreter: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        self.last_opcode.set(Some(interpreter.opcode()));
        let stack = interpreter.stack();
        if self.heap_start.get().is_none()
            && interpreter.opcode() == op::MSTORE
            && stack.peek(0) == Some(U256::from(0x40))
        {
            self.heap_start.set(stack.peek(1));
        }
    }
}

/// What a call did in the EVM.
pub(super) struct Trace {
    pub(super) before: Chain,
    pub(super) stop: InstrStop,
    pub(super) output: Vec<u8>,
    logs: Vec<Log>,
    storage: HashMap<U256, U256>,
    pub(super) heap_start: Option<U256>,
    pub(super) last_opcode: Option<u8>,
}

impl Trace {
    /// Records the call that took `evm` from `before` to its current state, as `observer` saw it.
    pub(super) fn new(
        before: Chain,
        evm: &Evm<'_, BaseEvmTypes>,
        stop: InstrStop,
        output: &[u8],
        logs: &[Log],
        observer: &Observer,
    ) -> Self {
        let storage = storage(evm, before.contract);
        Self {
            before,
            stop,
            output: output.to_vec(),
            logs: logs.to_vec(),
            storage,
            heap_start: observer.heap_start.get(),
            last_opcode: observer.last_opcode.get(),
        }
    }

    /// Describes how the logs and storage writes of an interpreted run, `logs` and `written`,
    /// differ from the EVM's, naming the interpreted program `program`.
    pub(super) fn effect_differences(
        &self,
        logs: Vec<(Vec<U256>, Vec<u8>)>,
        written: &[(U256, U256)],
        program: &str,
    ) -> Vec<String> {
        let mut differences = Vec::new();
        let evm_logs = self
            .logs
            .iter()
            .map(|log| {
                let topics = log.data.topics().iter().map(|topic| U256::from_be_bytes(topic.0));
                (topics.collect::<Vec<_>>(), log.data.data.to_vec())
            })
            .collect::<Vec<_>>();
        if evm_logs != logs {
            differences.push(format!(
                "the EVM logged {}, the {program} {}",
                describe_logs(&evm_logs),
                describe_logs(&logs)
            ));
        }
        let written = written.iter().copied().collect::<HashMap<_, _>>();
        for (&slot, &value) in &written {
            let evm = self.storage.get(&slot).copied().unwrap_or_default();
            if evm != value {
                differences.push(format!(
                    "slot {slot:#x} holds {evm:#x} in the EVM, {value:#x} in the {program}"
                ));
            }
        }
        for (&slot, &value) in &self.storage {
            let before = self.before.storage.get(&slot).copied().unwrap_or_default();
            if value != before && !written.contains_key(&slot) {
                differences.push(format!(
                    "the EVM wrote {value:#x} to slot {slot:#x}, the {program} did not"
                ));
            }
        }
        differences
    }
}

/// Describes events as their topics and data, in hex.
fn describe_logs(logs: &[(Vec<U256>, Vec<u8>)]) -> String {
    let logs = logs.iter().map(|(topics, data)| {
        let topics = topics.iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>();
        format!("topics [{}] data 0x{}", topics.join(", "), hex::encode(data))
    });
    format!("[{}]", logs.collect::<Vec<_>>().join("; "))
}

/// Answers an interpreter's context reads from the captured chain state and the values the EVM
/// was configured with.
pub(super) struct Context<'a> {
    chain: &'a Chain,
    value: U256,
    block: BlockEnv<BaseEvmTypes>,
}

impl<'a> Context<'a> {
    /// Answers for a call that sends `value` to the contract in `chain`.
    pub(super) fn new(chain: &'a Chain, value: U256) -> Self {
        Self { chain, value, block: BlockEnv::<BaseEvmTypes>::default() }
    }

    /// Returns what `opcode` reads for these operands during the call, or `None` when it does not
    /// read the context.
    pub(super) fn read(&self, opcode: u8, operands: &[U256]) -> Option<U256> {
        let address = || operands.first().map(|word| Address::from_word(word.to_be_bytes().into()));
        let account = |address: Address| self.chain.accounts.get(&address);
        Some(match opcode {
            op::ADDRESS => self.chain.contract.into_word().into(),
            op::ORIGIN | op::CALLER => CALLER.into_word().into(),
            op::CALLVALUE => self.value,
            // The call is a legacy transaction with a zero gas price and no blobs, in the first
            // block, which has no ancestors to hash.
            op::GASPRICE | op::BLOCKHASH | op::BLOBHASH => U256::ZERO,
            op::CHAINID => TxEnv::<BaseEvmTypes>::default().chain_id,
            op::COINBASE => self.block.beneficiary.into_word().into(),
            op::TIMESTAMP => self.block.timestamp,
            op::NUMBER => self.block.number,
            op::PREVRANDAO => self.block.prevrandao,
            op::GASLIMIT => self.block.gas_limit,
            op::BASEFEE => self.block.basefee,
            op::BLOBBASEFEE => self.block.blob_basefee,
            op::SLOTNUM => self.block.slot_num,
            op::SELFBALANCE => self.balance(self.chain.contract),
            op::BALANCE => self.balance(address()?),
            op::CODESIZE => U256::from(account(self.chain.contract)?.code.len()),
            op::EXTCODESIZE => {
                U256::from(account(address()?).map_or(0, |account| account.code.len()))
            }
            op::EXTCODEHASH => {
                account(address()?).map_or(U256::ZERO, |account| account.code_hash.into())
            }
            _ => return None,
        })
    }

    /// Returns the value a persistent storage slot of the contract held before the call.
    pub(super) fn storage(&self, slot: U256) -> U256 {
        self.chain.storage.get(&slot).copied().unwrap_or_default()
    }

    /// Returns an account's balance during the call, after its value moved to the contract.
    fn balance(&self, address: Address) -> U256 {
        let before =
            self.chain.accounts.get(&address).map_or(U256::ZERO, |account| account.balance);
        if address == self.chain.contract {
            before + self.value
        } else if address == CALLER {
            before - self.value
        } else {
            before
        }
    }
}
