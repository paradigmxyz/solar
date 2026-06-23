//! Stack scheduling over EVM IR.
//!
//! This pass is intentionally target-level and untyped. It treats EVM IR values
//! as virtual stack words, materializes instruction operands with physical
//! `dupN`/`swapN`/`pop` operations, and leaves the result in a form closer to
//! final EVM assembly while still retaining block structure.
//!
//! Scheduling is atomic per block: a block is either fully scheduled or left
//! exactly as it was. Whenever an operand cannot be placed — a value that is not
//! live on the model stack, or a slot deeper than `DUP16`/`SWAP16` can reach —
//! the block is restored verbatim, so the pass never emits a half-scheduled,
//! semantically broken block. Values used by more than one instruction (or the
//! terminator) are duplicated so a copy survives every use, and the model stack
//! is seeded from the block's [`entry_stack`](super::ir::EvmIrBlock::entry_stack)
//! so blocks that consume predecessor values can be scheduled too.

use super::ir::{
    EvmIrBlockId, EvmIrInstruction, EvmIrInstructionKind, EvmIrModule, EvmIrOperand,
    EvmIrStackEffect, EvmIrStackOp, EvmIrTerminatorKind, EvmIrValueId,
    default_instruction_stack_effect, is_encoded_push_instruction,
};
use solar_data_structures::map::FxHashMap;

/// Maximum stack depth reachable by `DUPn`/`SWAPn`.
const MAX_STACK_REACH: usize = 16;

pub(super) fn schedule_stack_ops(module: &mut EvmIrModule) -> bool {
    EvmIrStackScheduler::new(module).run()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ScheduledStackItem {
    Value(EvmIrValueId),
    Anonymous(u32),
}

struct EvmIrStackScheduler<'a> {
    module: &'a mut EvmIrModule,
    next_anonymous: u32,
    changed: bool,
}

impl<'a> EvmIrStackScheduler<'a> {
    fn new(module: &'a mut EvmIrModule) -> Self {
        Self { module, next_anonymous: 0, changed: false }
    }

    fn run(mut self) -> bool {
        for block_id in self.module.blocks.indices().collect::<Vec<_>>() {
            self.schedule_block(block_id);
        }
        self.changed
    }

    fn schedule_block(&mut self, block_id: EvmIrBlockId) {
        // Seed the model stack from the block's incoming signature so blocks that
        // consume predecessor values can be scheduled instead of bailing.
        let mut stack: Vec<ScheduledStackItem> = self.module.blocks[block_id]
            .entry_stack
            .iter()
            .copied()
            .map(ScheduledStackItem::Value)
            .collect();

        // How many times each value is still used (operands of instructions and
        // the terminator) so multi-use values are duplicated and preserved.
        let mut remaining = self.collect_value_uses(block_id);

        // Snapshot for verbatim restore; take the live instructions to iterate by
        // value. The block is rewritten only if every instruction schedules.
        let original = self.module.blocks[block_id].instructions.clone();
        let working = std::mem::take(&mut self.module.blocks[block_id].instructions);

        let mut out = Vec::with_capacity(working.len());
        let mut changed = false;
        for inst in working {
            if !self.schedule_instruction(inst, &mut stack, &mut out, &mut remaining, &mut changed)
            {
                self.module.blocks[block_id].instructions = original;
                return;
            }
        }

        self.module.blocks[block_id].instructions = out;
        if changed {
            self.changed = true;
        }
    }

    /// Counts remaining value uses across the block's instructions and terminator.
    fn collect_value_uses(&self, block_id: EvmIrBlockId) -> FxHashMap<EvmIrValueId, usize> {
        let block = &self.module.blocks[block_id];
        let mut uses = FxHashMap::<EvmIrValueId, usize>::default();
        let mut count = |operand: &EvmIrOperand| {
            if let EvmIrOperand::Value(value) = operand {
                *uses.entry(*value).or_default() += 1;
            }
        };
        for inst in &block.instructions {
            for operand in &inst.operands {
                count(operand);
            }
        }
        if let Some(terminator) = &block.terminator {
            visit_terminator_value_operands(&terminator.kind, &mut count);
        }
        uses
    }

