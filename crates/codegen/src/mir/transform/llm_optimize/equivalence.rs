//! Differential tests of candidates against their original.
//!
//! [`Tests::new`] generates inputs for a function from a seed, runs the original on each through
//! the interpreter, and keeps the runs that finish within its limits. [`Tests::check`] runs a
//! candidate on the same inputs, plus inputs at the constants the candidate adds, and compares the
//! runs:
//!
//! - Both must end the same way: return the same words, revert or return the same payload, stop, or
//!   reach `invalid`. A candidate that exceeds its limits where the original finished fails.
//! - The candidate may only write memory bytes the original writes on the same input. The backend
//!   keeps its call frames and spill slots in memory no MIR function addresses, so a byte the
//!   original never writes may belong to another function; a scratch word may be one a caller keeps
//!   across the call.
//! - When the original returns, the bytes it wrote must end with the same contents.
//! - When the original returns or ends the call without reverting, the candidate may only write
//!   persistent and transient storage slots the original writes, every slot the original writes
//!   must end with the same value, and both must log the same events in the same order. A write the
//!   original does not make would also fail in a static call, where the original succeeds.
//!
//! The inputs must also exercise the code. Every reachable block must run to its terminator, every
//! decision must come out both true and false, and every other value must come out nonzero, on
//! some input: in the original, or the function is not tested at all, and in the candidate, or it
//! is rejected. A revert discards what its run wrote, so a block from which the function can end
//! without reverting, and each of its values, only count in runs that do not revert. Decisions are
//! the comparisons and the `and`, `or`, and `xor` of boolean words, which is how if-converted code
//! combines them. They matter beyond blocks because such code decides without branching: a check
//! that never comes out true would hide every change to what depends on it. A value that is zero on
//! every input hides changes the same way, as on a path the tests only complete with null pointers,
//! where every field it loads and masks is zero.
//!
//! # Inputs
//!
//! Constants come from the function and every function it can call, with their neighbors and, for
//! `bytesN`-style comparisons, their left-aligned forms. Arguments mix zero; powers of two, their
//! neighbors, and their negations; constants; small numbers; words near the free memory pointer;
//! repeats of earlier arguments for aliasing; and random words, each masked to its type so that it
//! holds the clean bits the type promises. `memptr` arguments point near the free memory pointer.
//! Probes then place each constant in each word argument of an earlier input and, for functions
//! that read storage or their context, make each constant every answer of the world, or half of
//! them.
//!
//! Memory holds seeded garbage except for the zero word at `0x60`, the free memory pointer at
//! `0x40`, which varies from `0x80` up and is sometimes unaligned, and the objects at pointer
//! arguments: a small length or a constant, which keeps loops short, followed by small numbers
//! such as flags, and constants. Memory starts as large as the free memory pointer. Its garbage
//! holds small numbers, addresses near the heap start, and constants as well as random words, so
//! that chains of loads through pointers reach nested objects.
//!
//! Each input also has a world derived from its seed, which answers storage and context reads: a
//! storage slot or context value is zero, a small number, a constant, one of the input's
//! arguments, the caller, or a random word, cut to the width the value has on chain, such as 160
//! bits for addresses. Only addresses keep their width everywhere: the EVM bounds no other value,
//! so a quarter of the worlds, and every probe, answer the rest with whole words. Each read draws
//! from a hash of the seed and the whole slot or operands, so that no two slots share their
//! draws. Transient storage is zero more often, as every
//! transaction starts it empty. The tests answer no reads of code sizes or hashes: a rewrite
//! changes the contract's own code, and with it what such reads return on chain.
//!
//! Testing is not proof: a difference on an input no generator reaches goes unnoticed.

use super::cost::{GasMeter, code_bytes};
use crate::{
    backend::evm::op,
    llm::{CostReport, Stage},
    mir::{
        BlockId, Function, FunctionId, InstId, InstKind, MirType, Module, Terminator, Value,
        ValueId,
        analysis::{CallGraphInfo, CfgInfo},
        memory::EvmMemoryLayout,
        utils::interp::{
            self, Execution, Host, Limit, Limits, Log, MEMORY_LIMIT, Machine, Memory, Outcome,
            mix64,
        },
    },
    target::Target,
};
use alloy_primitives::{B256, Keccak256, U256, hex, keccak256};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::{fmt::Write, sync::Arc};

/// Runs of the original that must finish before its tests count.
const MIN_FINISHED_RUNS: usize = 16;
/// Fuel for one run of the original: loops of a few thousand iterations finish.
const ORIGINAL_FUEL: u64 = 20_000;
/// Call depth of one run.
const DEPTH: usize = 64;
/// The most inputs added at the constants a candidate introduces.
const MAX_CONSTANT_INPUTS: usize = 64;
/// The most probes placing the original's constants in its arguments.
const MAX_PROBES: usize = 512;
/// The most constants that probes place in the original's storage and context, three probes each.
const MAX_WORLD_PROBES: usize = 128;
/// The most words of the object at a pointer argument that inputs seed.
const OBJECT_WORDS: u64 = 8;
/// The most memory, in words, a run may grow by and still price the function: 64 KiB. Runs that
/// grow further got an argument that acts as a far pointer, which real calls do not pass, and the
/// quadratic cost of that memory would drown every other cost.
const PRICED_GROWTH_WORDS: u64 = 2048;
/// The longest payload a report spells out, in bytes.
const MAX_REPORTED_BYTES: usize = 68;
/// The most storage and context reads a counterexample lists.
const MAX_REPORTED_READS: usize = 8;

