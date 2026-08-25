//! Reorders pushes around self-contained read-only expressions.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, default_instruction_stack_effect},
    op,
};
use solar_sema::Gcx;

pub(super) struct ReorderPushes;

impl EvmPass for ReorderPushes {
    fn name(&self) -> &'static str {
        "reorder-pushes"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let mut scratch = Vec::new();
        for block in &mut module.blocks {
            changed |= reorder(&mut block.instructions, &mut scratch);
        }
        changed
    }
}

fn reorder(instructions: &mut Vec<Instruction>, scratch: &mut Vec<Instruction>) -> bool {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());

    let mut changed = false;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        let start = if let [.., pushed, swap] = instructions.as_slice()
            && pushed.is_encoded_push()
            && raw_opcode(swap) == Some(op::SWAP1)
        {
            self_contained_producer_start(&instructions[..instructions.len() - 2])
        } else {
            None
        };
        if let Some(start) = start {
            instructions.pop();
            let pushed = instructions.pop().expect("matched push must exist");
            instructions.insert(start, pushed);
            changed = true;
        }
    }
    changed
}

fn self_contained_producer_start(instructions: &[Instruction]) -> Option<usize> {
    let mut needed = 1usize;
    for (index, inst) in instructions.iter().enumerate().rev() {
        if inst.is_physical_stack_op()
            || raw_opcode(inst).is_some_and(|opcode| !op::is_read_only(opcode))
        {
            return None;
        }

        let effect = inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))?;
        let outputs = usize::from(effect.outputs);
        if outputs > needed {
            return None;
        }
        needed = needed - outputs + usize::from(effect.inputs);
        if needed == 0 {
            return Some(index);
        }
    }
    None
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}
