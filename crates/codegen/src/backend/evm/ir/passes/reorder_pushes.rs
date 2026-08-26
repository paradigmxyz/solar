//! Reorders pushes around self-contained expressions that do not observe the push.

use super::{
    EvmPass,
    utils::{StackDepths, terminator_lowering_growth},
};
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module},
    op,
    stack::MAX_STACK_DEPTH,
};
use solar_config::OptimizationMode;
use solar_sema::Gcx;

pub(super) struct ReorderPushes;

impl EvmPass for ReorderPushes {
    fn name(&self) -> &'static str {
        "reorder-pushes"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
            && (!gcx.sess.opts.optimization.is_size()
                || gcx.sess.opts.evm_version.has_extended_stack_ops())
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let candidates = module
            .blocks
            .indices()
            .filter(|&block_id| has_candidate(&module.blocks[block_id].instructions))
            .map(|block_id| (block_id, relative_high_water(module, block_id)))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return false;
        }
        let mut changed = false;
        let mut pending = Vec::new();
        for &(block_id, high_water) in &candidates {
            let instructions = &mut module.blocks[block_id].instructions;
            let result = reorder(instructions, None, high_water);
            changed |= result.changed;
            if result.needs_depths {
                pending.push((block_id, high_water));
            }
        }
        if !pending.is_empty()
            && let Some(depths) = StackDepths::new(module)
        {
            for (block_id, high_water) in pending {
                let instructions = &mut module.blocks[block_id].instructions;
                let entry_depth = depths.entry_depth(block_id);
                changed |= reorder(instructions, entry_depth, high_water).changed;
            }
        }
        changed
    }
}

fn has_candidate(instructions: &[Instruction]) -> bool {
    instructions
        .windows(2)
        .any(|pair| pair[0].is_encoded_push() && pair[1].as_legacy_opcode() == Some(op::SWAP1))
}

struct ReorderResult {
    changed: bool,
    needs_depths: bool,
}

fn reorder(
    instructions: &mut Vec<Instruction>,
    entry_depth: Option<usize>,
    high_water: Option<isize>,
) -> ReorderResult {
    let mut source = Vec::new();
    std::mem::swap(instructions, &mut source);
    let mut sequence = InstructionSequence::with_capacity(source.len());
    let mut expressions = Vec::<Expression>::new();
    let mut changed = false;
    let mut needs_depths = false;
    let mut depth = entry_depth;
    let mut relative_depth = 0isize;
    for inst in source.drain(..) {
        let effect = inst.effective_stack_effect();
        depth = effect.and_then(|effect| {
            depth.and_then(|before| {
                before
                    .checked_sub(usize::from(effect.inputs))
                    .map(|after_inputs| after_inputs + usize::from(effect.outputs))
            })
        });
        if let Some(effect) = effect {
            relative_depth += isize::from(effect.outputs) - isize::from(effect.inputs);
        }

        if inst.as_legacy_opcode() == Some(op::SWAP1)
            && let [.., producer, pushed] = expressions.as_slice()
            && sequence.last == Some(pushed.start)
            && sequence.instruction(pushed.start).is_encoded_push()
        {
            let (producer, pushed) = (*producer, *pushed);
            let local_peak_fits = high_water.is_some_and(|high_water| {
                relative_depth
                    .checked_add_unsigned(producer.peak - 1)
                    .is_some_and(|peak| peak <= high_water)
            });
            if producer.peak == 1 || local_peak_fits || reordered_peak_fits(depth, producer.peak) {
                sequence.move_before(pushed.start, producer.start);
                let len = expressions.len();
                expressions.swap(len - 2, len - 1);
                changed = true;
                continue;
            } else if depth.is_none() {
                needs_depths = true;
            }
        }
        let node = sequence.push(inst);
        update_expressions(&mut expressions, &sequence, node);
    }
    sequence.finish_into(&mut source);
    *instructions = source;
    ReorderResult { changed, needs_depths }
}

