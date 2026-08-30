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
//! strictly improving at least one. These Pareto checks prevent target-specific deep stack ops from
//! trading a regression in one objective for a win in another. Instructions with custom stack
//! effects break a run, and metadata from retained positions is transferred to replacement ops.
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
        for block in &mut module.blocks {
            changed |= normalize_runs(
                &mut block.instructions,
                gcx.sess.opts.evm_version,
                &mut cache,
                &mut scratch,
                &mut normalizations,
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
) -> bool {
    normalizations.clear();
    let mut input = StackRun::new();
    let mut cursor = 0;
    while cursor < instructions.len() {
        let start = cursor;
        input.clear();
        while cursor < instructions.len()
            && let Some(op) = stack_op(&instructions[cursor])
        {
            input.push(op);
            cursor += 1;
        }
        if cursor == start {
            cursor += 1;
            continue;
        }
        if !(2..=MAX_STACK_RUN_LEN).contains(&input.len()) {
            continue;
        }

        let Some(output) = cache
            .entry(input.clone())
            .or_insert_with(|| {
                let output = StackRun::from_vec(resynthesize_physical_ops(&input, evm_version)?);
                if relative_peak(&output) > relative_peak(&input) {
                    return None;
                }
                let input_cost = lowered_stack_cost(&input, evm_version);
                let output_cost = lowered_stack_cost(&output, evm_version);
                (output_cost.0 <= input_cost.0
                    && output_cost.1 <= input_cost.1
                    && output_cost.2 <= input_cost.2
                    && output_cost != input_cost)
                    .then_some(output)
            })
            .clone()
        else {
            continue;
        };
        normalizations.push(Normalization { start, end: cursor, output });
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
