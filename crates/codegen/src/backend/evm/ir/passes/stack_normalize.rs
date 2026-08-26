//! Bounded normalization of scheduled physical stack operations.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    stack::{StackOp, resynthesize_physical_ops},
};
use solar_sema::Gcx;

const MAX_STACK_RUN_LEN: usize = 24;

pub(super) struct StackNormalize;

impl EvmPass for StackNormalize {
    fn name(&self) -> &'static str {
        "stack-normalize"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= normalize_runs(&mut block.instructions);
        }
        changed
    }
}

fn normalize_runs(instructions: &mut Vec<Instruction>) -> bool {
    let source = std::mem::take(instructions);
    instructions.reserve(source.len());
    let mut source = source.into_iter().peekable();
    let mut run = Vec::new();
    let mut input = Vec::new();
    let mut changed = false;
    while let Some(inst) = source.next() {
        if stack_op(&inst).is_none() {
            instructions.push(inst);
            continue;
        }

        run.clear();
        run.push(inst);
        while source.peek().is_some_and(|inst| stack_op(inst).is_some()) {
            run.push(source.next().unwrap());
        }
        if !(2..=MAX_STACK_RUN_LEN).contains(&run.len()) {
            instructions.append(&mut run);
            continue;
        }

        input.clear();
        input.extend(run.iter().map(|inst| stack_op(inst).unwrap()));
        let Some(output) = resynthesize_physical_ops(&input) else {
            instructions.append(&mut run);
            continue;
        };
        let input_gas = input.iter().map(|op| op.static_gas()).sum::<u32>();
        let output_gas = output.iter().map(|op| op.static_gas()).sum::<u32>();
        if output.len() > input.len()
            || output_gas > input_gas
            || (output.len() == input.len() && output_gas == input_gas)
        {
            instructions.append(&mut run);
            continue;
        }

        for (op, mut inst) in output.into_iter().zip(run.drain(..)) {
            inst.opcode = op.opcode();
            inst.metadata.stack = None;
            instructions.push(inst);
        }
        changed = true;
    }
    changed
}

fn stack_op(inst: &Instruction) -> Option<StackOp> {
    inst.has_canonical_stack_effect().then_some(())?;
    StackOp::from_opcode(inst.raw_opcode()?)
}
