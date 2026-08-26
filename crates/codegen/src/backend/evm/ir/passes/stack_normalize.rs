//! Bounded normalization of scheduled physical stack operations.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, default_instruction_stack_effect},
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
    let mut changed = false;
    let mut cursor = 0;
    while cursor < instructions.len() {
        let Some(_) = stack_op(&instructions[cursor]) else {
            cursor += 1;
            continue;
        };
        let start = cursor;
        while cursor < instructions.len() && stack_op(&instructions[cursor]).is_some() {
            cursor += 1;
        }
        let input_len = cursor - start;
        if input_len > MAX_STACK_RUN_LEN {
            continue;
        }

        let input: Vec<_> = instructions[start..cursor]
            .iter()
            .map(|inst| stack_op(inst).expect("physical stack run was checked"))
            .collect();
        let Some(output) = resynthesize_physical_ops(&input) else { continue };
        let input_gas = static_gas(&input);
        let output_gas = static_gas(&output);
        if output.len() > input.len()
            || output_gas > input_gas
            || (output.len() == input.len() && output_gas == input_gas)
        {
            continue;
        }

        let output: Vec<_> = output
            .into_iter()
            .enumerate()
            .map(|(index, op)| {
                let mut inst = instructions[start + index].clone();
                inst.opcode = op.opcode();
                inst.metadata.stack = None;
                inst
            })
            .collect();
        let output_len = output.len();
        instructions.splice(start..cursor, output);
        cursor = start + output_len;
        changed = true;
    }
    changed
}

fn stack_op(inst: &Instruction) -> Option<StackOp> {
    inst.metadata
        .stack
        .is_none_or(|effect| Some(effect) == default_instruction_stack_effect(inst))
        .then_some(())?;
    StackOp::from_opcode(inst.raw_opcode()?)
}

fn static_gas(ops: &[StackOp]) -> usize {
    ops.iter().map(|op| if matches!(op, StackOp::Pop) { 2 } else { 3 }).sum()
}
