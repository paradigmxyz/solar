//! Bounded normalization of scheduled physical stack operations.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    stack::{StackOp, lowered_stack_cost, resynthesize_physical_ops},
};
use solar_config::EvmVersion;
use solar_sema::Gcx;

const MAX_STACK_RUN_LEN: usize = 24;

pub(super) struct StackNormalize;

impl EvmPass for StackNormalize {
    fn name(&self) -> &'static str {
        "stack-normalize"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= normalize_runs(&mut block.instructions, gcx.sess.opts.evm_version);
        }
        changed
    }
}

fn normalize_runs(instructions: &mut Vec<Instruction>, evm_version: EvmVersion) -> bool {
    struct Normalization {
        start: usize,
        end: usize,
        output: Vec<StackOp>,
    }

    let mut normalizations = Vec::new();
    let mut input = Vec::new();
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

        let Some(output) = resynthesize_physical_ops(&input, evm_version) else {
            continue;
        };
        if relative_peak(&output) > relative_peak(&input) {
            continue;
        }
        let input_cost = lowered_stack_cost(&input, evm_version);
        let output_cost = lowered_stack_cost(&output, evm_version);
        if output_cost.0 > input_cost.0
            || output_cost.1 > input_cost.1
            || output_cost.2 > input_cost.2
            || output_cost == input_cost
        {
            continue;
        }
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
        for op in normalization.output {
            let (_, mut inst) = source.next().unwrap();
            let mut replacement = Instruction::stack_op(op);
            replacement.metadata = std::mem::take(&mut inst.metadata);
            replacement.metadata.stack = None;
            instructions.push(replacement);
        }
        while source.peek().is_some_and(|&(index, _)| index < normalization.end) {
            source.next();
        }
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
        match op {
            StackOp::Dup(_) => depth += 1,
            StackOp::Pop => depth -= 1,
            StackOp::Swap(_) | StackOp::Exchange(_, _) => {}
        }
        peak = peak.max(depth);
    }
    peak
}