/// The context reads the tests answer: all a host can answer except the sizes and hashes of code,
/// which a rewrite changes for the contract's own code.
const CONTEXT: [u8; 18] = [
    op::ADDRESS,
    op::BALANCE,
    op::ORIGIN,
    op::CALLER,
    op::CALLVALUE,
    op::GASPRICE,
    op::BLOCKHASH,
    op::COINBASE,
    op::TIMESTAMP,
    op::NUMBER,
    op::PREVRANDAO,
    op::GASLIMIT,
    op::CHAINID,
    op::SELFBALANCE,
    op::BASEFEE,
    op::BLOBHASH,
    op::BLOBBASEFEE,
    op::SLOTNUM,
];

/// Returns whether the tests run an instruction of this kind.
pub(super) fn runs(kind: &InstKind) -> bool {
    interp::supports_with_host(kind, &CONTEXT)
}

/// Why a candidate failed.
#[derive(Clone, Debug)]
pub(super) struct Rejection {
    pub(super) stage: Stage,
    pub(super) reason: String,
    pub(super) counterexample: Option<String>,
}

impl Rejection {
    pub(super) fn new(stage: Stage, reason: impl Into<String>) -> Self {
        Self { stage, reason: reason.into(), counterexample: None }
    }
}

/// One generated input.
#[derive(Clone, Debug)]
struct Input {
    args: SmallVec<[U256; 4]>,
    memory: Memory,
    free_memory_pointer: U256,
    /// The seed of the storage and context the input runs with.
    world: u64,
    /// A word the world answers some of its reads with, when the input probes it.
    focus: Option<Focus>,
}

/// A word a probe's world answers reads with: every read when `share` is one, half when two.
#[derive(Clone, Copy, Debug)]
struct Focus {
    word: U256,
    share: u64,
}

/// A finished run of the original.
struct Reference {
    input: Input,
    execution: Execution,
    gas: u64,
    /// Whether the run prices the function: an input that is not a probe, which exercises edge
    /// cases, and does not grow memory by more than [`PRICED_GROWTH_WORDS`].
    priced: bool,
}

/// Generated inputs and the original's runs on them.
pub(super) struct Tests<'a> {
    target: Target,
    module: &'a Module,
    id: FunctionId,
    seed: u64,
    constants: FxHashSet<U256>,
    /// The constants, in order, which inputs and their worlds draw words from.
    palette: Arc<[U256]>,
    /// Whether the function or a function it calls reads or writes storage or its context.
    world: bool,
    references: Vec<Reference>,
    baseline: CostReport,
}

