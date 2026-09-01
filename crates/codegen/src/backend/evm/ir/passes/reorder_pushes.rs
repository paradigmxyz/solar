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
//! instructions and observations such as `PC` or `GAS`, and clears at unknown effects.
//!
//! For a logical concrete immediate, the pass queries the compact-push selector and accounts for
//! the selected recipe's transient peak without first expanding it. This also keeps standalone pass
//! pipelines safe when they run before compacting pushes. The first expression sweep is disabled
//! for pre-extended-stack size builds because its interaction with later structural cleanup can
//! increase code size there. The final sweep runs after structural sharing is fixed and enables it
//! safely. The exact mixed-stack rules run in every optimized mode because they preserve the
//! surrounding value layout and cannot increase local cost.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    op::{self, StackOp},
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
        for block in &mut module.blocks {
            changed |= reorder(&mut block.instructions, evm_version, reorder_expressions);
        }
        changed
    }
}

fn reorder(
    instructions: &mut Vec<Instruction>,
    evm_version: EvmVersion,
    reorder_expressions: bool,
) -> bool {
    let mut source = Vec::new();
    std::mem::swap(instructions, &mut source);
    let mut sequence = InstructionSequence::with_capacity(source.len());
    let mut expressions = Vec::<Expression>::new();
    let mut changed = false;
    for inst in source.drain(..) {
        if inst.as_stack_op() == Some(StackOp::Swap(1))
            && inst.has_canonical_stack_effect()
            && let Some(pushed) = expressions.last()
            && pushed.immediate_recipe
            && let Some(pushed_end) = sequence.last
            && let Some((dup_node, rebased)) =
                rebasable_dup_before(&sequence, pushed.start, evm_version)
        {
            sequence.replace_stack_op(dup_node, rebased);
            sequence.move_range_before(pushed.start, pushed_end, dup_node);
            expressions.clear();
            changed = true;
            continue;
        }

        if reorder_expressions
            && inst.as_stack_op() == Some(StackOp::Swap(1))
            && inst.has_canonical_stack_effect()
            && let [.., producer, pushed] = expressions.as_slice()
            && pushed.immediate_recipe
            && let Some(pushed_end) = sequence.last
        {
            let (producer, pushed) = (*producer, *pushed);
            sequence.move_range_before(pushed.start, pushed_end, producer.start);
            let len = expressions.len();
            expressions.swap(len - 2, len - 1);
            changed = true;
            continue;
        }

        let node = sequence.push(inst);
        update_expressions(&mut expressions, &sequence, node);
    }
    sequence.finish_into(&mut source);
    *instructions = source;
    changed
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
            || !inst.as_evm_opcode().is_some_and(op::is_unaffected_by_preceding_push)
            || effect.inputs != 1
            || effect.outputs != 1
        {
            return None;
        }
        node = sequence.previous(node)?;
    }
}

#[derive(Clone, Copy)]
struct Expression {
    start: usize,
    immediate_recipe: bool,
}

fn update_expressions(
    expressions: &mut Vec<Expression>,
    sequence: &InstructionSequence,
    node: usize,
) {
    let inst = sequence.instruction(node);
    let effect = if let Some(effect) = inst.effective_stack_effect()
        && inst.has_canonical_stack_effect()
        && !inst.is_physical_stack_op()
        && inst.as_evm_opcode().is_none_or(op::is_unaffected_by_preceding_push)
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
        expressions.push(Expression { start: node, immediate_recipe: inst.is_encoded_push() });
        return;
    }
    let start = expressions.len() - inputs;
    let immediate_recipe =
        inst.as_evm_opcode().is_some_and(|opcode| matches!(opcode, op::NOT | op::SHL | op::SHR))
            && expressions[start..].iter().all(|expression| expression.immediate_recipe);
    let first = expressions[start].start;
    expressions.truncate(start);
    expressions.push(Expression { start: first, immediate_recipe });
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
