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
//! - Every reachable block of the candidate must run on some input, and so must every reachable
//!   block of the original, or the function is not tested at all.
//!
//! # Inputs
//!
//! Arguments mix the boundaries of their width, constants from the original and their neighbors,
//! small numbers, words near the free memory pointer, repeats of earlier arguments for aliasing,
//! and random words, each masked to its type so that it holds the clean bits the type promises;
//! `memptr` arguments point near the free memory pointer. Memory holds seeded garbage except
//! for the zero word at `0x60`, the free memory pointer at `0x40`, which varies from `0x80` up
//! and is sometimes unaligned, and small words at pointer arguments, which keep length-driven
//! loops short. Memory starts as large as the free memory pointer.
//!
//! Testing is not proof: a difference on an input no generator reaches goes unnoticed.

use super::cost::{GasMeter, code_bytes};
use crate::{
    llm::{CostReport, Stage},
    mir::{
        BlockId, Function, FunctionId, MirType, Module, Value,
        analysis::CfgInfo,
        memory::EvmMemoryLayout,
        utils::interp::{Execution, Limit, Limits, MEMORY_LIMIT, Machine, Memory, Outcome, mix64},
    },
    target::Target,
};
use alloy_primitives::{U256, hex};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashSet};
use std::fmt::Write;

/// Runs of the original that must finish before its tests count.
const MIN_FINISHED_RUNS: usize = 16;
/// Fuel for one run of the original: loops of a few thousand iterations finish.
const ORIGINAL_FUEL: u64 = 20_000;
/// Call depth of one run.
const DEPTH: usize = 64;
/// The most inputs added at the constants a candidate introduces.
const MAX_CONSTANT_INPUTS: usize = 64;
/// The longest payload a report spells out, in bytes.
const MAX_REPORTED_BYTES: usize = 68;

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
}

/// A finished run of the original.
struct Reference {
    input: Input,
    execution: Execution,
    gas: u64,
}