impl<'a> Tests<'a> {
    /// Runs function `id` on `count` inputs generated from `seed`. Fails when too few runs
    /// finish or a reachable block never runs.
    pub(super) fn new(
        target: Target,
        module: &'a Module,
        id: FunctionId,
        seed: u64,
        count: usize,
    ) -> Result<Self, String> {
        let function = module.function(id);
        let constants = call_tree_constants(module, id);
        let mut sorted = constants.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        let palette = Arc::<[U256]>::from(sorted.clone());
        let mut rng = Rng(seed);
        let machine = Machine::new(module);
        let mut meter = GasMeter::new(target, module);
        let mut references = Vec::new();
        let mut run = |input: Input, probe: bool, references: &mut Vec<Reference>| {
            let limits = Limits { fuel: ORIGINAL_FUEL, depth: DEPTH };
            let mut world = World::new(&input, &palette);
            let memory = input.memory.clone();
            let execution =
                machine.run(id, &input.args, memory, Some(&mut world), limits, &mut meter);
            let spent = meter.take();
            match execution.outcome {
                Outcome::Unsupported(what) => Err(format!("reaches unsupported `{what}`")),
                Outcome::Limit(_) => Ok(()),
                _ => {
                    let gas = run_gas(target, &input, &execution, spent);
                    let growth = execution.memory.size().saturating_sub(input.memory.size());
                    let priced = !probe && growth <= PRICED_GROWTH_WORDS;
                    references.push(Reference { input, execution, gas, priced });
                    Ok(())
                }
            }
        };
        for _ in 0..count {
            run(generate(&mut rng, function, &sorted, &palette), false, &mut references)?;
        }
        if references.len() < MIN_FINISHED_RUNS {
            return Err(format!("finishes only {} of {count} test runs", references.len()));
        }
        // Probes: each constant in each word argument of an earlier input, in seeded order.
        sorted.sort_by_key(|constant| mix64(seed ^ constant.as_limbs()[0]));
        let mut probes = Vec::new();
        'probes: for (index, &constant) in sorted.iter().enumerate() {
            for (param, &ty) in function.params.iter().enumerate() {
                if probes.len() == MAX_PROBES {
                    break 'probes;
                }
                if ty == MirType::MemPtr {
                    continue;
                }
                let mut input = references[(index + param) % references.len()].input.clone();
                input.args[param] = mask(constant, ty);
                probes.push(input);
            }
        }
        // World probes: each constant as every storage and context read of an earlier input, and
        // as half of them in two more.
        let world = reads_world(module, id);
        if world {
            for (index, &word) in sorted.iter().take(MAX_WORLD_PROBES).enumerate() {
                for (round, share) in [1, 2, 2].into_iter().enumerate() {
                    let base = (3 * index + round) % references.len();
                    let mut input = references[base].input.clone();
                    input.focus = Some(Focus { word, share });
                    probes.push(input);
                }
            }
        }
        for input in probes {
            run(input, true, &mut references)?;
        }
        let mut coverage = Coverage::new(module, function);
        for reference in &references {
            coverage.add(&reference.execution);
        }
        if let Some((block, runs)) = coverage.unvisited(function) {
            return Err(format!("never completes block bb{} in {runs}", block.index()));
        }
        if let Some((inst, block, outcome, runs)) = coverage.one_sided(function) {
            let mnemonic = function.inst(inst).kind.op_def().mnemonic;
            return Err(format!(
                "never sees its `{mnemonic}` in bb{} come out {outcome} in {runs}",
                block.index()
            ));
        }
        if let Some((inst, block, runs)) = coverage.never_nonzero(function) {
            let mnemonic = function.inst(inst).kind.op_def().mnemonic;
            return Err(format!(
                "never sees its `{mnemonic}` in bb{} come out nonzero in {runs}",
                block.index()
            ));
        }
        let gas = average_gas(references.iter().map(|reference| (reference, reference.gas)));
        let baseline = CostReport { gas, bytes: code_bytes(target, module, function) };
        Ok(Self { target, module, id, seed, constants, palette, world, references, baseline })
    }

    /// The original's cost.
    pub(super) fn baseline(&self) -> CostReport {
        self.baseline
    }

    /// Runs `candidate` in place of the original on every input and returns its cost.
    pub(super) fn check(&self, candidate: &Function) -> Result<CostReport, Rejection> {
        let machine = Machine::with_replacement(self.module, self.id, candidate);
        let mut meter = GasMeter::new(self.target, self.module);
        let mut coverage = Coverage::new(self.module, candidate);
        let mut gas = Vec::with_capacity(self.references.len());
        for reference in &self.references {
            let (execution, spent) = self.run_candidate(&machine, &mut meter, reference);
            self.compare(&reference.input, &reference.execution, &execution)?;
            coverage.add(&execution);
            gas.push(run_gas(self.target, &reference.input, &execution, spent));
        }
        for input in self.constant_inputs(candidate) {
            let (execution, _) = self.run_original(&input, false);
            if matches!(execution.outcome, Outcome::Limit(_) | Outcome::Unsupported(_)) {
                continue;
            }
            let reference = Reference { input, execution, gas: 0, priced: false };
            let (candidate_execution, _) = self.run_candidate(&machine, &mut meter, &reference);
            self.compare(&reference.input, &reference.execution, &candidate_execution)?;
            coverage.add(&candidate_execution);
        }
        if let Some((block, runs)) = coverage.unvisited(candidate) {
            let reason = format!(
                "block {} of the candidate, counting from zero in the order written, never runs \
                 to its end in {runs}",
                block.index()
            );
            return Err(Rejection::new(Stage::Equivalence, reason));
        }
        if let Some((inst, block, outcome, runs)) = coverage.one_sided(candidate) {
            let mnemonic = candidate.inst(inst).kind.op_def().mnemonic;
            let reason = format!(
                "the `{mnemonic}` in block {} of the candidate, counting from zero in the order \
                 written, never comes out {outcome} in {runs}",
                block.index()
            );
            return Err(Rejection::new(Stage::Equivalence, reason));
        }
        if let Some((inst, block, runs)) = coverage.never_nonzero(candidate) {
            let mnemonic = candidate.inst(inst).kind.op_def().mnemonic;
            let reason = format!(
                "the `{mnemonic}` in block {} of the candidate, counting from zero in the order \
                 written, never comes out nonzero in {runs}",
                block.index()
            );
            return Err(Rejection::new(Stage::Equivalence, reason));
        }
        let gas = average_gas(self.references.iter().zip(gas));
        Ok(CostReport { gas, bytes: code_bytes(self.target, self.module, candidate) })
    }

    fn run_candidate(
        &self,
        machine: &Machine<'_>,
        meter: &mut GasMeter<'_>,
        reference: &Reference,
    ) -> (Execution, u64) {
        // A candidate may take a few times the original's steps before it counts as diverging.
        let fuel = reference.execution.fuel.saturating_mul(4).saturating_add(1_000);
        let limits = Limits { fuel, depth: DEPTH };
        let input = &reference.input;
        let mut world = World::new(input, &self.palette);
        let memory = input.memory.clone();
        let execution = machine.run(self.id, &input.args, memory, Some(&mut world), limits, meter);
        (execution, meter.take())
    }

    /// Runs the original on `input`, and returns what it reads from its world when `record` is
    /// set.
    fn run_original(&self, input: &Input, record: bool) -> (Execution, Vec<String>) {
        let limits = Limits { fuel: ORIGINAL_FUEL, depth: DEPTH };
        let mut world = World::new(input, &self.palette);
        world.reads = record.then(Vec::new);
        let memory = input.memory.clone();
        let machine = Machine::new(self.module);
        let execution =
            machine.run(self.id, &input.args, memory, Some(&mut world), limits, &mut ());
        (execution, world.reads.unwrap_or_default())
    }

    /// Compares a candidate's run with the original's on `input`.
    fn compare(
        &self,
        input: &Input,
        original: &Execution,
        candidate: &Execution,
    ) -> Result<(), Rejection> {
        let fail = |reason: String| Rejection {
            stage: Stage::Equivalence,
            reason,
            counterexample: Some(self.describe(input)),
        };
        if original.outcome != candidate.outcome {
            return Err(fail(format!(
                "the original {}, but the candidate {}",
                describe_outcome(&original.outcome),
                describe_outcome(&candidate.outcome)
            )));
        }
        if let Some(address) = candidate.memory.first_write_outside(&original.memory) {
            return Err(fail(format!(
                "the candidate writes memory byte {address:#x}, which the original never writes"
            )));
        }
        if matches!(original.outcome, Outcome::Return(_))
            && let Some(address) = original.memory.first_difference(&candidate.memory)
        {
            let word = address - address % EvmMemoryLayout::WORD_SIZE;
            return Err(fail(format!(
                "the word at {word:#x} ends as {:#x} in the original but {:#x} in the candidate",
                original.memory.word(word),
                candidate.memory.word(word)
            )));
        }
        if matches!(original.outcome, Outcome::Return(_) | Outcome::ReturnData(_) | Outcome::Stop) {
            let world = World::new(input, &self.palette);
            let (original, candidate) = (&original.effects, &candidate.effects);
            compare_slots("storage", &original.storage, &candidate.storage, |slot| {
                world.storage_word(slot)
            })
            .and_then(|()| {
                compare_slots(
                    "transient storage",
                    &original.transient,
                    &candidate.transient,
                    |slot| world.transient_word(slot),
                )
            })
            .and_then(|()| compare_logs(&original.logs, &candidate.logs))
            .map_err(fail)?;
        }
        Ok(())
    }

    /// Describes `input`: its arguments, its free memory pointer, and the storage and context
    /// values the original reads on it.
    fn describe(&self, input: &Input) -> String {
        let mut text = String::new();
        for (index, arg) in input.args.iter().enumerate() {
            let _ = write!(text, "arg{index} = {arg:#x}, ");
        }
        let _ = write!(text, "free memory pointer {:#x}", input.free_memory_pointer);
        let (_, mut reads) = self.run_original(input, true);
        let mut seen = FxHashSet::default();
        reads.retain(|read| seen.insert(read.clone()));
        let omitted = reads.len().saturating_sub(MAX_REPORTED_READS);
        for read in reads.iter().take(MAX_REPORTED_READS) {
            let _ = write!(text, ", {read}");
        }
        if omitted > 0 {
            let _ = write!(text, ", and {omitted} more reads");
        }
        text
    }

    /// Builds inputs that place each constant the candidate adds, and its neighbors and
    /// left-aligned form, in each word argument of an existing input and, when either function
    /// reads storage or its context, in half of the world's answers.
    fn constant_inputs(&self, candidate: &Function) -> Vec<Input> {
        let mut added = constants(candidate)
            .into_iter()
            .filter(|constant| !self.constants.contains(constant))
            .collect::<Vec<_>>();
        added.sort_unstable();
        let params = &self.module.function(self.id).params;
        let world = self.world || uses_world(candidate);
        let mut inputs = Vec::new();
        for (index, &constant) in added.iter().enumerate() {
            let base = mix64(self.seed ^ index as u64) as usize % self.references.len();
            for (param, &ty) in params.iter().enumerate() {
                if inputs.len() == MAX_CONSTANT_INPUTS {
                    return inputs;
                }
                let mut input = self.references[base].input.clone();
                input.args[param] = mask(constant, ty);
                inputs.push(input);
            }
            for share in [1, 2] {
                if world && inputs.len() < MAX_CONSTANT_INPUTS {
                    let mut input = self.references[base].input.clone();
                    input.focus = Some(Focus { word: constant, share });
                    inputs.push(input);
                }
            }
        }
        inputs
    }
}

