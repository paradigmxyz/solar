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
//! The closed-expression sweep also moves pure arithmetic and calldata expressions. These
//! computations neither mutate memory nor observe its size, gas remaining, or the current PC.
//! Their operands are entirely contained in the moved range, so moving them before another
//! closed producer preserves both results while removing the intervening swap.
//! This broader sweep runs only after the final structural sharing pass: moving a calldata
//! read before a hash can otherwise expose a shared tail whose jumps cost more than the swap.
//!
//! The expression tracker accepts only known one-result operations, rejects physical stack
//! instructions and observations such as `PC` or `GAS`, and clears at unknown effects.
//!
//! The first expression sweep is disabled for pre-extended-stack size builds because its
//! interaction with later structural cleanup can increase code size there. The final sweep runs
//! after structural sharing is fixed and enables it safely. The exact mixed-stack rules run in
//! every optimized mode because they preserve the surrounding value layout and cannot increase
//! local cost.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    op::{self, StackOp},
};
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{index::IndexVec, newtype_index};
use solar_sema::Gcx;

pub(super) const REORDER_PUSHES: ReorderPushes =
    ReorderPushes { reorder_legacy_size_expressions: false, reorder_closed_expressions: false };
pub(super) const FINAL_REORDER_PUSHES: ReorderPushes =
    ReorderPushes { reorder_legacy_size_expressions: true, reorder_closed_expressions: false };
pub(super) const REORDER_EXPRESSIONS: ReorderPushes =
    ReorderPushes { reorder_legacy_size_expressions: true, reorder_closed_expressions: true };

pub(super) struct ReorderPushes {
    reorder_legacy_size_expressions: bool,
    reorder_closed_expressions: bool,
}

impl EvmPass for ReorderPushes {
    fn name(&self) -> &'static str {
        if self.reorder_closed_expressions { "reorder-expressions" } else { "reorder-pushes" }
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    fn cache_config(&self) -> u64 {
        u64::from(self.reorder_legacy_size_expressions)
            | (u64::from(self.reorder_closed_expressions) << 1)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let evm_version = gcx.sess.opts.evm_version;
        let reorder_expressions = self.reorder_legacy_size_expressions
            || !gcx.sess.opts.optimization.is_size()
            || gcx.sess.opts.evm_version.has_extended_stack_ops();
        let mut state = ReorderState::default();
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= state.reorder(
                &mut block.instructions,
                evm_version,
                reorder_expressions,
                self.reorder_closed_expressions,
            );
        }
        changed
    }
}

#[derive(Default)]
struct ReorderState {
    sequence: InstructionSequence,
    expressions: Vec<Expression>,
}

