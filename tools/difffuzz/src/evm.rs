//! Runs bytecode on revm, an EVM implementation independent of the one Solar's tests use.

use crate::obs::{Obs, Status};
use alloy_primitives::{Address, Bytes, U256};
use revm::{
    Context, ExecuteCommitEvm, InspectEvm, Inspector, MainBuilder, MainContext,
    context::{
        TxEnv,
        result::{ExecutionResult, Output},
    },
    database::{CacheDB, EmptyDB},
    interpreter::{Interpreter, InterpreterTypes, interpreter_types::Jumps},
    primitives::{TxKind, hardfork::SpecId},
};
use std::collections::BTreeMap;

/// The caller of every transaction, as SolidCore's harness fixes it.
pub(crate) const CALLER: Address =
    Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x20, 0]);

const GAS: u64 = 16_000_000;

/// Records the first value stored at the free memory pointer's slot, the heap start the
/// interpreters need, and the last opcode.
#[derive(Default)]
pub(crate) struct Observer {
    pub(crate) heap_start: Option<U256>,
    pub(crate) last_opcode: Option<u8>,
}

impl<CTX, INTR: InterpreterTypes> Inspector<CTX, INTR> for Observer
where
    INTR::Stack: revm::interpreter::interpreter_types::StackTr,
{
    fn step(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        use revm::interpreter::interpreter_types::StackTr;
        let op = interp.bytecode.opcode();
        self.last_opcode = Some(op);
        if self.heap_start.is_none() && op == 0x52 {
            let data = interp.stack.data();
            let n = data.len();
            if n >= 2 && data[n - 1] == U256::from(0x40) {
                self.heap_start = Some(data[n - 2]);
            }
        }
    }
}

type Db = CacheDB<EmptyDB>;

/// One chain with one deployed contract.
pub(crate) struct Chain {
    db: Option<Db>,
    pub(crate) address: Address,
    nonce: u64,
}

impl Chain {
    pub(crate) fn new() -> Self {
        Self { db: Some(Db::default()), address: Address::ZERO, nonce: 0 }
    }

    fn run(
        &mut self,
        kind: TxKind,
        data: Vec<u8>,
    ) -> (ExecutionResult, BTreeMap<U256, U256>, Observer) {
        let db = self.db.take().unwrap();
        let ctx = Context::mainnet().with_db(db).modify_cfg_chained(|cfg| {
            cfg.spec = SpecId::OSAKA;
            cfg.disable_nonce_check = true;
            cfg.limit_contract_code_size = Some(usize::MAX);
        });
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 0;
            b.gas_limit = GAS;
        });
        let mut evm = ctx.build_mainnet_with_inspector(Observer::default());
        let tx = TxEnv::builder()
            .caller(CALLER)
            .kind(kind)
            .data(Bytes::from(data))
            .gas_limit(GAS)
            .gas_price(0)
            .nonce(self.nonce)
            .build()
            .unwrap();
        self.nonce += 1;
        let out = evm.inspect_tx(tx).expect("revm rejected the transaction");
        let mut written = BTreeMap::new();
        let target = match (&out.result, kind) {
            (ExecutionResult::Success { output: Output::Create(_, Some(a)), .. }, _) => Some(*a),
            (_, TxKind::Call(a)) => Some(a),
            _ => None,
        };
        if let Some(target) = target
            && let Some(account) = out.state.get(&target)
        {
            for (slot, value) in &account.storage {
                if value.is_changed() {
                    written.insert(*slot, value.present_value);
                }
            }
        }
        evm.commit(out.state);
        let observer = std::mem::take(&mut evm.inspector);
        self.db = Some(evm.ctx.journaled_state.database);
        (out.result, written, observer)
    }

    /// Deploys `initcode`; returns the observation of the constructor.
    pub(crate) fn deploy(&mut self, initcode: &[u8]) -> Obs {
        let (result, _, observer) = self.run(TxKind::Create, initcode.to_vec());
        let (mut obs, created) = obs(result, observer.last_opcode);
        if let Status::Success = obs.status {
            self.address = created.unwrap_or_default();
            obs.output.clear();
        }
        obs.storage = self.storage();
        obs
    }

    /// Calls the contract; also returns the heap start the call stored.
    pub(crate) fn call(&mut self, calldata: &[u8]) -> (Obs, Option<U256>) {
        let (result, written, observer) = self.run(TxKind::Call(self.address), calldata.to_vec());
        let (mut obs, _) = obs(result, observer.last_opcode);
        obs.writes = written;
        obs.storage = self.storage();
        (obs, observer.heap_start)
    }

    /// The contract's nonzero storage.
    pub(crate) fn storage(&self) -> BTreeMap<U256, U256> {
        let db = self.db.as_ref().unwrap();
        db.cache
            .accounts
            .get(&self.address)
            .map(|a| {
                a.storage.iter().filter(|(_, v)| !v.is_zero()).map(|(k, v)| (*k, *v)).collect()
            })
            .unwrap_or_default()
    }

    /// The deployed runtime code.
    pub(crate) fn code(&self) -> Vec<u8> {
        let db = self.db.as_ref().unwrap();
        db.cache
            .accounts
            .get(&self.address)
            .and_then(|a| a.info.code.as_ref())
            .map(|c| c.original_byte_slice().to_vec())
            .unwrap_or_default()
    }
}

fn obs(result: ExecutionResult, last: Option<u8>) -> (Obs, Option<Address>) {
    let mut created = None;
    let obs = match result {
        ExecutionResult::Success { output, logs, .. } => {
            let data = match output {
                Output::Call(b) => b.to_vec(),
                Output::Create(b, a) => {
                    created = a;
                    b.to_vec()
                }
            };
            let logs = logs
                .into_iter()
                .map(|l| {
                    (
                        l.data.topics().iter().map(|t| U256::from_be_bytes(t.0)).collect(),
                        l.data.data.to_vec(),
                    )
                })
                .collect();
            Obs { status: Status::Success, output: data, logs, ..Obs::default() }
        }
        ExecutionResult::Revert { output, .. } => {
            Obs { status: Status::Revert, output: output.to_vec(), ..Obs::default() }
        }
        ExecutionResult::Halt { reason, .. } => {
            Obs { status: Status::Halt(format!("{reason:?} at opcode {last:?}")), ..Obs::default() }
        }
    };
    (obs, created)
}