/// What the runs of one function exercised.
///
/// A revert discards the memory and storage writes and the logs of its run, so in a block from
/// which the function can end without reverting, only runs that do not revert count: for running
/// the block to its end, for its values coming out nonzero, and for its decisions coming out each
/// way. The exception is a decision's outcome that branches into blocks that always revert, whose
/// consequence is the revert itself; it counts in every run, as everything in such blocks does.
struct Coverage {
    /// The reachable blocks from which a run can end without reverting.
    succeeding: DenseBitSet<BlockId>,
    /// The blocks all runs completed, and those runs that did not revert completed.
    visited: DenseBitSet<BlockId>,
    succeeded_visited: DenseBitSet<BlockId>,
    /// Whether each result was ever zero (bit 0) and ever nonzero (bit 1), in every run, and in
    /// runs that did not revert.
    outcomes: IndexVec<InstId, u8>,
    succeeded_outcomes: IndexVec<InstId, u8>,
}

impl Coverage {
    fn new(module: &Module, function: &Function) -> Self {
        let blocks = function.blocks.len();
        Self {
            succeeding: succeeding_blocks(module, function),
            visited: DenseBitSet::new_empty(blocks),
            succeeded_visited: DenseBitSet::new_empty(blocks),
            outcomes: index_vec![0; function.num_insts()],
            succeeded_outcomes: index_vec![0; function.num_insts()],
        }
    }

    fn add(&mut self, execution: &Execution) {
        self.visited.union(&execution.visited);
        merge_outcomes(&mut self.outcomes, &execution.outcomes);
        if matches!(execution.outcome, Outcome::Return(_) | Outcome::ReturnData(_) | Outcome::Stop)
        {
            self.succeeded_visited.union(&execution.visited);
            merge_outcomes(&mut self.succeeded_outcomes, &execution.outcomes);
        }
    }

