//! One scheduled EVM IR window with explicit live-in state.
//!
//! Runs the existing interpreter's instructions, stopping at the first outgoing transfer or
//! terminal opcode. Gas replay uses opcode-table prices and memory growth; calls, storage and
//! other dynamic gas schedules remain unsupported. Transaction execution is unchanged.

use crate::{
    Host, Options,
    machine::{Layout, Machine, Outcome as RunOutcome},
};
use alloy_primitives::U256;
use solar_evmir::ir::Module;
use solar_interface::{ColorChoice, Session, source_map::FileName};

/// Incoming state at the first instruction, after any entry JUMPDEST.
#[derive(Default)]
pub struct Input {
    /// Stack in top-first order.
    pub stack: Vec<U256>,
    /// Materialized prefix, at most the active memory size.
    pub memory: Vec<u8>,
    /// Active byte count, word aligned and at most 16 MiB.
    pub memory_size: usize,
    /// Explicit context sizes; calldata bytes are not supplied by this entry.
    pub calldata_size: U256,
    /// Explicit return-data size; its bytes are not supplied by this entry.
    pub returndata_size: U256,
    /// Optional gas limit, separate from interpreter work fuel.
    pub gas: Option<U256>,
}

/// Boundary reached by a window.
#[derive(Debug)]
pub enum Outcome {
    /// A block index, or the symbolic fallthrough edge.
    Edge(Option<usize>),
    /// Exact successful output bytes.
    Success(Vec<u8>),
    /// Exact reverted output bytes.
    Revert(Vec<u8>),
    /// An exceptional halt.
    Halt(crate::Halt),
    /// Unsupported behavior or an interpreter resource limit.
    Unsupported(String),
}

/// Observations at the window boundary.
#[derive(Debug)]
pub struct Execution {
    /// Boundary reached.
    pub outcome: Outcome,
    /// Outgoing stack, top first.
    pub stack: Vec<U256>,
    /// First 256 memory bytes, padded without expansion.
    pub memory: Vec<u8>,
    /// Active memory size in bytes.
    pub memory_size: usize,
    /// Remaining gas when charging is enabled.
    pub gas: Option<U256>,
}

/// Replays the first block without entering its successors.
pub fn run(text: &str, input: Input, options: Options) -> Result<Execution, String> {
    run_window(text, None, input, options)
}

/// Replays a complete block in its original module, identified by stable textual label.
pub fn run_window(
    text: &str,
    label: Option<&str>,
    input: Input,
    options: Options,
) -> Result<Execution, String> {
    if input.stack.len() > 1024
        || input.memory_size > (1 << 24)
        || !input.memory_size.is_multiple_of(32)
        || input.memory.len() > input.memory_size
    {
        return Err("invalid or oversized window state".into());
    }
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let file = sess
            .source_map()
            .new_source_file(FileName::Custom("replay.evmir".into()), text)
            .map_err(|e| e.to_string())?;
        let module =
            Module::parse(&sess, &file).map_err(|_| "cannot parse EVM IR replay".to_owned())?;
        let selected = if let Some(label) = label {
            let label = label
                .strip_prefix("bb")
                .ok_or("invalid replay label")?
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            let (id, _) = module
                .blocks
                .iter_enumerated()
                .find(|(_, block)| block.label == label)
                .ok_or("missing replay block")?;
            let heights = solar_evmir::ir::verify::Verifier::stack_entry_exact_depths(
                &module,
                options.evm_version,
            );
            if heights[id] != Some(input.stack.len()) {
                return Err("unproved replay entry height".into());
            }
            Some(id)
        } else {
            None
        };
        Machine::new(&module, options.evm_version).replay(
            input,
            options.fuel,
            Layout::COMPACT,
            selected,
        )
    })
}

pub(crate) struct Unknown;
impl Host for Unknown {
    fn read(&mut self, _: u8, _: &[U256]) -> Option<U256> {
        None
    }
    fn storage(&mut self, _: U256) -> U256 {
        U256::ZERO
    }
}

pub(crate) fn outcome(result: Result<Option<usize>, RunOutcome>) -> Outcome {
    match result {
        Ok(target) => Outcome::Edge(target),
        Err(RunOutcome::Return(data)) => Outcome::Success(data),
        Err(RunOutcome::Stop) => Outcome::Success(Vec::new()),
        Err(RunOutcome::Revert(data)) => Outcome::Revert(data),
        Err(RunOutcome::Halt(halt)) => Outcome::Halt(halt),
        Err(RunOutcome::Limit(limit)) => Outcome::Unsupported(format!("window limit: {limit:?}")),
        Err(RunOutcome::Unsupported(reason)) => Outcome::Unsupported(reason.into_owned()),
    }
}