impl ReorderState {
    fn reorder(
        &mut self,
        instructions: &mut Vec<Instruction>,
        evm_version: EvmVersion,
        reorder_expressions: bool,
        reorder_closed_expressions: bool,
    ) -> bool {
        if !instructions.iter().any(|inst| inst.as_stack_op() == Some(StackOp::Swap(1))) {
            return false;
        }

        self.sequence.clear();
        std::mem::swap(instructions, &mut self.sequence.instructions);
        self.sequence.reserve(self.sequence.instructions.len());
        self.expressions.clear();
        let mut changed = false;
        for index in 0..self.sequence.instructions.len() {
            let inst = &self.sequence.instructions[index];
            let swap1 = inst.as_stack_op() == Some(StackOp::Swap(1));
            if swap1
                && let Some(pushed) = self.expressions.last()
                && pushed.immediate_recipe
                && let Some(pushed_end) = self.sequence.last
                && let Some((dup_node, rebased)) =
                    rebasable_dup_before(&self.sequence, pushed.start, evm_version)
            {
                self.sequence.replace_stack_op(dup_node, rebased);
                self.sequence.move_range_before(pushed.start, pushed_end, dup_node);
                self.expressions.clear();
                changed = true;
                continue;
            }

            // producer; closed expression; swap1 -> closed expression; producer
            if reorder_expressions
                && swap1
                && let [.., producer, pushed] = self.expressions.as_slice()
                && (pushed.immediate_recipe || (reorder_closed_expressions && pushed.movable))
                && let Some(pushed_end) = self.sequence.last
            {
                let (producer, pushed) = (*producer, *pushed);
                self.sequence.move_range_before(pushed.start, pushed_end, producer.start);
                let len = self.expressions.len();
                self.expressions.swap(len - 2, len - 1);
                changed = true;
                continue;
            }

            let node = self.sequence.push(index as u32);
            update_expressions(&mut self.expressions, &self.sequence, node);
        }
        self.sequence.finish_into(instructions, changed);
        changed
    }
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
    before: NodeId,
    evm_version: EvmVersion,
) -> Option<(NodeId, StackOp)> {
    let mut node = sequence.previous(before)?;
    loop {
        let inst = sequence.instruction(node);
        if let Some(StackOp::Dup(depth)) = inst.as_stack_op() {
            return Some((node, rebase_dup(evm_version, depth)?));
        }
        let effect = inst.effective_stack_effect()?;
        if inst.is_physical_stack_op()
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
    start: NodeId,
    immediate_recipe: bool,
    movable: bool,
}

fn update_expressions(
    expressions: &mut Vec<Expression>,
    sequence: &InstructionSequence,
    node: NodeId,
) {
    let inst = sequence.instruction(node);
    let effect = if let Some(effect) = inst.effective_stack_effect()
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
        expressions.push(Expression {
            start: node,
            immediate_recipe: inst.is_encoded_push(),
            movable: inst.is_encoded_push() || inst.as_evm_opcode() == Some(op::CALLDATASIZE),
        });
        return;
    }
    let start = expressions.len() - inputs;
    let immediate_recipe =
        inst.as_evm_opcode().is_some_and(|opcode| matches!(opcode, op::NOT | op::SHL | op::SHR))
            && expressions[start..].iter().all(|expression| expression.immediate_recipe);
    let first = expressions[start].start;
    let movable = inst
        .as_evm_opcode()
        .is_some_and(|opcode| op::is_pure(opcode) || opcode == op::CALLDATALOAD)
        && expressions[start..].iter().all(|expression| expression.movable);
    expressions.truncate(start);
    expressions.push(Expression { start: first, immediate_recipe, movable });
}

newtype_index! {
    /// A node of an [`InstructionSequence`].
    struct NodeId;
}

struct InstructionNode {
    /// Index of the instruction in the block being reordered.
    instruction: u32,
    previous: Option<NodeId>,
    next: Option<NodeId>,
}

/// A linked order over the block's instructions, which stay in place until it is finished.
#[derive(Default)]
struct InstructionSequence {
    instructions: Vec<Instruction>,
    nodes: IndexVec<NodeId, InstructionNode>,
    first: Option<NodeId>,
    last: Option<NodeId>,
}

impl InstructionSequence {
    fn clear(&mut self) {
        self.instructions.clear();
        self.nodes.clear();
        self.first = None;
        self.last = None;
    }

    fn reserve(&mut self, additional: usize) {
        self.nodes.reserve(additional);
    }

    fn push(&mut self, instruction: u32) -> NodeId {
        let index =
            self.nodes.push(InstructionNode { instruction, previous: self.last, next: None });
        if let Some(last) = self.last {
            self.nodes[last].next = Some(index);
        } else {
            self.first = Some(index);
        }
        self.last = Some(index);
        index
    }

    fn instruction(&self, index: NodeId) -> &Instruction {
        &self.instructions[self.nodes[index].instruction as usize]
    }

    fn previous(&self, index: NodeId) -> Option<NodeId> {
        self.nodes[index].previous
    }

    fn replace_stack_op(&mut self, index: NodeId, stack_op: StackOp) {
        let instruction = &mut self.instructions[self.nodes[index].instruction as usize];
        let metadata = std::mem::take(&mut instruction.metadata);
        let mut replacement = Instruction::stack_op(stack_op);
        replacement.metadata = metadata;
        replacement.metadata.stack = None;
        *instruction = replacement;
    }

    fn move_range_before(&mut self, start: NodeId, end: NodeId, before: NodeId) {
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

    /// Writes the linked order into the empty `instructions`. Without a rewrite, the order is
    /// the original one and every instruction is linked, so the block is returned as it was.
    fn finish_into(&mut self, instructions: &mut Vec<Instruction>, changed: bool) {
        if !changed {
            std::mem::swap(instructions, &mut self.instructions);
            return;
        }
        let mut slots = self.instructions.drain(..).map(Some).collect::<Vec<_>>();
        instructions.reserve(self.nodes.len());
        let mut current = self.first;
        while let Some(index) = current {
            let node = &self.nodes[index];
            instructions.push(slots[node.instruction as usize].take().unwrap());
            current = node.next;
        }
    }
}