    /// Returns the runs that count for `block`, as a report names them.
    fn runs(&self, block: BlockId) -> &'static str {
        if self.succeeding.contains(block) { "a test that does not revert" } else { "tests" }
    }

    /// Returns the first reachable block of `function`, in order, that no counting run completed,
    /// with the runs that count.
    fn unvisited(&self, function: &Function) -> Option<(BlockId, &'static str)> {
        let cfg = CfgInfo::new(function);
        let visited = |block| {
            let visited = if self.succeeding.contains(block) {
                &self.succeeded_visited
            } else {
                &self.visited
            };
            visited.contains(block)
        };
        let block = cfg.rpo().iter().copied().filter(|&block| !visited(block)).min()?;
        Some((block, self.runs(block)))
    }

    /// Returns the zero and nonzero outcomes of `inst` in `block` that counting runs saw.
    fn seen(&self, block: BlockId, inst: InstId) -> u8 {
        let outcomes =
            if self.succeeding.contains(block) { &self.succeeded_outcomes } else { &self.outcomes };
        outcomes.get(inst).copied().unwrap_or_default()
    }

    /// Returns the first decision of `function` that never came out one way in a counting run,
    /// with its block, the outcome it never had, and the runs that count.
    fn one_sided(&self, function: &Function) -> Option<(InstId, BlockId, bool, &'static str)> {
        decisions(function).into_iter().find_map(|(inst, block)| {
            let mut seen = self.seen(block, inst);
            // An outcome whose branch always reverts counts in every run.
            if let Some(Terminator::Branch { condition, then_block, else_block }) =
                &function.blocks[block].terminator
                && function.inst(inst).result() == Some(*condition)
            {
                let all = self.outcomes.get(inst).copied().unwrap_or_default();
                if !self.succeeding.contains(*then_block) {
                    seen |= all & 2;
                }
                if !self.succeeding.contains(*else_block) {
                    seen |= all & 1;
                }
            }
            let runs = self.runs(block);
            if seen & 2 == 0 {
                Some((inst, block, true, runs))
            } else if seen & 1 == 0 {
                Some((inst, block, false, runs))
            } else {
                None
            }
        })
    }

    /// Returns the first value of the reachable blocks of `function`, in order, other than its
    /// decisions, that never came out nonzero in a counting run, with its block and the runs that
    /// count.
    fn never_nonzero(&self, function: &Function) -> Option<(InstId, BlockId, &'static str)> {
        let decisions =
            decisions(function).into_iter().map(|(inst, _)| inst).collect::<FxHashSet<_>>();
        let cfg = CfgInfo::new(function);
        cfg.rpo().iter().find_map(|&block| {
            function.blocks[block].instructions.iter().find_map(|&inst| {
                let value = function.inst(inst).result().is_some() && !decisions.contains(&inst);
                (value && self.seen(block, inst) & 2 == 0).then(|| (inst, block, self.runs(block)))
            })
        })
    }
}

/// Returns the blocks of `function` from which a run can end without reverting: by returning, by
/// ending the call with `returndata` or `stop`, or by tail calling a function that can.
fn succeeding_blocks(module: &Module, function: &Function) -> DenseBitSet<BlockId> {
    let cfg = CfgInfo::new(function);
    let mut succeeding = DenseBitSet::new_empty(function.blocks.len());
    for (block, body) in function.blocks.iter_enumerated() {
        if ends_successfully(module, body.terminator.as_ref(), &mut FxHashSet::default()) {
            succeeding.insert(block);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in function.blocks.indices() {
            if !succeeding.contains(block)
                && cfg.successors(block).iter().any(|&successor| succeeding.contains(successor))
            {
                changed |= succeeding.insert(block);
            }
        }
    }
    succeeding
}

/// Returns whether `terminator` can end a call without reverting, following tail calls into the
/// functions `visiting` does not hold yet.
fn ends_successfully(
    module: &Module,
    terminator: Option<&Terminator>,
    visiting: &mut FxHashSet<FunctionId>,
) -> bool {
    match terminator {
        Some(Terminator::Return { .. } | Terminator::ReturnData { .. } | Terminator::Stop) => true,
        Some(&Terminator::TailCall { function, .. }) => {
            visiting.insert(function)
                && module
                    .function(function)
                    .blocks
                    .iter()
                    .any(|block| ends_successfully(module, block.terminator.as_ref(), visiting))
        }
        _ => false,
    }
}

/// Adds the outcomes of one run to those of earlier runs.
fn merge_outcomes(outcomes: &mut IndexVec<InstId, u8>, run: &IndexVec<InstId, u8>) {
    for (inst, &seen) in run.iter_enumerated() {
        outcomes[inst] |= seen;
    }
}

/// Checks that a candidate writes only the storage slots of one kind the original writes, and
/// that every slot the original writes ends with the same value, where `before` returns what a
/// slot held before the run.
fn compare_slots(
    kind: &str,
    original: &FxHashMap<U256, U256>,
    candidate: &FxHashMap<U256, U256>,
    before: impl Fn(U256) -> U256,
) -> Result<(), String> {
    let outside = candidate.keys().filter(|slot| !original.contains_key(slot)).min();
    if let Some(slot) = outside {
        return Err(format!(
            "the candidate writes {kind} slot {slot:#x}, which the original never writes"
        ));
    }
    let mut slots = original.keys().copied().collect::<Vec<_>>();
    slots.sort_unstable();
    for slot in slots {
        let ends = original[&slot];
        let candidate_ends = candidate.get(&slot).copied().unwrap_or_else(|| before(slot));
        if ends != candidate_ends {
            return Err(format!(
                "{kind} slot {slot:#x}, which held {:#x}, ends as {ends:#x} in the original but \
                 {candidate_ends:#x} in the candidate",
                before(slot)
            ));
        }
    }
    Ok(())
}

/// Checks that a candidate logs the events the original logs, in the same order.
fn compare_logs(original: &[Log], candidate: &[Log]) -> Result<(), String> {
    for (index, (log, candidate_log)) in original.iter().zip(candidate).enumerate() {
        if log != candidate_log {
            return Err(format!(
                "event {index}, counting from zero, differs: the original logs {}, but the \
                 candidate {}",
                describe_log(log),
                describe_log(candidate_log)
            ));
        }
    }
    if original.len() != candidate.len() {
        let events = |count: usize| format!("{count} event{}", if count == 1 { "" } else { "s" });
        return Err(format!(
            "the original logs {}, but the candidate {}",
            events(original.len()),
            events(candidate.len())
        ));
    }
    Ok(())
}

/// Returns the decisions in the reachable blocks of `function`, in order: its comparisons, and the
/// `and`, `or`, and `xor` of boolean words, which are `i1` values, their zero extensions, zero and
/// one, and earlier such decisions.
fn decisions(function: &Function) -> Vec<(InstId, BlockId)> {
    let cfg = CfgInfo::new(function);
    let mut booleans = DenseBitSet::new_empty(function.num_values());
    let mut decisions = Vec::new();
    for &block in cfg.rpo() {
        for &inst in &function.blocks[block].instructions {
            let instruction = function.inst(inst);
            let Some(result) = instruction.result() else { continue };
            let boolean = |value: ValueId| match function.value(value) {
                Value::Immediate(immediate) => {
                    immediate.as_u256().is_some_and(|word| word <= U256::ONE)
                }
                _ => function.value_ty(value) == Some(MirType::I1) || booleans.contains(value),
            };
            let decision = match instruction.kind {
                InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
                | InstKind::Eq(..)
                | InstKind::Ne(..) => true,
                InstKind::And(a, b) | InstKind::Or(a, b) | InstKind::Xor(a, b) => {
                    boolean(a) && boolean(b)
                }
                _ => false,
            };
            let widened = matches!(instruction.kind, InstKind::Zext(value) if boolean(value));
            if decision || widened {
                booleans.insert(result);
            }
            if decision {
                decisions.push((inst, block));
            }
        }
    }
    decisions
}

/// Averages the gas of the priced runs on inputs the original returns on, or of every priced run
/// when it returns on none.
fn average_gas<'r>(runs: impl Iterator<Item = (&'r Reference, u64)>) -> u64 {
    let (mut total, mut count, mut returning_total, mut returning) = (0u128, 0u128, 0u128, 0u128);
    for (reference, gas) in runs.filter(|(reference, _)| reference.priced) {
        total += u128::from(gas);
        count += 1;
        if matches!(reference.execution.outcome, Outcome::Return(_)) {
            returning_total += u128::from(gas);
            returning += 1;
        }
    }
    let (total, count) = if returning == 0 { (total, count) } else { (returning_total, returning) };
    u64::try_from(total / count.max(1)).unwrap_or(u64::MAX)
}