    /// Schedules one instruction onto `out`/`stack`. Returns `false` when it
    /// cannot be placed; the caller then restores the block, discarding any
    /// partial work left in `out`/`stack`.
    fn schedule_instruction(
        &mut self,
        mut inst: EvmIrInstruction,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<EvmIrInstruction>,
        remaining: &mut FxHashMap<EvmIrValueId, usize>,
        changed: &mut bool,
    ) -> bool {
        // Already-physical stack operations are replayed onto the model.
        if let EvmIrInstructionKind::Stack(op) = inst.kind {
            if !Self::apply_stack_op(stack, op) {
                return false;
            }
            out.push(inst);
            return true;
        }

        let effect = instruction_stack_effect(&inst);
        let schedule_operands =
            !inst.operands.is_empty() && !instruction_keeps_encoded_operands(&inst);

        if schedule_operands {
            let Some(target) = self.materialize_operand_stack(&inst.operands, stack, out, changed)
            else {
                return false;
            };
            if !self.arrange_stack_for(stack, &target, remaining, out, changed) {
                return false;
            }
            // Consume the operands now arranged on top, updating use counts so the
            // survivors stay live for later uses.
            if stack.len() < target.len() {
                return false;
            }
            for item in &target {
                if let ScheduledStackItem::Value(value) = item
                    && let Some(count) = remaining.get_mut(value)
                {
                    *count = count.saturating_sub(1);
                }
            }
            stack.drain(0..target.len());
            inst.operands.clear();
            *changed = true;
        } else if (stack.len() as u64) < u64::from(effect.inputs) {
            return false;
        } else {
            for _ in 0..effect.inputs {
                stack.remove(0);
            }
        }

        for index in 0..effect.outputs {
            let item = if index == 0 {
                inst.result.map(ScheduledStackItem::Value).unwrap_or_else(|| self.fresh_anonymous())
            } else {
                self.fresh_anonymous()
            };
            stack.insert(0, item);
        }
        out.push(inst);
        true
    }

    /// Builds the desired top-of-stack arrangement for an instruction's operands,
    /// pushing immediates inline. Returns `None` if a value operand is not live.
    fn materialize_operand_stack(
        &mut self,
        operands: &[EvmIrOperand],
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<EvmIrInstruction>,
        changed: &mut bool,
    ) -> Option<Vec<ScheduledStackItem>> {
        let mut target = Vec::with_capacity(operands.len());
        for operand in operands {
            match operand {
                EvmIrOperand::Value(value) => target.push(ScheduledStackItem::Value(*value)),
                EvmIrOperand::Immediate(_) | EvmIrOperand::Block(_) | EvmIrOperand::Symbol(_) => {
                    let item = self.fresh_anonymous();
                    let mut push = EvmIrInstruction::new("push", vec![operand.clone()]);
                    push.metadata.stack = Some(EvmIrStackEffect::new(0, 1));
                    out.push(push);
                    stack.insert(0, item);
                    target.push(item);
                    *changed = true;
                }
            }
        }
        if target.iter().all(|item| stack.contains(item)) { Some(target) } else { None }
    }

    /// Rearranges the stack so its top matches `target`, duplicating values that
    /// are needed again later.
    fn arrange_stack_for(
        &mut self,
        stack: &mut Vec<ScheduledStackItem>,
        target: &[ScheduledStackItem],
        remaining: &FxHashMap<EvmIrValueId, usize>,
        out: &mut Vec<EvmIrInstruction>,
        changed: &mut bool,
    ) -> bool {
        if !self.ensure_multiplicities(stack, target, remaining, out, changed) {
            return false;
        }
        if stack.starts_with(target) {
            return true;
        }

        for (target_depth, target_item) in target.iter().copied().enumerate() {
            if stack.get(target_depth).copied() == Some(target_item) {
                continue;
            }

            let Some(source_depth) = stack
                .iter()
                .enumerate()
                .skip(target_depth)
                .find_map(|(depth, item)| (*item == target_item).then_some(depth))
            else {
                return false;
            };
            if source_depth >= MAX_STACK_REACH {
                return false;
            }

            if target_depth == 0 {
                if !self.emit_stack_op(EvmIrStackOp::Swap(source_depth as u8), stack, out, changed)
                {
                    return false;
                }
            } else if !self.shuffle_item_to_depth(target_depth, target_item, stack, out, changed) {
                return false;
            }
        }
        true
    }