fn reordered_peak_fits(depth: Option<usize>, producer_peak: usize) -> bool {
    depth
        .and_then(|depth| depth.checked_add(producer_peak - 1))
        .is_some_and(|depth| depth <= MAX_STACK_DEPTH)
}

#[derive(Clone, Copy)]
struct Expression {
    start: usize,
    peak: usize,
}

fn update_expressions(
    expressions: &mut Vec<Expression>,
    sequence: &InstructionSequence,
    node: usize,
) {
    let inst = sequence.instruction(node);
    let effect = if let Some(effect) = inst.effective_stack_effect()
        && !inst.is_physical_stack_op()
        && inst.as_legacy_opcode().is_none_or(op::is_unaffected_by_preceding_push)
        && effect.outputs == 1
        && usize::from(effect.inputs) <= expressions.len()
    {
        effect
    } else {
        expressions.clear();
        return;
    };
    let inputs = usize::from(effect.inputs);
    if inputs == 0 {
        expressions.push(Expression { start: node, peak: 1 });
        return;
    }
    let start = expressions.len() - inputs;
    let peak = expressions[start..]
        .iter()
        .enumerate()
        .map(|(offset, expression)| offset + expression.peak)
        .max()
        .unwrap_or(1);
    let first = expressions[start].start;
    expressions.truncate(start);
    expressions.push(Expression { start: first, peak });
}

fn relative_high_water(module: &Module, block_id: BlockId) -> Option<isize> {
    let block = &module.blocks[block_id];
    let mut depth = 0isize;
    let mut high_water = 0isize;
    for inst in &block.instructions {
        let effect = inst.effective_stack_effect()?;
        depth += isize::from(effect.outputs) - isize::from(effect.inputs);
        high_water = high_water.max(depth);
    }
    let growth = terminator_lowering_growth(module, block_id)?;
    Some(high_water.max(depth + growth as isize))
}

struct InstructionNode {
    instruction: Option<Instruction>,
    previous: Option<usize>,
    next: Option<usize>,
}

struct InstructionSequence {
    nodes: Vec<InstructionNode>,
    first: Option<usize>,
    last: Option<usize>,
}

impl InstructionSequence {
    fn with_capacity(capacity: usize) -> Self {
        Self { nodes: Vec::with_capacity(capacity), first: None, last: None }
    }

    fn push(&mut self, instruction: Instruction) -> usize {
        let index = self.nodes.len();
        self.nodes.push(InstructionNode {
            instruction: Some(instruction),
            previous: self.last,
            next: None,
        });
        if let Some(last) = self.last {
            self.nodes[last].next = Some(index);
        } else {
            self.first = Some(index);
        }
        self.last = Some(index);
        index
    }

    fn instruction(&self, index: usize) -> &Instruction {
        self.nodes[index].instruction.as_ref().unwrap()
    }

    fn move_before(&mut self, node: usize, before: usize) {
        let previous = self.nodes[node].previous;
        let next = self.nodes[node].next;
        if let Some(previous) = previous {
            self.nodes[previous].next = next;
        } else {
            self.first = next;
        }
        if let Some(next) = next {
            self.nodes[next].previous = previous;
        } else {
            self.last = previous;
        }

        let previous = self.nodes[before].previous;
        self.nodes[node].previous = previous;
        self.nodes[node].next = Some(before);
        self.nodes[before].previous = Some(node);
        if let Some(previous) = previous {
            self.nodes[previous].next = Some(node);
        } else {
            self.first = Some(node);
        }
    }

    fn finish_into(mut self, instructions: &mut Vec<Instruction>) {
        instructions.reserve(self.nodes.len());
        let mut current = self.first;
        while let Some(index) = current {
            let node = &mut self.nodes[index];
            instructions.push(node.instruction.take().unwrap());
            current = node.next;
        }
    }
}