/// Adds the memory a run grew into to the gas its operations spent.
fn run_gas(target: Target, input: &Input, execution: &Execution, operations: u64) -> u64 {
    let expansion = target.memory_expansion_gas(input.memory.size(), execution.memory.size());
    operations.saturating_add(u64::try_from(expansion).unwrap_or(u64::MAX))
}

/// Returns the constants of function `id` and every function it can call.
fn call_tree_constants(module: &Module, id: FunctionId) -> FxHashSet<U256> {
    let mut constants = constants(module.function(id));
    let graph = CallGraphInfo::new(module);
    for callee in graph.reachable_callees_from([id]).iter() {
        constants.extend(self::constants(module.function(callee)));
    }
    constants
}

/// Returns whether function `id` or a function it can call reads or writes storage or its
/// context, which world probes exercise.
fn reads_world(module: &Module, id: FunctionId) -> bool {
    let graph = CallGraphInfo::new(module);
    uses_world(module.function(id))
        || graph
            .reachable_callees_from([id])
            .iter()
            .any(|callee| uses_world(module.function(callee)))
}

/// Returns whether `function` reads or writes storage or its context.
fn uses_world(function: &Function) -> bool {
    function.instructions().any(|inst| {
        let kind = &function.inst(inst).kind;
        !interp::supports(kind) && runs(kind)
    })
}

/// Returns the constants of `function`, with their neighbors and left-aligned forms.
fn constants(function: &Function) -> FxHashSet<U256> {
    let mut constants = FxHashSet::default();
    for value in function.live_values() {
        if let Value::Immediate(immediate) = function.value(value)
            && let Some(word) = immediate.as_u256()
        {
            constants.extend([word, word.wrapping_add(U256::ONE), word.wrapping_sub(U256::ONE)]);
            let bytes = word.byte_len();
            if (1..32).contains(&bytes) {
                constants.insert(word << (8 * (32 - bytes)));
            }
        }
    }
    constants
}