/// Generated inputs and the original's runs on them.
pub(super) struct Tests<'a> {
    target: Target,
    module: &'a Module,
    id: FunctionId,
    seed: u64,
    constants: FxHashSet<U256>,
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
        let constants = constants(function);
        let mut rng = Rng(seed);
        let machine = Machine::new(module);
        let mut meter = GasMeter::new(target, module);
        let mut references = Vec::new();
        for _ in 0..count {
            let input = generate(&mut rng, function, &constants);
            let limits = Limits { fuel: ORIGINAL_FUEL, depth: DEPTH };
            let execution = machine.run(id, &input.args, input.memory.clone(), limits, &mut meter);
            let spent = meter.take();
            match execution.outcome {
                Outcome::Unsupported(what) => return Err(format!("reaches unsupported `{what}`")),
                Outcome::Limit(_) => {}
                _ => {
                    let gas = run_gas(target, &input, &execution, spent);
                    references.push(Reference { input, execution, gas });
                }
            }
        }
        if references.len() < MIN_FINISHED_RUNS {
            return Err(format!("finishes only {} of {count} test runs", references.len()));
        }
        let mut visited = DenseBitSet::new_empty(function.blocks.len());
        for reference in &references {
            visited.union(&reference.execution.visited);
        }
        if let Some(block) = unvisited(function, &visited) {
            return Err(format!("never runs block bb{block} in tests"));
        }
        let gas = average_gas(references.iter().map(|reference| (reference, reference.gas)));
        let baseline = CostReport { gas, bytes: code_bytes(target, module, function) };
        Ok(Self { target, module, id, seed, constants, references, baseline })
    }

    /// The original's cost.
    pub(super) fn baseline(&self) -> CostReport {
        self.baseline
    }

    /// Runs `candidate` in place of the original on every input and returns its cost.
    pub(super) fn check(&self, candidate: &Function) -> Result<CostReport, Rejection> {
        let machine = Machine::with_replacement(self.module, self.id, candidate);
        let mut meter = GasMeter::new(self.target, self.module);
        let mut visited = DenseBitSet::new_empty(candidate.blocks.len());
        let mut gas = Vec::with_capacity(self.references.len());
        for reference in &self.references {
            let (execution, spent) = self.run_candidate(&machine, &mut meter, reference);
            compare(&reference.input, &reference.execution, &execution)?;
            visited.union(&execution.visited);
            gas.push(run_gas(self.target, &reference.input, &execution, spent));
        }
        for input in self.constant_inputs(candidate) {
            let original = Machine::new(self.module);
            let limits = Limits { fuel: ORIGINAL_FUEL, depth: DEPTH };
            let execution =
                original.run(self.id, &input.args, input.memory.clone(), limits, &mut ());
            if matches!(execution.outcome, Outcome::Limit(_) | Outcome::Unsupported(_)) {
                continue;
            }
            let reference = Reference { input, execution, gas: 0 };
            let (candidate_execution, _) = self.run_candidate(&machine, &mut meter, &reference);
            compare(&reference.input, &reference.execution, &candidate_execution)?;
            visited.union(&candidate_execution.visited);
        }
        if let Some(block) = unvisited(candidate, &visited) {
            let reason = format!(
                "block {block} of the candidate, counting from zero in the order written, never \
                 runs in tests"
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
        let execution = machine.run(self.id, &input.args, input.memory.clone(), limits, meter);
        (execution, meter.take())
    }

    /// Builds inputs that place each constant the candidate adds, and its neighbors, in each word
    /// argument of an existing input.
    fn constant_inputs(&self, candidate: &Function) -> Vec<Input> {
        let mut added = constants(candidate)
            .into_iter()
            .filter(|constant| !self.constants.contains(constant))
            .collect::<Vec<_>>();
        added.sort_unstable();
        let params = &self.module.function(self.id).params;
        let mut inputs = Vec::new();
        for (index, &constant) in added.iter().enumerate() {
            for (param, &ty) in params.iter().enumerate() {
                if inputs.len() == MAX_CONSTANT_INPUTS {
                    return inputs;
                }
                let base = mix64(self.seed ^ index as u64) as usize % self.references.len();
                let mut input = self.references[base].input.clone();
                input.args[param] = mask(constant, ty);
                inputs.push(input);
            }
        }
        inputs
    }
}

/// Compares a candidate's run with the original's on `input`.
fn compare(input: &Input, original: &Execution, candidate: &Execution) -> Result<(), Rejection> {
    let fail = |reason: String| Rejection {
        stage: Stage::Equivalence,
        reason,
        counterexample: Some(describe_input(input)),
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
    Ok(())
}

/// Returns the first reachable block of `function` that `visited` lacks.
fn unvisited(function: &Function, visited: &DenseBitSet<BlockId>) -> Option<usize> {
    let cfg = CfgInfo::new(function);
    cfg.rpo().iter().filter(|&&block| !visited.contains(block)).map(|block| block.index()).min()
}

/// Averages the gas of the runs on inputs the original returns on, or of every run when it
/// returns on none.
fn average_gas<'r>(runs: impl Iterator<Item = (&'r Reference, u64)>) -> u64 {
    let (mut total, mut count, mut returning_total, mut returning) = (0u128, 0u128, 0u128, 0u128);
    for (reference, gas) in runs {
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

/// Returns the constants of `function`, with their neighbors.
fn constants(function: &Function) -> FxHashSet<U256> {
    let mut constants = FxHashSet::default();
    for value in function.live_values() {
        if let Value::Immediate(immediate) = function.value(value)
            && let Some(word) = immediate.as_u256()
        {
            constants.extend([word, word.wrapping_add(U256::ONE), word.wrapping_sub(U256::ONE)]);
        }
    }
    constants
}

/// Generates one input for `function`.
fn generate(rng: &mut Rng, function: &Function, constants: &FxHashSet<U256>) -> Input {
    let free_memory_pointer = U256::from(match rng.below(8) {
        0..=4 => EvmMemoryLayout::HEAP_START + 32 * rng.below(16),
        5 | 6 => EvmMemoryLayout::HEAP_START + 32 * rng.below(4096),
        _ => EvmMemoryLayout::HEAP_START + rng.below(512),
    });
    let fmp = free_memory_pointer.to::<u64>();
    let mut memory = Memory::new(rng.next(), fmp.div_ceil(EvmMemoryLayout::WORD_SIZE));
    memory.set(EvmMemoryLayout::FMP_SLOT, free_memory_pointer);
    memory.set(EvmMemoryLayout::ZERO_SLOT, U256::ZERO);
    let mut constants = constants.iter().copied().collect::<Vec<_>>();
    constants.sort_unstable();
    let mut args = SmallVec::<[U256; 4]>::new();
    for &ty in &function.params {
        let value = match ty {
            MirType::MemPtr => match rng.below(8) {
                0 if !args.is_empty() => args[rng.below(args.len() as u64) as usize],
                1 => U256::from(EvmMemoryLayout::ZERO_SLOT),
                _ => U256::from(fmp + 32 * rng.below(8)),
            },
            _ => match rng.below(8) {
                0 | 1 => boundary(rng, ty),
                2 | 3 if !constants.is_empty() => {
                    constants[rng.below(constants.len() as u64) as usize]
                }
                4 => U256::from(rng.below(65)),
                5 => U256::from(fmp + 32 * rng.below(8)),
                6 if !args.is_empty() => {
                    let previous = args[rng.below(args.len() as u64) as usize];
                    if rng.below(2) == 0 { previous } else { previous + U256::from(32) }
                }
                _ => rng.word(),
            },
        };
        let value = mask(value, ty);
        // A pointer argument usually heads a length or count; keep it small so loops end.
        if value >= U256::from(EvmMemoryLayout::HEAP_START)
            && value < U256::from(MEMORY_LIMIT - EvmMemoryLayout::WORD_SIZE)
            && rng.below(2) == 0
        {
            memory.set(value.to::<u64>(), U256::from(rng.below(33)));
        }
        args.push(value);
    }
    Input { args, memory, free_memory_pointer }
}

/// Returns a boundary value of `ty`'s width.
fn boundary(rng: &mut Rng, ty: MirType) -> U256 {
    let bits = match ty {
        MirType::Int(bits) => bits.get().min(256) as usize,
        _ => 256,
    };
    let mut values = vec![U256::ZERO, U256::ONE, U256::from(2)];
    for width in [8, 16, 32, 64, 128, 160, bits - 1, bits] {
        if width == 0 || width > bits || width > 255 {
            continue;
        }
        let power = U256::ONE << width;
        values.extend([power - U256::ONE, power, power + U256::ONE]);
    }
    values.push(U256::MAX);
    values[rng.below(values.len() as u64) as usize]
}

/// Keeps the bits values of `ty` may hold.
fn mask(value: U256, ty: MirType) -> U256 {
    match ty {
        MirType::Int(bits) if bits.get() < 256 => value & (U256::MAX >> (256 - bits.get())),
        _ => value,
    }
}

fn describe_input(input: &Input) -> String {
    let mut text = String::new();
    for (index, arg) in input.args.iter().enumerate() {
        let _ = write!(text, "arg{index} = {arg:#x}, ");
    }
    let _ = write!(text, "free memory pointer {:#x}", input.free_memory_pointer);
    text
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
