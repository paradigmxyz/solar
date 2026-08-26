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
    struct Normalization {
        start: usize,
        end: usize,
        output: Vec<StackOp>,
    }

    let mut normalizations = Vec::new();
    let mut input = Vec::new();
    let mut cursor = 0;
    while cursor < instructions.len() {
        if stack_op(&instructions[cursor]).is_none() {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < instructions.len() && stack_op(&instructions[cursor]).is_some() {
            cursor += 1;
        }
        if !(2..=MAX_STACK_RUN_LEN).contains(&(cursor - start)) {
            continue;
        }

        input.clear();
        input.extend(instructions[start..cursor].iter().map(|inst| stack_op(inst).unwrap()));
        let Some(output) = resynthesize_physical_ops(&input) else {
            continue;
        };
        let input_gas = input.iter().map(|op| op.static_gas()).sum::<u32>();
        let output_gas = output.iter().map(|op| op.static_gas()).sum::<u32>();
        if output.len() > input.len()
            || output_gas > input_gas
            || (output.len() == input.len() && output_gas == input_gas)
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
            inst.opcode = op.opcode();
            inst.metadata.stack = None;
            instructions.push(inst);
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
    StackOp::from_opcode(inst.raw_opcode()?)
}