/// Generates one input for `function`, whose memory garbage draws words from `palette`.
fn generate(
    rng: &mut Rng,
    function: &Function,
    constants: &[U256],
    palette: &Arc<[U256]>,
) -> Input {
    let free_memory_pointer = U256::from(match rng.below(8) {
        0..=4 => EvmMemoryLayout::HEAP_START + 32 * rng.below(16),
        5 | 6 => EvmMemoryLayout::HEAP_START + 32 * rng.below(4096),
        _ => EvmMemoryLayout::HEAP_START + rng.below(512),
    });
    let fmp = free_memory_pointer.to::<u64>();
    let mut memory = Memory::new(rng.next(), fmp.div_ceil(EvmMemoryLayout::WORD_SIZE))
        .with_palette(Arc::clone(palette));
    memory.set(EvmMemoryLayout::FMP_SLOT, free_memory_pointer);
    memory.set(EvmMemoryLayout::ZERO_SLOT, U256::ZERO);
    let constant = |rng: &mut Rng| {
        (!constants.is_empty()).then(|| constants[rng.below(constants.len() as u64) as usize])
    };
    let mut args = SmallVec::<[U256; 4]>::new();
    for &ty in &function.params {
        let value = match ty {
            MirType::MemPtr => match rng.below(8) {
                0 if !args.is_empty() => args[rng.below(args.len() as u64) as usize],
                1 => U256::from(EvmMemoryLayout::ZERO_SLOT),
                _ => U256::from(fmp + 32 * rng.below(8)),
            },
            _ => match rng.below(16) {
                // Zero is the null address, the empty amount, and false, which code checks often.
                0 => U256::ZERO,
                1..=4 => boundary(rng),
                5..=8 if let Some(constant) = constant(rng) => constant,
                9 | 10 => U256::from(rng.below(65)),
                11 | 12 => U256::from(fmp + 32 * rng.below(8)),
                13 | 14 if !args.is_empty() => {
                    let previous = args[rng.below(args.len() as u64) as usize];
                    if rng.below(2) == 0 { previous } else { previous + U256::from(32) }
                }
                _ => rng.word(),
            },
        };
        let value = mask(value, ty);
        // A pointer argument usually heads an object: a length or count, kept small so that loops
        // end, then fields that are often flags or constants.
        let words = OBJECT_WORDS * EvmMemoryLayout::WORD_SIZE;
        if value >= U256::from(EvmMemoryLayout::HEAP_START)
            && value < U256::from(MEMORY_LIMIT - words)
            && rng.below(2) == 0
        {
            let head = value.to::<u64>();
            let length = match rng.below(2) {
                0 => constant(rng).filter(|&constant| constant <= U256::from(64)),
                _ => None,
            };
            memory.set(head, length.unwrap_or_else(|| U256::from(rng.below(33))));
            for word in 1..=rng.below(OBJECT_WORDS) {
                let field = match rng.below(4) {
                    0 => Some(U256::from(rng.below(3))),
                    1 | 2 => constant(rng),
                    _ => None,
                };
                if let Some(field) = field {
                    memory.set(head + word * EvmMemoryLayout::WORD_SIZE, field);
                }
            }
        }
        args.push(value);
    }
    Input { args, memory, free_memory_pointer, world: rng.next(), focus: None }
}

/// Returns a boundary value: a power of two, a neighbor, or a negation of either, which masking
/// to a type's width turns into that width's boundaries.
fn boundary(rng: &mut Rng) -> U256 {
    let power = U256::ONE << rng.below(256) as usize;
    let near = match rng.below(3) {
        0 => power.wrapping_sub(U256::ONE),
        1 => power,
        _ => power.wrapping_add(U256::ONE),
    };
    if rng.below(2) == 0 { near } else { near.wrapping_neg() }
}

/// Keeps the bits values of `ty` may hold.
fn mask(value: U256, ty: MirType) -> U256 {
    match ty {
        MirType::Int(bits) if bits.get() < 256 => value & (U256::MAX >> (256 - bits.get())),
        _ => value,
    }
}

fn describe_outcome(outcome: &Outcome) -> String {
    let bytes = |bytes: &[u8]| {
        if bytes.is_empty() {
            return "no data".to_string();
        }
        let shown = &bytes[..bytes.len().min(MAX_REPORTED_BYTES)];
        let ellipsis = if shown.len() < bytes.len() { "…" } else { "" };
        format!("0x{}{ellipsis}", hex::encode(shown))
    };
    match outcome {
        Outcome::Return(values) if values.is_empty() => "returns".into(),
        Outcome::Return(values) => {
            let values = values.iter().map(|value| format!("{value:#x}")).collect::<Vec<_>>();
            format!("returns {}", values.join(", "))
        }
        Outcome::Revert(payload) => format!("reverts with {}", bytes(payload)),
        Outcome::ReturnData(payload) => format!("ends the call returning {}", bytes(payload)),
        Outcome::Stop => "stops".into(),
        Outcome::Invalid => "reaches `invalid`".into(),
        Outcome::Limit(Limit::Fuel) => "runs far longer".into(),
        Outcome::Limit(Limit::Depth) => "nests calls too deeply".into(),
        Outcome::Limit(Limit::Memory) => "reaches past 16 MiB of memory".into(),
        Outcome::Unsupported(what) => format!("reaches unsupported `{what}`"),
    }
}

fn describe_log(log: &Log) -> String {
    let topics = log.topics.iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>();
    let shown = &log.data[..log.data.len().min(MAX_REPORTED_BYTES)];
    let ellipsis = if shown.len() < log.data.len() { "…" } else { "" };
    format!("topics [{}] and data 0x{}{ellipsis}", topics.join(", "), hex::encode(shown))
}

/// The storage and context one input runs with, derived from its seed.
struct World<'t> {
    seed: u64,
    palette: &'t [U256],
    args: &'t [U256],
    focus: Option<Focus>,
    /// What the run read, when recorded for a report.
    reads: Option<Vec<String>>,
}

impl<'t> World<'t> {
    /// Salts the digests of persistent and transient storage slots apart from context reads,
    /// which the opcode salts.
    const STORAGE_SALT: u64 = 0x100;
    const TRANSIENT_SALT: u64 = 0x101;
    /// Salts the choice of the worlds whose context reads answer with whole words.
    const WIDTH_SALT: u64 = 0x102;

