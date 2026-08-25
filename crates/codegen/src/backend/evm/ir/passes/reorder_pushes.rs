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
        let mut changed = false;
        let mut needs_depths = false;
        let mut scratch = Vec::new();
        for block_index in 0..module.blocks.len() {
            let block_id = BlockId::from_usize(block_index);
            let instructions = &mut module.blocks[block_id].instructions;
            if has_candidate(instructions) {
                let result = reorder(instructions, None, &mut scratch);
                changed |= result.changed;
                needs_depths |= result.needs_depths;
            }
        }
        if needs_depths && let Some(depths) = StackDepths::new(module) {
            for block_index in 0..module.blocks.len() {
                let block_id = BlockId::from_usize(block_index);
                let instructions = &mut module.blocks[block_id].instructions;
                if has_candidate(instructions) {
                    let entry_depth = depths.entry_depth(block_id);
                    changed |= reorder(instructions, entry_depth, &mut scratch).changed;
                }
            }
        }
        changed
    }
}

fn has_candidate(instructions: &[Instruction]) -> bool {
    instructions
        .windows(2)
        .any(|pair| pair[0].is_encoded_push() && pair[1].raw_opcode() == Some(op::SWAP1))
}

struct ReorderResult {
    changed: bool,
    needs_depths: bool,
}

fn reorder(
    instructions: &mut Vec<Instruction>,
    entry_depth: Option<usize>,
    scratch: &mut Vec<Instruction>,
) -> ReorderResult {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());

    let mut changed = false;
    let mut needs_depths = false;
    let mut depth = entry_depth;
    for inst in scratch.drain(..) {
        let effect = stack_effect(&inst);
        instructions.push(inst);
        depth = effect.and_then(|effect| {
            depth.and_then(|before| {
                before
                    .checked_sub(usize::from(effect.inputs))
                    .map(|after_inputs| after_inputs + usize::from(effect.outputs))
            })
        });
        let producer = if let [.., pushed, swap] = instructions.as_slice()
            && pushed.is_encoded_push()
            && swap.raw_opcode() == Some(op::SWAP1)
        {
            self_contained_producer(&instructions[..instructions.len() - 2])
        } else {
            None
        };
        if let Some((start, peak)) = producer {
            if peak == 1 || reordered_peak_fits(depth, peak) {
                instructions.pop();
                instructions[start..].rotate_right(1);
                changed = true;
            } else if depth.is_none() {
                needs_depths = true;
            }
        }
    }
    ReorderResult { changed, needs_depths }
}

fn reordered_peak_fits(depth: Option<usize>, producer_peak: usize) -> bool {
    depth
        .and_then(|depth| depth.checked_add(producer_peak - 1))
        .is_some_and(|depth| depth <= MAX_STACK_DEPTH)
}

fn self_contained_producer(instructions: &[Instruction]) -> Option<(usize, usize)> {
    let mut needed = 1usize;
    for (index, inst) in instructions.iter().enumerate().rev() {
        if inst.is_physical_stack_op()
            || inst.raw_opcode().is_some_and(|opcode| !op::is_read_only(opcode))
        {
            return None;
        }

        let effect = stack_effect(inst)?;
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
