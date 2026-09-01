//! Move encoded pushes before self-contained producers to remove a following `SWAP1`.
//!
//! Physical scheduling can emit `producer; PUSH value; SWAP1` when the producer must run before an
//! immediate operand is materialized. If the producer is a closed expression and none of its
//! instructions observes the extra stack word, this pass rewrites the sequence to `PUSH value;
//! producer`. The values reach the consumer in the same order and the `SWAP1` disappears. A linked
//! instruction sequence lets chained rewrites move whole expression fragments in linear time.
//! The expression-range rewrite handles compact multi-instruction immediate recipes, whether the
//! recipe is the producer or the value that must move before it. A separate backward matcher turns
//! `DUPn; unary*; immediate recipe; SWAP1` into `immediate recipe; DUP(n+1); unary*`. The exact
//! DUP rebasing checks target reach and accepts only a Pareto improvement after target-specific
//! stack-op lowering.
//!
//! The expression tracker accepts only known one-result operations, rejects physical stack
//! instructions and observations such as `PC` or `GAS`, and clears at unknown effects. Moving a
//! push can increase the producer's absolute stack peak, so the pass first admits rewrites that do
//! not exceed the block's existing relative high-water mark. Remaining candidates use CFG-derived
//! entry depths and are rejected when dynamic control flow or an unbounded path prevents a proof.
//! Net stack effects do not change, which lets the second phase reuse depths after the first phase.
//!
//! For a logical concrete immediate, the pass queries the compact-push selector and accounts for
//! the selected recipe's transient peak without first expanding it. This also keeps standalone pass
//! pipelines safe when they run before compacting pushes. The first expression sweep is disabled
//! for pre-extended-stack size builds because its interaction with later structural cleanup can
//! increase code size there. The final sweep runs after structural sharing is fixed and enables it
//! safely. The exact mixed-stack rules run in every optimized mode because they preserve the
//! surrounding value layout and cannot increase local cost.

use super::{EvmPass, compact_pushes::ImmediateMaterialization, utils::StackDepths};
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module},
    op::{self, StackOp},
    stack::MAX_STACK_DEPTH,
};
use solar_config::{EvmVersion, OptimizationMode};
use solar_sema::Gcx;

pub(super) const REORDER_PUSHES: ReorderPushes =
    ReorderPushes { reorder_legacy_size_expressions: false };
pub(super) const FINAL_REORDER_PUSHES: ReorderPushes =
    ReorderPushes { reorder_legacy_size_expressions: true };

pub(super) struct ReorderPushes {
    reorder_legacy_size_expressions: bool,
}

impl EvmPass for ReorderPushes {
    fn name(&self) -> &'static str {
        "reorder-pushes"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let evm_version = gcx.sess.opts.evm_version;
        let reorder_expressions = self.reorder_legacy_size_expressions
            || !gcx.sess.opts.optimization.is_size()
            || gcx.sess.opts.evm_version.has_extended_stack_ops();
        let mut changed = false;
        let mut pending = Vec::new();
        for index in 0..module.blocks.len() {
            let block_id = BlockId::from_usize(index);
            let Some(high_water) = candidate_high_water(module, block_id) else { continue };
            let instructions = &mut module.blocks[block_id].instructions;
            let result = reorder(instructions, None, high_water, evm_version, reorder_expressions);
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
                changed |= reorder(
                    instructions,
                    entry_depth,
                    high_water,
                    evm_version,
                    reorder_expressions,
                )
                .changed;
            }
        }
        changed
    }
}

struct ReorderResult {
    changed: bool,
    needs_depths: bool,
}

fn reorder(
    instructions: &mut Vec<Instruction>,
    entry_depth: Option<usize>,
    high_water: Option<isize>,
    evm_version: EvmVersion,
    reorder_expressions: bool,
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

        if inst.as_stack_op() == Some(StackOp::Swap(1))
            && inst.has_canonical_stack_effect()
            && let Some(pushed) = expressions.last()
            && pushed.immediate_recipe
            && let Some(pushed_end) = sequence.last
            && let Some((dup_node, rebased)) =
                rebasable_dup_before(&sequence, pushed.start, evm_version)
        {
            let source_peak = 1 + pushed.peak;
            let reordered_peak = pushed.peak.max(2);
            if source_peak <= reordered_peak || source_peak_fits(depth, source_peak) {
                sequence.replace_stack_op(dup_node, rebased);
                sequence.move_range_before(pushed.start, pushed_end, dup_node);
                expressions.clear();
                changed = true;
                continue;
            }
            needs_depths |= depth.is_none();
        }

        if reorder_expressions
            && inst.as_stack_op() == Some(StackOp::Swap(1))
            && inst.has_canonical_stack_effect()
            && let [.., producer, pushed] = expressions.as_slice()
            && pushed.immediate_recipe
            && let Some(pushed_end) = sequence.last
        {
            let (producer, pushed) = (*producer, *pushed);
            let source_peak = producer.peak.max(1 + pushed.peak);
            let reordered_peak = pushed.peak.max(1 + producer.peak);
            let added_peak = reordered_peak - 2;
            let local_peak_fits = high_water.is_some_and(|high_water| {
                relative_depth
                    .checked_add_unsigned(added_peak)
                    .is_some_and(|peak| peak <= high_water)
            });
            let source_fits = source_peak <= reordered_peak || source_peak_fits(depth, source_peak);
            if source_fits
                && (added_peak == 0 || local_peak_fits || reordered_peak_fits(depth, added_peak))
            {
                sequence.move_range_before(pushed.start, pushed_end, producer.start);
                let len = expressions.len();
                expressions.swap(len - 2, len - 1);
                changed = true;
                continue;
            } else if depth.is_none() {
                needs_depths = true;
            }
        }

        let node = sequence.push(inst);
        update_expressions(&mut expressions, &sequence, node, evm_version);
    }
    sequence.finish_into(&mut source);
    *instructions = source;
    ReorderResult { changed, needs_depths }
}

