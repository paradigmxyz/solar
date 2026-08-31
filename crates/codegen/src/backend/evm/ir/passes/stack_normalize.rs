//! Bounded normalization of scheduled physical stack operations.
//!
//! Stack scheduling and later deletion passes can leave adjacent `DUP`, `SWAP`, `EXCHANGE`, and
//! `POP` operations that implement a non-minimal physical permutation. This pass splits each block
//! into maximal canonical stack-op runs of at most 24 instructions, symbolically computes the run's
//! input and output layouts, and asks the shared stack shuffler to synthesize an equivalent run.
//! Results are cached by input sequence because generated code often repeats the same shuffle.
//!
//! A replacement must be lowerable on the selected EVM version, must not raise the run's relative
//! stack peak, and must weakly improve encoded bytes, static gas, and instruction count while
//! strictly improving at least one. It also preserves the minimum entry depth unless the preceding
//! block prefix guarantees enough words for the original run. This keeps an underflowing run from
//! becoming executable when an opaque jump reaches it with too few words. The Pareto checks prevent
//! target-specific deep stack ops from trading a regression in one objective for a win in another.
//! Instructions with custom stack effects break a run, and metadata from retained positions is
//! transferred to replacement ops.
//!
//! This is deliberately a small late machine-level normalizer, not a second MIR stack scheduler.
//! It repairs local permutations exposed after value identities are gone, then peephole cleanup
//! removes any simpler identities that become adjacent. The length bound keeps symbolic
//! resynthesis and cache keys independent of function size.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    stack::{StackOp, lowered_stack_cost, resynthesize_physical_ops},
};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

const MAX_STACK_RUN_LEN: usize = 24;

pub(super) struct StackNormalize;

impl EvmPass for StackNormalize {
    fn name(&self) -> &'static str {
        "stack-normalize"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut cache = FxHashMap::default();
        let mut scratch = Vec::new();
        let mut normalizations = Vec::new();
        let mut guaranteed_depths = Vec::new();
        for block in &mut module.blocks {
            changed |= normalize_runs(
                &mut block.instructions,
                gcx.sess.opts.evm_version,
                &mut cache,
                &mut scratch,
                &mut normalizations,
                &mut guaranteed_depths,
            );
        }
        changed
    }
}

type StackRun = SmallVec<[StackOp; MAX_STACK_RUN_LEN]>;
type NormalizationCache = FxHashMap<StackRun, Option<StackRun>>;

struct Normalization {
    start: usize,
    end: usize,
    output: StackRun,
}

fn normalize_runs(
    instructions: &mut Vec<Instruction>,
    evm_version: EvmVersion,
    cache: &mut NormalizationCache,
    scratch: &mut Vec<Instruction>,
    normalizations: &mut Vec<Normalization>,
    guaranteed_depths: &mut Vec<usize>,
) -> bool {
    normalizations.clear();
    if !instructions.windows(2).any(|window| window.iter().all(|inst| stack_op(inst).is_some())) {
        return false;
    }
    compute_guaranteed_depths(instructions, guaranteed_depths);
    let mut input = StackRun::new();
    let mut cursor = 0;
    while cursor < instructions.len() {
        let run_start = cursor;
        while cursor < instructions.len() && stack_op(&instructions[cursor]).is_some() {
            cursor += 1;
        }
        if cursor == run_start {
            cursor += 1;
            continue;
        }
        let mut start = run_start;
        while start < cursor {
            let remaining = cursor - start;
            let len = if remaining == MAX_STACK_RUN_LEN + 1 {
                MAX_STACK_RUN_LEN - 1
            } else {
                remaining.min(MAX_STACK_RUN_LEN)
            };
            let end = start + len;
            input.clear();
            input.extend(instructions[start..end].iter().filter_map(stack_op));
            if input.len() >= 2
                && let Some(output) =
                    normalization(&input, evm_version, guaranteed_depths[start], cache)
            {
                normalizations.push(Normalization { start, end, output });
            }
            start = end;
        }
    }
    if normalizations.is_empty() {
        return false;
    }

    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut source = scratch.drain(..).enumerate().peekable();
    for normalization in normalizations.drain(..) {
        while source.peek().is_some_and(|&(index, _)| index < normalization.start) {
            instructions.push(source.next().unwrap().1);
        }
        let mut original = source.by_ref().take(normalization.end - normalization.start);
        for op in normalization.output {
            let mut replacement = Instruction::stack_op(op);
            if let Some((_, mut inst)) = original.next() {
                replacement.metadata = std::mem::take(&mut inst.metadata);
                replacement.metadata.stack = None;
            }
            instructions.push(replacement);
        }
        original.for_each(drop);
    }
    instructions.extend(source.map(|(_, inst)| inst));
    true
}

fn normalization(
    input: &StackRun,
    evm_version: EvmVersion,
    guaranteed_depth: usize,
    cache: &mut NormalizationCache,
) -> Option<StackRun> {
    let output = if let Some(output) = cache.get(input) {
        output.clone()
    } else {
        let output = compute_normalization(input, evm_version);
        cache.insert(input.clone(), output.clone());
        output
    }?;
    let input_required = required_entry_depth(input);
    (required_entry_depth(&output) == input_required || guaranteed_depth >= input_required)
        .then_some(output)
}

fn compute_normalization(input: &StackRun, evm_version: EvmVersion) -> Option<StackRun> {
    let output = StackRun::from_vec(resynthesize_physical_ops(input, evm_version)?);
    if required_entry_depth(&output) > required_entry_depth(input)
        || relative_peak(&output) > relative_peak(input)
    {
        return None;
    }
    let input_cost = lowered_stack_cost(input, evm_version);
    let output_cost = lowered_stack_cost(&output, evm_version);
    (output_cost.0 <= input_cost.0
        && output_cost.1 <= input_cost.1
        && output_cost.2 <= input_cost.2
        && output_cost != input_cost)
        .then_some(output)
}

fn stack_op(inst: &Instruction) -> Option<StackOp> {
    inst.has_canonical_stack_effect().then_some(())?;
    inst.as_stack_op()
}

fn relative_peak(ops: &[StackOp]) -> isize {
    let mut depth = 0isize;
    let mut peak = 0isize;
    for op in ops {
        depth += op.net_growth();
        peak = peak.max(depth);
    }
    peak
}

fn required_entry_depth(ops: &[StackOp]) -> usize {
    let mut depth = 0isize;
    let mut required = 0isize;
    for op in ops {
        required = required.max(op.required_depth() as isize - depth);
        depth += op.net_growth();
    }
    required as usize
}

fn compute_guaranteed_depths(instructions: &[Instruction], depths: &mut Vec<usize>) {
    depths.clear();
    depths.reserve(instructions.len() + 1);
    depths.push(0);
    let mut relative_depth = 0isize;
    let mut required_entry = 0isize;
    for inst in instructions {
        let Some(effect) = inst.effective_stack_effect() else {
            relative_depth = 0;
            required_entry = 0;
            depths.push(0);
            continue;
        };
        let required =
            inst.as_stack_op().map_or(usize::from(effect.inputs), StackOp::required_depth);
        required_entry = required_entry.max(required as isize - relative_depth);
        relative_depth += isize::from(effect.outputs) - isize::from(effect.inputs);
        depths.push((required_entry + relative_depth) as usize);
    }
}