    fn new(input: &'t Input, palette: &'t [U256]) -> Self {
        let (seed, args, focus) = (input.world, &input.args[..], input.focus);
        Self { seed, palette, args, focus, reads: None }
    }

    /// Returns what persistent storage slot `slot` holds before the run.
    fn storage_word(&self, slot: U256) -> U256 {
        self.draw(&self.digest(Self::STORAGE_SALT, &[slot]), true)
    }

    /// Returns what transient storage slot `slot` holds before the run: zero half the time, as
    /// every transaction starts it empty.
    fn transient_word(&self, slot: U256) -> U256 {
        let digest = self.digest(Self::TRANSIENT_SALT, &[slot]);
        if lane(&digest, 2).is_multiple_of(2) { U256::ZERO } else { self.draw(&digest, true) }
    }

    /// Returns what context read `opcode` returns for `operands`. Addresses have 160 bits, as the
    /// EVM returns them. Block numbers, times, limits, and prices fit 64 bits on chain, and amounts
    /// of ether 128, so most worlds cut them to that width, where checked arithmetic on them
    /// succeeds. The EVM does not bound them, though, and the compiler relies on no such width,
    /// so a quarter of the worlds, and every focused read, answer them with whole words.
    fn context_word(&self, opcode: u8, operands: &[U256]) -> U256 {
        let digest = self.digest(u64::from(opcode), operands);
        // Storage may hold the caller, which context values do not draw, keeping draws finite.
        let (word, focused) = match self.focused(&digest) {
            Some(word) => (word, true),
            None => (self.drawn(&digest, false), false),
        };
        let bits = match opcode {
            op::ADDRESS | op::ORIGIN | op::CALLER | op::COINBASE => 160,
            _ if focused || mix64(self.seed ^ Self::WIDTH_SALT).is_multiple_of(4) => 256,
            op::TIMESTAMP
            | op::NUMBER
            | op::GASLIMIT
            | op::CHAINID
            | op::GASPRICE
            | op::BASEFEE
            | op::BLOBBASEFEE
            | op::SLOTNUM => 64,
            op::CALLVALUE | op::BALANCE | op::SELFBALANCE => 128,
            _ => 256,
        };
        word & (U256::MAX >> (256 - bits))
    }

    /// Hashes the world's seed, `salt`, and every bit of `words`, so that reads of different
    /// slots or operands draw independently. A shorter key would let two slots share every
    /// draw, and a candidate read one in place of the other unseen.
    fn digest(&self, salt: u64, words: &[U256]) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(self.seed.to_be_bytes());
        hasher.update(salt.to_be_bytes());
        for word in words {
            hasher.update(word.to_be_bytes::<32>());
        }
        hasher.finalize()
    }

    /// Returns the word for a read hashed to `digest`: the input's focused word, or a drawn one.
    fn draw(&self, digest: &B256, caller: bool) -> U256 {
        self.focused(digest).unwrap_or_else(|| self.drawn(digest, caller))
    }

    /// Returns the input's focused word when it answers the read hashed to `digest`.
    fn focused(&self, digest: &B256) -> Option<U256> {
        let Focus { word, share } = self.focus?;
        lane(digest, 1).is_multiple_of(share).then_some(word)
    }

    /// Returns the word drawn for a read hashed to `digest`: zero, a small number, a constant, an
    /// argument, the caller when `caller` is set, or a random word.
    fn drawn(&self, digest: &B256, caller: bool) -> U256 {
        let pick = lane(digest, 0);
        let rest = pick >> 4;
        let choose = |words: &[U256]| words[(rest % words.len() as u64) as usize];
        match pick % 16 {
            0..=3 => U256::ZERO,
            4 | 5 => U256::from(1 + rest % 64),
            6..=8 if !self.palette.is_empty() => choose(self.palette),
            9 | 10 if !self.args.is_empty() => choose(self.args),
            11 if caller => self.context_word(op::CALLER, &[]),
            _ => U256::from_be_bytes(keccak256(digest).0),
        }
    }

    fn record(&mut self, read: impl FnOnce() -> String) {
        if let Some(reads) = &mut self.reads {
            reads.push(read());
        }
    }
}

impl Host for World<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        if !CONTEXT.contains(&opcode) {
            return None;
        }
        let word = self.context_word(opcode, operands);
        self.record(|| {
            let mnemonic = op::mnemonic(opcode).unwrap_or("opcode");
            let operands = operands.iter().map(|operand| format!(" {operand:#x}"));
            format!("`{mnemonic}{}` = {word:#x}", operands.collect::<String>())
        });
        Some(word)
    }

    fn storage(&mut self, slot: U256) -> U256 {
        let word = self.storage_word(slot);
        self.record(|| format!("storage slot {slot:#x} = {word:#x}"));
        word
    }

    fn transient(&mut self, slot: U256) -> U256 {
        let word = self.transient_word(slot);
        self.record(|| format!("transient storage slot {slot:#x} = {word:#x}"));
        word
    }
}

/// Returns the `index`th 64-bit lane of `digest`.
fn lane(digest: &B256, index: usize) -> u64 {
    u64::from_be_bytes(digest[8 * index..8 * index + 8].try_into().unwrap())
}

/// A deterministic generator of test inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        mix64(self.0)
    }

    /// Returns a number below `bound`, which must be positive.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn word(&mut self) -> U256 {
        U256::from_limbs([self.next(), self.next(), self.next(), self.next()])
    }
}
