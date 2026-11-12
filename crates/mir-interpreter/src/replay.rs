//! Concrete function replay before or after MIR lowering.
//!
//! This entry runs validated scalar SSA directly, with caller-supplied memory and no hidden
//! backend frame or heap initialization. It is separate from transaction execution: semantic
//! casts, integer widths, checked arithmetic and checks are meaningful before backend lowering.
//! Missing semantics and resource limits remain unsupported, never guessed execution results.

use crate::{
    Host, Log, Options,
    machine::{Limits, Machine, Memory, Outcome as RunOutcome, scalar_bits},
};
use alloy_primitives::U256;
use solar_interface::{ColorChoice, Session, source_map::FileName};
use solar_mir::{FunctionId, Module};

/// An observed scalar function outcome.
#[derive(Debug)]
pub enum Outcome {
    /// Returned word values.
    Return(Vec<U256>),
    /// Reverted with exact payload bytes.
    Revert(Vec<u8>),
    /// An exceptional halt.
    Invalid,
    /// An operation or state outside the interpreter's supported fragment.
    Unsupported(String),
}

/// Function result and the memory visible to its caller on success.
#[derive(Debug)]
pub struct Execution {
    /// How execution ended.
    pub outcome: Outcome,
    /// The first 256 bytes, padded without expanding memory.
    pub memory: Vec<u8>,
    /// Active memory size in bytes.
    pub memory_size: u64,
}

/// Runs one validated scalar MIR function with explicit arguments and initial memory.
/// Initial memory must be word-aligned and at most the interpreter's 16 MiB limit.
pub fn run(
    text: &str,
    args: &[U256],
    prefix: &[u8],
    size: u64,
    options: Options,
) -> Result<Execution, String> {
    run_function(text, None, args, prefix, size, options)
}

/// Runs a named body in a full module, preserving its parser and replay context.
pub fn run_function(
    text: &str,
    name: Option<&str>,
    args: &[U256],
    prefix: &[u8],
    size: u64,
    options: Options,
) -> Result<Execution, String> {
    let memory = Memory::from_prefix(prefix, size)?;
    with_function(text, name, args, |module, id| {
        let (outcome, memory) = Machine::new(module).replay(
            id,
            args,
            memory,
            Limits { fuel: options.fuel, depth: options.depth },
            options.evm_version,
        );
        let outcome = scalar_outcome(outcome);
        Ok(Execution { outcome, memory: memory.prefix(256), memory_size: memory.size_bytes() })
    })
}

fn with_function<T>(
    text: &str,
    name: Option<&str>,
    args: &[U256],
    run: impl FnOnce(&Module, FunctionId) -> Result<T, String>,
) -> Result<T, String> {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let file = sess
            .source_map()
            .new_source_file(FileName::Custom("replay.mir".into()), text)
            .map_err(|e| e.to_string())?;
        let module =
            Module::parse(&sess, &file).map_err(|_| "cannot parse MIR replay".to_owned())?;
        solar_mir::validate(&sess.dcx, &module);
        if sess.dcx.has_errors().is_err() {
            return Err("invalid MIR replay".into());
        }
        if (name.is_none() && module.functions.len() != 1) || module.backend_memory.is_some() {
            return Err(
                "function replay requires one function without backend memory records".into()
            );
        }
        let mut selected = module
            .functions
            .iter_enumerated()
            .filter(|(_, function)| name.is_none_or(|name| function.name.to_string() == name));
        let (id, function) = selected.next().ok_or("missing replay function")?;
        if selected.next().is_some() {
            return Err("ambiguous replay function".into());
        }
        if function.params.len() != args.len() {
            return Err("mismatched replay arguments".into());
        }
        for (ty, value) in function.params.iter().zip(args) {
            let Some(bits @ 1..=256) = scalar_bits(*ty) else {
                return Err("non-word replay argument".into());
            };
            if bits < 256 && (*value >> bits) != U256::ZERO {
                return Err("unclean replay argument".into());
            }
        }
        run(&module, id)
    })
}

fn scalar_outcome(outcome: RunOutcome) -> Outcome {
    match outcome {
        RunOutcome::Return(values) => Outcome::Return(values.into_vec()),
        RunOutcome::Revert(data) => Outcome::Revert(data),
        RunOutcome::Invalid => Outcome::Invalid,
        RunOutcome::Unsupported(what) => Outcome::Unsupported(what.into()),
        RunOutcome::Limit(what) => Outcome::Unsupported(format!("replay limit: {what:?}")),
        _ => Outcome::Unsupported("non-scalar function termination".into()),
    }
}

