//! Runs a contract's lowered MIR on a transaction.
//!
//! The UI test runner executes every `run-call` directive in an EVM. With `SOLAR_RUN_CALL_MIR`
//! set, it also runs the call through [`transact`] on the MIR the compiler lowered to that
//! bytecode. Both execute the same compiled program, so a disagreement is a bug in the backend or
//! in the interpreter the `llm-optimize` pass trusts.
//!
//! The interpreter models one contract: calls, contract creation, and `gas` end a run as
//! unsupported, and so does a module the compiler dumped before lowering it, as `-O none` does.

use crate::mir::{
    MirPhase, Module,
    utils::interp::{Limit, Limits, Machine, Outcome as RunOutcome},
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_interface::{ColorChoice, Session, source_map::FileName};

pub use crate::mir::utils::interp::{Host, Log};

/// The bounds of one transaction, well past what the tests execute.
const LIMITS: Limits = Limits { fuel: 10_000_000, depth: 1024 };

/// How a transaction ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It returned this data with `RETURN`, or stopped with none.
    Success(Vec<u8>),
    /// It reverted with this data.
    Revert(Vec<u8>),
    /// It reached `INVALID` or another exceptional halt.
    Halt,
    /// The interpreter cannot run it, for this reason.
    Unsupported(String),
}

/// What a transaction did.
#[derive(Clone, Debug)]
pub struct Execution {
    /// How it ended.
    pub outcome: Outcome,
    /// The events it logged, in order.
    pub logs: Vec<Log>,
    /// The persistent storage slots it wrote, with their final values, ordered by slot.
    pub storage: Vec<(U256, U256)>,
}

/// Runs a transaction with `calldata` on the contract whose lowered MIR module `mir` holds, as
/// `-Zdump=mir-final` prints it for `evm_version`, asking `host` for its context.
pub fn transact(
    mir: &str,
    calldata: &[u8],
    host: &mut dyn Host,
    evm_version: EvmVersion,
) -> Execution {
    let unsupported = |reason: String| Execution {
        outcome: Outcome::Unsupported(reason),
        logs: Vec::new(),
        storage: Vec::new(),
    };
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let name = FileName::Custom("run-call.mir".into());
        let file = match sess.source_map().new_source_file(name, mir) {
            Ok(file) => file,
            Err(error) => return unsupported(format!("unreadable MIR: {error}")),
        };
        let Ok(module) = Module::parse(&sess, &file) else {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return unsupported(format!("unparsable MIR: {}", diagnostics.unwrap_or_default()));
        };
        if module.phase() != MirPhase::Lowered {
            return unsupported("MIR dumped before lowering".into());
        }
        let execution = Machine::new(&module).transact(calldata, host, evm_version, LIMITS);
        let outcome = match execution.outcome {
            RunOutcome::ReturnData(data) => Outcome::Success(data),
            RunOutcome::Stop => Outcome::Success(Vec::new()),
            RunOutcome::Revert(data) => Outcome::Revert(data),
            RunOutcome::Invalid => Outcome::Halt,
            RunOutcome::Limit(Limit::Fuel) => Outcome::Unsupported("runs too long".into()),
            RunOutcome::Limit(Limit::Depth) => {
                Outcome::Unsupported("nests calls too deeply".into())
            }
            RunOutcome::Limit(Limit::Memory) => {
                Outcome::Unsupported("reaches past 16 MiB of memory".into())
            }
            RunOutcome::Unsupported(what) => Outcome::Unsupported(format!("reaches `{what}`")),
            RunOutcome::Return(_) => Outcome::Unsupported("returns from the entry".into()),
        };
        let mut storage = execution.storage.into_iter().collect::<Vec<_>>();
        storage.sort_unstable();
        Execution { outcome, logs: execution.logs, storage }
    })
}