fn rebase_dup(evm_version: EvmVersion, depth: u8) -> Option<StackOp> {
    let original = StackOp::Dup(depth).metrics(evm_version)?;
    let rebased = StackOp::Dup(depth.checked_add(1)?);
    let replacement = rebased.metrics(evm_version)?;
    let removed = StackOp::Swap(1).metrics(evm_version)?;
    (replacement.assembled_len <= original.assembled_len + removed.assembled_len
        && replacement.static_gas <= original.static_gas + removed.static_gas
        && replacement.instruction_count <= original.instruction_count + removed.instruction_count)
        .then_some(rebased)
}

fn rebasable_dup_before(
    sequence: &InstructionSequence,
    before: usize,
    evm_version: EvmVersion,
) -> Option<(usize, StackOp)> {
    let mut node = sequence.previous(before)?;
    loop {
        let inst = sequence.instruction(node);
        if let Some(StackOp::Dup(depth)) = inst.as_stack_op() {
            if !inst.has_canonical_stack_effect() {
                return None;
            }
            return Some((node, rebase_dup(evm_version, depth)?));
        }
        let effect = inst.effective_stack_effect()?;
        if !inst.has_canonical_stack_effect()
            || inst.is_physical_stack_op()
            || !inst.as_opcode().is_some_and(op::is_unaffected_by_preceding_push)
            || effect.inputs != 1
            || effect.outputs != 1
        {
            return None;
        }
        node = sequence.previous(node)?;
    }
}

fn reordered_peak_fits(depth: Option<usize>, added_peak: usize) -> bool {
    depth
        .and_then(|depth| depth.checked_add(added_peak))
        .is_some_and(|depth| depth <= MAX_STACK_DEPTH)
}

fn source_peak_fits(depth: Option<usize>, source_peak: usize) -> bool {
    depth
        .and_then(|depth| depth.checked_sub(2))
        .and_then(|depth| depth.checked_add(source_peak))
        .is_some_and(|depth| depth <= MAX_STACK_DEPTH)
}

#[derive(Clone, Copy)]
struct Expression {
    start: usize,
    peak: usize,
    immediate_recipe: bool,
}

fn update_expressions(
    expressions: &mut Vec<Expression>,
    sequence: &InstructionSequence,
    node: usize,
    evm_version: EvmVersion,
) {
    let inst = sequence.instruction(node);
    let effect = if let Some(effect) = inst.effective_stack_effect()
        && inst.has_canonical_stack_effect()
        && !inst.is_physical_stack_op()
        && inst.as_opcode().is_none_or(op::is_unaffected_by_preceding_push)
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
        expressions.push(Expression {
            start: node,
            peak: inst
                .concrete_immediate()
                .map_or(1, |value| ImmediateMaterialization::new(evm_version, value).stack_peak()),
            immediate_recipe: inst.is_encoded_push(),
        });
        return;
    }
    let start = expressions.len() - inputs;
    let immediate_recipe =
        inst.as_opcode().is_some_and(|opcode| matches!(opcode, op::NOT | op::SHL | op::SHR))
            && expressions[start..].iter().all(|expression| expression.immediate_recipe);
    let peak = expressions[start..]
        .iter()
        .enumerate()
        .map(|(offset, expression)| offset + expression.peak)
        .max()
        .unwrap_or(1);
    let first = expressions[start].start;
    expressions.truncate(start);
    expressions.push(Expression { start: first, peak, immediate_recipe });
}

fn candidate_high_water(module: &Module, block_id: BlockId) -> Option<Option<isize>> {
    let block = &module.blocks[block_id];
    let mut depth = Some(0isize);
    let mut high_water = Some(0isize);
    let mut found = false;
    for inst in &block.instructions {
        found |= inst.as_stack_op() == Some(StackOp::Swap(1));
        if let Some(effect) = inst.effective_stack_effect()
            && let Some(current) = depth
        {
            let current = current + isize::from(effect.outputs) - isize::from(effect.inputs);
            depth = Some(current);
            high_water = high_water.map(|high_water| high_water.max(current));
        } else {
            depth = None;
            high_water = None;
        }
    }
    // Block layout may change whether the terminator needs a target push. Only instruction peaks
    // are stable across later layouts and can justify a local rewrite.
    found.then_some(high_water)
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

    fn previous(&self, index: usize) -> Option<usize> {
        self.nodes[index].previous
    }

    fn replace_stack_op(&mut self, index: usize, stack_op: StackOp) {
        let metadata =
            std::mem::take(&mut self.nodes[index].instruction.as_mut().unwrap().metadata);
        let mut replacement = Instruction::stack_op(stack_op);
        replacement.metadata = metadata;
        replacement.metadata.stack = None;
        self.nodes[index].instruction = Some(replacement);
    }

    fn move_range_before(&mut self, start: usize, end: usize, before: usize) {
        let previous = self.nodes[start].previous;
        let next = self.nodes[end].next;
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
        self.nodes[start].previous = previous;
        self.nodes[end].next = Some(before);
        self.nodes[before].previous = Some(end);
        if let Some(previous) = previous {
            self.nodes[previous].next = Some(start);
        } else {
            self.first = Some(start);
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