/// Explicit input to semantic state replay. No frame, heap or FMP writes are implied.
pub struct StateInput<'a> {
    /// Selected function, or the only function when absent.
    pub function: Option<&'a str>,
    /// Clean scalar arguments, including opaque memory addresses.
    pub arguments: &'a [U256],
    /// Initial memory bytes, padded to `memory_size` without implicit stores.
    pub memory: &'a [u8],
    /// Word-aligned active memory size, bounded to 64 KiB for this replay mode.
    pub memory_size: u64,
    /// Transaction calldata visible to the selected body.
    pub calldata: &'a [u8],
}

/// Semantic state outcome, with complete memory and the journal's changed cells.
pub struct StateExecution {
    /// Return data visible at a normal internal return.
    pub return_data: Vec<u8>,
    /// How the selected body ended; raw RETURN and STOP end the whole call frame.
    pub outcome: StateOutcome,
    /// Active memory on success. Failure memory is not observable.
    pub memory: Vec<u8>,
    /// Final persistent writes, discarded on failure.
    pub storage: Vec<(U256, U256)>,
    /// Final transient writes, discarded on failure.
    pub transient: Vec<(U256, U256)>,
    /// Logs, discarded on failure.
    pub logs: Vec<Log>,
}

/// Function return and transaction termination are distinct at an internal boundary.
pub enum StateOutcome {
    /// Return to the caller with these scalar values.
    Return(Vec<U256>),
    /// STOP or raw RETURN terminated the transaction with these bytes.
    Success(Vec<u8>),
    /// Revert, with exact bytes.
    Revert(Vec<u8>),
    /// Exceptional halt, including static violations or unavailable instructions.
    Invalid,
    /// The interpreter cannot establish the result, including work/memory limits.
    Unsupported(String),
}

/// Runs validated MIR directly, with explicit initial memory and state supplied by `host`.
/// Backend memory records are rejected. Gas is not assigned to semantic instructions.
pub fn run_state(
    text: &str,
    input: StateInput<'_>,
    host: &mut dyn Host,
    options: Options,
) -> Result<StateExecution, String> {
    if input.memory_size > 65536 || input.memory.len() as u64 > input.memory_size {
        return Err("state replay memory limit of 64 KiB".into());
    }
    let memory = Memory::from_prefix(input.memory, input.memory_size)?;
    with_function(text, input.function, input.arguments, |module, id| {
        let entry = module.dispatch_entry() == Some(id);
        let (outcome, memory, effects) = Machine::new(module).replay_state(
            id,
            input.arguments,
            memory,
            host,
            (input.calldata, options.evm_version),
            Limits { fuel: options.fuel, depth: options.depth },
        );
        let outcome = if memory.size_bytes() > 65536 {
            StateOutcome::Unsupported("state replay memory limit of 64 KiB".into())
        } else {
            match outcome {
                RunOutcome::Return(values) if entry && values.is_empty() => {
                    StateOutcome::Success(Vec::new())
                }
                RunOutcome::Return(_) if entry => {
                    StateOutcome::Unsupported("entry returned words".into())
                }
                RunOutcome::Return(values) => StateOutcome::Return(values.into_vec()),
                RunOutcome::ReturnData(data) => StateOutcome::Success(data),
                RunOutcome::Stop => StateOutcome::Success(Vec::new()),
                RunOutcome::Revert(data) => StateOutcome::Revert(data),
                RunOutcome::Invalid | RunOutcome::ReturndataOutOfBounds => StateOutcome::Invalid,
                RunOutcome::Unsupported(what) => StateOutcome::Unsupported(what.into()),
                RunOutcome::Limit(what) => {
                    StateOutcome::Unsupported(format!("state replay limit: {what:?}"))
                }
            }
        };
        let sorted = |map: solar_data_structures::map::FxHashMap<U256, U256>| {
            let mut values = map.into_iter().collect::<Vec<_>>();
            values.sort_unstable();
            values
        };
        Ok(StateExecution {
            return_data: effects.return_data,
            outcome,
            memory: memory.prefix(memory.size_bytes()),
            storage: sorted(effects.storage),
            transient: sorted(effects.transient),
            logs: effects.logs,
        })
    })
}
