//! Concrete function replay before or after MIR lowering.
//!
//! This entry runs validated scalar SSA directly, with caller-supplied memory and no hidden
//! backend frame or heap initialization. It is separate from transaction execution: semantic
//! casts, integer widths, checked arithmetic and checks are meaningful before backend lowering.
//! Missing semantics and resource limits remain unsupported, never guessed execution results.

use crate::{
    Options,
    machine::{Limits, Machine, Memory, Outcome as RunOutcome},
};
use alloy_primitives::U256;
use solar_interface::{ColorChoice, Session, source_map::FileName};
use solar_mir::Module;

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
            let Some(bits @ 1..=256) = ty.integer_bits() else {
                return Err("non-word replay argument".into());
            };
            if bits < 256 && (*value >> bits) != U256::ZERO {
                return Err("unclean replay argument".into());
            }
        }
        let (outcome, memory) = Machine::new(&module).replay(
            id,
            args,
            memory,
            Limits { fuel: options.fuel, depth: options.depth },
            options.evm_version,
        );
        let outcome = match outcome {
            RunOutcome::Return(values) => Outcome::Return(values.into_vec()),
            RunOutcome::Revert(data) => Outcome::Revert(data),
            RunOutcome::Invalid => Outcome::Invalid,
            RunOutcome::Unsupported(what) => Outcome::Unsupported(what.into()),
            RunOutcome::Limit(what) => Outcome::Unsupported(format!("replay limit: {what:?}")),
            _ => Outcome::Unsupported("non-scalar function termination".into()),
        };
        Ok(Execution { outcome, memory: memory.prefix(256), memory_size: memory.size_bytes() })
    })
}
