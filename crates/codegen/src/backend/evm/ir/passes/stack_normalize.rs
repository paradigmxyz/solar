//! Bounded normalization of scheduled physical stack operations.

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
        for block in &mut module.blocks {
            changed |=
                normalize_runs(&mut block.instructions, gcx.sess.opts.evm_version, &mut cache);
        }
        changed
    }
}

type StackRun = SmallVec<[StackOp; MAX_STACK_RUN_LEN]>;
type NormalizationCache = FxHashMap<StackRun, Option<StackRun>>;

fn normalize_runs(
    instructions: &mut Vec<Instruction>,
    evm_version: EvmVersion,
    cache: &mut NormalizationCache,
) -> bool {
    struct Normalization {
        start: usize,
        end: usize,
        output: StackRun,
    }

    let mut normalizations = Vec::new();
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

    let source = std::mem::take(instructions);
    instructions.reserve(source.len());
    let mut source = source.into_iter().enumerate().peekable();
    for normalization in normalizations {
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
