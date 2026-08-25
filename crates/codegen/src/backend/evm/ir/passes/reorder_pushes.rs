//! Reorders pushes around self-contained read-only expressions.

use super::{EvmPass, utils::StackDepths};
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, StackEffect, default_instruction_stack_effect},
    op,
    stack::MAX_STACK_DEPTH,
};
use solar_sema::Gcx;

pub(super) struct ReorderPushes;

impl EvmPass for ReorderPushes {
    fn name(&self) -> &'static str {
        "reorder-pushes"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !module.blocks.iter().any(|block| has_candidate(&block.instructions)) {
            return false;
        }
        let Some(depths) = StackDepths::new(module) else { return false };
        let mut changed = false;
        let mut scratch = Vec::new();
        for block_index in 0..module.blocks.len() {
            let block_id = BlockId::from_usize(block_index);
            let Some(entry_depth) = depths.entry_depth(block_id) else { continue };
            changed |=
                reorder(&mut module.blocks[block_id].instructions, entry_depth, &mut scratch);
        }
        changed
    }
}

fn has_candidate(instructions: &[Instruction]) -> bool {
    instructions
        .windows(2)
        .any(|pair| pair[0].is_encoded_push() && raw_opcode(&pair[1]) == Some(op::SWAP1))
}

fn reorder(
    instructions: &mut Vec<Instruction>,
    entry_depth: usize,
    scratch: &mut Vec<Instruction>,
) -> bool {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());

    let mut changed = false;
    let mut depth = entry_depth;
    for inst in scratch.drain(..) {
        let effect = stack_effect(&inst);
        instructions.push(inst);
        if let Some(effect) = effect {
            let Some(after_inputs) = depth.checked_sub(usize::from(effect.inputs)) else {
                return changed;
            };
            depth = after_inputs + usize::from(effect.outputs);
        }
        let producer = if let [.., pushed, swap] = instructions.as_slice()
            && pushed.is_encoded_push()
            && raw_opcode(swap) == Some(op::SWAP1)
        {
            self_contained_producer(&instructions[..instructions.len() - 2])
        } else {
            None
        };
        if let Some((start, peak)) = producer
            && depth
                .checked_sub(2)
                .and_then(|producer_depth| producer_depth.checked_add(peak + 1))
                .is_some_and(|peak| peak <= MAX_STACK_DEPTH)
        {
            instructions.pop();
            instructions[start..].rotate_right(1);
            changed = true;
        }
    }
    changed
}

fn self_contained_producer(instructions: &[Instruction]) -> Option<(usize, usize)> {
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
            let mut depth = 0usize;
            let mut peak = 0usize;
            for inst in &instructions[index..] {
                let effect = stack_effect(inst)?;
                depth =
                    depth.checked_sub(usize::from(effect.inputs))? + usize::from(effect.outputs);
                peak = peak.max(depth);
            }
            return Some((index, peak));
        }
    }
    None
}

fn stack_effect(inst: &Instruction) -> Option<StackEffect> {
    inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}