    fn shuffle_item_to_depth(
        &mut self,
        target_depth: usize,
        target_item: ScheduledStackItem,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<EvmIrInstruction>,
        changed: &mut bool,
    ) -> bool {
        if target_depth >= MAX_STACK_REACH {
            return false;
        }
        if !self.emit_stack_op(EvmIrStackOp::Swap(target_depth as u8), stack, out, changed) {
            return false;
        }
        let Some(new_depth) = stack.iter().position(|item| *item == target_item) else {
            return false;
        };
        if new_depth == 0 || new_depth >= MAX_STACK_REACH {
            return false;
        }
        if !self.emit_stack_op(EvmIrStackOp::Swap(new_depth as u8), stack, out, changed) {
            return false;
        }
        self.emit_stack_op(EvmIrStackOp::Swap(target_depth as u8), stack, out, changed)
    }

    /// Duplicates values until the stack holds a copy for every remaining use,
    /// so later uses (and the terminator) still find them after consumption.
    fn ensure_multiplicities(
        &mut self,
        stack: &mut Vec<ScheduledStackItem>,
        target: &[ScheduledStackItem],
        remaining: &FxHashMap<EvmIrValueId, usize>,
        out: &mut Vec<EvmIrInstruction>,
        changed: &mut bool,
    ) -> bool {
        let mut target_counts = FxHashMap::<ScheduledStackItem, usize>::default();
        for &item in target {
            *target_counts.entry(item).or_default() += 1;
        }

        for (&item, &target_count) in &target_counts {
            // A value needs a copy for every remaining use (this instruction plus
            // any later instruction or the terminator). Anonymous push results are
            // single-use, so only the target multiplicity is required.
            let needed = match item {
                ScheduledStackItem::Value(value) => {
                    remaining.get(&value).copied().unwrap_or(target_count).max(target_count)
                }
                ScheduledStackItem::Anonymous(_) => target_count,
            };
            let mut have = stack.iter().filter(|&&stack_item| stack_item == item).count();
            while have < needed {
                let Some(depth) = stack.iter().position(|stack_item| *stack_item == item) else {
                    return false;
                };
                if depth >= MAX_STACK_REACH {
                    return false;
                }
                if !self.emit_stack_op(EvmIrStackOp::Dup((depth + 1) as u8), stack, out, changed) {
                    return false;
                }
                have += 1;
            }
        }
        true
    }

    fn emit_stack_op(
        &mut self,
        op: EvmIrStackOp,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<EvmIrInstruction>,
        changed: &mut bool,
    ) -> bool {
        if !Self::apply_stack_op(stack, op) {
            return false;
        }
        out.push(EvmIrInstruction::stack_op(op));
        *changed = true;
        true
    }

    fn apply_stack_op(stack: &mut Vec<ScheduledStackItem>, op: EvmIrStackOp) -> bool {
        match op {
            EvmIrStackOp::Dup(n) => {
                let depth = usize::from(n - 1);
                let Some(value) = stack.get(depth).copied() else {
                    return false;
                };
                stack.insert(0, value);
            }
            EvmIrStackOp::Swap(n) => {
                let depth = usize::from(n);
                if depth >= stack.len() {
                    return false;
                }
                stack.swap(0, depth);
            }
            EvmIrStackOp::Pop => {
                if stack.is_empty() {
                    return false;
                }
                stack.remove(0);
            }
        }
        true
    }

    fn fresh_anonymous(&mut self) -> ScheduledStackItem {
        let id = self.next_anonymous;
        self.next_anonymous += 1;
        ScheduledStackItem::Anonymous(id)
    }
}

fn instruction_stack_effect(inst: &EvmIrInstruction) -> EvmIrStackEffect {
    inst.metadata.stack.unwrap_or_else(|| default_instruction_stack_effect(inst))
}

fn instruction_keeps_encoded_operands(inst: &EvmIrInstruction) -> bool {
    is_encoded_push_instruction(inst)
}

/// Invokes `visit` for each value operand referenced by a terminator.
fn visit_terminator_value_operands(
    kind: &EvmIrTerminatorKind,
    visit: &mut impl FnMut(&EvmIrOperand),
) {
    match kind {
        EvmIrTerminatorKind::Branch { condition, .. } => visit(condition),
        EvmIrTerminatorKind::Switch { value, cases, .. } => {
            visit(value);
            for (case_value, _) in cases {
                visit(case_value);
            }
        }
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => {
            visit(offset);
            visit(size);
        }
        EvmIrTerminatorKind::SelfDestruct { recipient } => visit(recipient),
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
}
