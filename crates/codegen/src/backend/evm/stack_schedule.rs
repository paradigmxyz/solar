//! Stack scheduling over EVM IR.
//!
//! This pass is intentionally target-level and untyped. It treats EVM IR values
//! as virtual stack words, materializes instruction operands with physical
//! `dupN`/`swapN`/`pop` operations, and leaves the result in a form closer to
//! final EVM assembly while still retaining block structure.
//!
//! After the instructions, the block's terminator value operands are arranged
//! onto the top of the model stack in the order EVM expects (operand 0 on top):
//! `return`/`revert` offset over size, `br`/`switch` discriminant on top,
//! `selfdestruct` recipient on top. The terminator keeps those operands as
//! positioned markers; the verifier then applies the terminator stack effect to
//! the abstract exit stack, so consumed branch/switch operands cannot be claimed
//! as successor inputs.
//!
//! Scheduling is atomic per block: a block is either fully scheduled or left
//! exactly as it was. Whenever an operand cannot be placed — a value that is not
//! live on the model stack, or a slot deeper than `DUP16`/`SWAP16` can reach —
//! the block is restored verbatim (including when the terminator operands cannot
//! be arranged), so the pass never emits a half-scheduled, semantically broken
//! block. Values used by more than one instruction (or the terminator) are
//! duplicated so a copy survives every use.
//!
//! # Spilling values out of reach
//!
//! A value buried deeper than `DUP16`/`SWAP16` can reach cannot be arranged by
//! `dup`/`swap` alone. Rather than always bailing, the scheduler can **spill**
//! the deepest reachable live value to a fixed memory slot (an `mstore`),
//! shrinking the model stack so the buried value rises into reach, and **reload**
//! it (an `mload`) just before its next use. Slot offsets reuse
//! [`SpillSlot`]'s memory layout (a high, conflict-free region). The spilled word
//! is removed from the model stack; the reload pushes a fresh anonymous word back
//! that the model treats as the spilled value again.
//!
//! Spilling is still fully atomic: if even spilling cannot bring every operand
//! into reach the block is restored verbatim, so the safe-bail guarantee holds.
//! To keep the reloaded (anonymous) word from corrupting a value identity that a
//! successor relies on, spilling is only enabled in blocks whose terminator does
//! not hand a named value-word stack to a successor — i.e. anything other than a
//! linear `jump`. Those are exactly the blocks whose flowing exit
//! stack carries no named words across the edge, so replacing a spilled value
//! with an anonymous reloaded word never breaks cross-block identity.
//!
//! # Inferring entry stacks from the CFG
//!
//! A block's incoming stack is not trusted from the manual `(in ...)` signature:
//! an inconsistent signature would silently miscompile. Instead the scheduler
//! *infers* each block's entry stack from the control-flow graph. It builds
//! successors from each terminator (and the corresponding predecessor map),
//! walks the blocks in reverse-postorder from the entry block so every
//! predecessor is scheduled before its successors, and seeds each block's model
//! stack from the **exit stack** its predecessor leaves behind.
//!
//! The entry block starts from an empty stack. A non-entry block's entry stack
//! is the value words flowing out of its predecessors. At a merge point all
//! predecessor exit stacks must *agree*; if they disagree the block is left
//! unscheduled (a safe bail) rather than guessed. A back-edge whose predecessor
//! has not been scheduled yet (a loop) also bails. When a block carries an
//! explicit `(in ...)` signature it is treated as a claim to verify: if it does
//! not match the inferred entry the block bails instead of trusting it.
//!
//! Only `jump` edges propagate a non-empty entry stack, because
//! the verifier (the oracle) keeps a `br`/`switch` discriminant live on top of
//! the predecessor's exit stack and requires a successor's declared entry to be
//! a *prefix* of that exit. A successor of a conditional terminator therefore
//! inherits the empty prefix; only the linear `jump` case can hand
//! down the words below.

use super::{ir, opcode as op, stack::SpillSlot};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Maximum stack depth reachable by `DUP<N>`/`SWAP<N>`.
const MAX_STACK_REACH: usize = 16;

pub(super) fn schedule_stack_ops(module: &mut ir::Module) -> bool {
    StackScheduler::new(module).run()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ScheduledStackItem {
    Value(ir::ValueId),
    Anonymous(u32),
}

struct StackScheduler<'a> {
    module: &'a mut ir::Module,
    next_anonymous: u32,
    changed: bool,
    /// Exit stack each successfully scheduled block leaves behind, keyed by
    /// block id. A block that bailed has no entry here, so any successor that
    /// would inherit from it bails too. The entry value-word stack a block hands
    /// to a `jump` successor is the prefix of this exit stack that
    /// consists entirely of known SSA value words.
    exit_stacks: FxHashMap<ir::BlockId, Vec<ScheduledStackItem>>,
    /// Per-block spill state, reset for each block (see [`SpillState`]).
    spills: SpillState,
}

/// Tracks values spilled to memory while scheduling a single block.
///
/// A spill removes the deepest reachable live value from the model stack and
/// records its memory slot here. The value can then be reloaded with an `mload`
/// before its next use. State is reset at the start of every block; spilling is
/// always local to the block being scheduled.
#[derive(Default)]
struct SpillState {
    /// Whether spilling is permitted for the current block. Disabled for blocks
    /// whose terminator propagates a named value-word stack to a successor.
    enabled: bool,
    /// Memory slot reserved for each currently-spilled value.
    slots: FxHashMap<ir::ValueId, SpillSlot>,
    /// Next free spill slot offset (in 32-byte words).
    next_offset: u32,
}

impl SpillState {
    /// Resets the spill state for a new block.
    fn reset(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.slots.clear();
        self.next_offset = 0;
    }

    /// Reserves (or reuses) a memory slot for `value` and marks it spilled.
    fn allocate(&mut self, value: ir::ValueId) -> SpillSlot {
        if let Some(&slot) = self.slots.get(&value) {
            return slot;
        }
        let slot = SpillSlot { offset: self.next_offset };
        self.next_offset += 1;
        self.slots.insert(value, slot);
        slot
    }

    /// Returns the slot a spilled value can be reloaded from, if any.
    fn slot(&self, value: ir::ValueId) -> Option<SpillSlot> {
        self.slots.get(&value).copied()
    }

    /// Clears the spilled marker once a value has been reloaded onto the stack.
    fn mark_reloaded(&mut self, value: ir::ValueId) {
        self.slots.remove(&value);
    }

    /// Whether `value` is currently held in memory rather than on the stack.
    fn is_spilled(&self, value: ir::ValueId) -> bool {
        self.slots.contains_key(&value)
    }
}

impl<'a> StackScheduler<'a> {
    fn new(module: &'a mut ir::Module) -> Self {
        Self {
            module,
            next_anonymous: 0,
            changed: false,
            exit_stacks: FxHashMap::default(),
            spills: SpillState::default(),
        }
    }

    fn run(mut self) -> bool {
        // Process blocks in reverse-postorder from the entry so every
        // predecessor is scheduled before its successors. Unreachable blocks are
        // appended afterwards and scheduled with no inferred predecessor (they
        // fall back to a clean stack, matching the entry convention).
        let order = self.reverse_postorder();
        for block_id in order {
            self.schedule_block(block_id);
        }
        self.changed
    }

    /// Reverse-postorder of the blocks reachable from the entry, followed by any
    /// unreachable blocks in layout order. Predecessors precede successors except
    /// across back-edges (loops), which the scheduler detects and bails on.
    fn reverse_postorder(&self) -> Vec<ir::BlockId> {
        let mut postorder = Vec::with_capacity(self.module.blocks.len());
        let mut visited = DenseBitSet::new_empty(self.module.blocks.len());
        if let Some(entry) = self.module.entry_block {
            // Iterative DFS recording postorder. Each frame remembers the
            // successors still to descend into.
            let mut work: Vec<(ir::BlockId, Vec<ir::BlockId>)> = Vec::new();
            visited.insert(entry);
            work.push((entry, block_successors(&self.module.blocks[entry])));
            while let Some((block_id, succs)) = work.last_mut() {
                if let Some(succ) = succs.pop() {
                    if visited.insert(succ) {
                        let succs = block_successors(&self.module.blocks[succ]);
                        work.push((succ, succs));
                    }
                } else {
                    postorder.push(*block_id);
                    work.pop();
                }
            }
        }
        postorder.reverse();
        // Append unreachable blocks so they are still visited (and left verbatim
        // unless they happen to schedule from a clean stack).
        for block_id in self.module.blocks.indices() {
            if !visited.contains(block_id) {
                postorder.push(block_id);
            }
        }
        postorder
    }

    fn schedule_block(&mut self, block_id: ir::BlockId) {
        // Infer the block's entry stack from the CFG: the value words its
        // predecessors leave behind. `None` means the entry could not be inferred
        // (a disagreeing merge or an unscheduled/back-edge predecessor), so the
        // block is left verbatim.
        let Some(entry_stack) = self.infer_entry_stack(block_id) else {
            return;
        };

        // Reset the per-block spill state. Spilling is only safe in blocks whose
        // terminator does not hand a named value-word stack down to a successor,
        // because a reloaded value becomes an anonymous word that would break a
        // successor's declared incoming identities.
        self.spills.reset(self.block_allows_spilling(block_id));

        // Seed the model stack from the inferred entry so blocks that consume
        // predecessor values can be scheduled instead of bailing.
        let mut stack: Vec<ScheduledStackItem> =
            entry_stack.iter().copied().map(ScheduledStackItem::Value).collect();

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

        // Arrange the terminator's value operands onto the top of the model
        // stack (operand 0 on top). This shares the same materialize/arrange
        // machinery and stays atomic with instruction scheduling: if the
        // terminator cannot be arranged, the whole block is restored verbatim.
        if !self.schedule_terminator(block_id, &mut stack, &mut out, &remaining, &mut changed) {
            self.module.blocks[block_id].instructions = original;
            return;
        }

        // The block scheduled. Record the exit stack the terminator leaves so
        // successors can inherit it, and write back the inferred entry signature.
        self.module.blocks[block_id].instructions = out;
        self.module.blocks[block_id].entry_stack = entry_stack;
        let exit = self.terminator_exit_stack(block_id, stack);
        self.exit_stacks.insert(block_id, exit);
        if changed {
            self.changed = true;
        }
    }

    /// Infers the entry value-word stack for `block_id` from its predecessors'
    /// recorded exit stacks. Returns `None` to signal a safe bail: a merge point
    /// whose predecessors disagree, or a predecessor that has not been scheduled
    /// yet (a back-edge/loop or a predecessor that itself bailed). The entry
    /// block always infers an empty stack.
    ///
    /// An explicit `(in ...)` signature is treated as a claim to verify, not a
    /// value to trust. It is accepted only when it is a *prefix* of the inferred
    /// incoming stack — the same relation the verifier enforces on a CFG edge,
    /// where a successor names just the top words it consumes and leaves the rest
    /// as an inherited floor. A signature that is not such a prefix is
    /// inconsistent with the CFG, so the block bails. When a (valid) signature is
    /// present it is used verbatim as the seed; otherwise the full inferred
    /// incoming stack is used.
    fn infer_entry_stack(&self, block_id: ir::BlockId) -> Option<Vec<ir::ValueId>> {
        let declared = &self.module.blocks[block_id].entry_stack;

        let inferred = if self.module.entry_block == Some(block_id) {
            Vec::new()
        } else {
            let preds = self.predecessors(block_id);
            // A reachable non-entry block must have at least one predecessor; if
            // it has none it is unreachable, so fall back to a clean stack.
            if preds.is_empty() {
                Vec::new()
            } else {
                let mut merged: Option<Vec<ir::ValueId>> = None;
                for pred in preds {
                    // A predecessor not yet scheduled (back-edge/loop) or one that
                    // bailed has no recorded exit: bail this block too.
                    let exit = self.exit_stacks.get(&pred)?;
                    let flow = edge_entry_stack(&self.module.blocks[pred], exit);
                    match &merged {
                        // Merge points must agree on the incoming stack.
                        Some(existing) if *existing != flow => return None,
                        Some(_) => {}
                        None => merged = Some(flow),
                    }
                }
                merged.unwrap_or_default()
            }
        };

        if declared.is_empty() {
            return Some(inferred);
        }
        // A declared signature must be a prefix of what the CFG actually
        // delivers; otherwise it is inconsistent and the block bails. The
        // declared prefix is kept as the seed so the block only names the words
        // it consumes.
        if inferred.starts_with(declared) { Some(declared.clone()) } else { None }
    }

    /// Predecessors of `block_id`: every block whose terminator targets it.
    fn predecessors(&self, block_id: ir::BlockId) -> Vec<ir::BlockId> {
        let mut preds = Vec::new();
        for (id, block) in self.module.blocks.iter_enumerated() {
            if block_successors(block).contains(&block_id) {
                preds.push(id);
            }
        }
        preds
    }

    /// The exit stack the scheduled terminator leaves behind. `br`/`switch`
    /// consume their discriminant (the top word) at runtime, so the flowing exit
    /// drops it; every other terminator leaves the model stack as is.
    fn terminator_exit_stack(
        &self,
        block_id: ir::BlockId,
        mut stack: Vec<ScheduledStackItem>,
    ) -> Vec<ScheduledStackItem> {
        if let Some(term) = &self.module.blocks[block_id].terminator
            && matches!(
                term.kind,
                ir::TerminatorKind::Branch { .. } | ir::TerminatorKind::Switch { .. }
            )
            && !stack.is_empty()
        {
            stack.remove(0);
        }
        stack
    }

    /// Arranges the terminator's value operands onto the top of the model stack
    /// in operand order (operand 0 on top, operand 1 below it, etc.) by emitting
    /// the same physical `dup`/`swap` ops used for instructions. Returns `false`
    /// when the operands cannot be placed; the caller then restores the block.
    ///
    /// Representation: the terminator keeps its value operands as positioned
    /// markers; the verifier checks that those words are live and then applies
    /// the terminator stack effect to the abstract exit stack. Leaving the
    /// operands in the terminator keeps the scheduled text readable while the
    /// verifier still models the runtime pop.
    fn schedule_terminator(
        &mut self,
        block_id: ir::BlockId,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        remaining: &FxHashMap<ir::ValueId, usize>,
        changed: &mut bool,
    ) -> bool {
        let Some(terminator) = self.module.blocks[block_id].terminator.as_ref() else {
            return true;
        };
        let operands = terminator_arrange_operands(&terminator.kind);
        if operands.is_empty() {
            return true;
        }

        let target: Vec<ScheduledStackItem> =
            operands.into_iter().map(ScheduledStackItem::Value).collect();
        // Every targeted operand must already be live on the model stack.
        if !target.iter().all(|item| stack.contains(item)) {
            return false;
        }
        // Arrange (and duplicate as needed) without draining: the terminator
        // consumes these words at runtime, but the verifier models the
        // arranged operands as still present on the stack.
        self.arrange_stack_for(stack, &target, remaining, out, changed)
    }

    /// Counts remaining value uses across the block's instructions and terminator.
    fn collect_value_uses(&self, block_id: ir::BlockId) -> FxHashMap<ir::ValueId, usize> {
        let block = &self.module.blocks[block_id];
        let mut uses = FxHashMap::<ir::ValueId, usize>::default();
        let mut count = |operand: &ir::Operand| {
            if let ir::Operand::Value(value) = operand {
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
        mut inst: ir::Instruction,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        remaining: &mut FxHashMap<ir::ValueId, usize>,
        changed: &mut bool,
    ) -> bool {
        // Already-physical stack operations are replayed onto the model.
        if inst.is_physical_stack_op() {
            if !Self::apply_stack_op(stack, inst.opcode) {
                return false;
            }
            out.push(inst);
            return true;
        }

        let effect = instruction_stack_effect(&inst);
        let schedule_operands =
            !inst.operands.is_empty() && !instruction_keeps_encoded_operands(&inst);

        if schedule_operands {
            // Reload any spilled value operands and, if a needed value is buried
            // deeper than DUP/SWAP can reach, spill the deepest reachable live
            // value to lift it into range. A failure here is a clean bail.
            if !self.prepare_operands_reachable(&inst.operands, stack, out, changed) {
                return false;
            }
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
            inst.metadata.stack.get_or_insert(effect);
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

    /// Whether the current block may spill values to memory. Spilling reloads a
    /// value as a fresh anonymous word, which would corrupt a successor's
    /// declared incoming value identities; it is therefore disabled for blocks
    /// whose terminator hands a named value-word stack to a successor (a linear
    /// `jump`). Every other terminator leaves no named exit words
    /// flowing across an edge, so reloading as anonymous is harmless.
    fn block_allows_spilling(&self, block_id: ir::BlockId) -> bool {
        !matches!(
            self.module.blocks[block_id].terminator.as_ref().map(|term| &term.kind),
            Some(ir::TerminatorKind::Jump(_))
        )
    }

    /// Makes every value operand of an instruction reachable before it is
    /// arranged: reloads spilled operands from memory, then spills the deepest
    /// reachable live value while any value operand still sits below
    /// `DUP16`/`SWAP16`. Returns `false` (a clean bail) if reach cannot be
    /// achieved — leaving any emitted `mstore`/`mload` to be discarded with the
    /// rest of the partial schedule when the caller restores the block.
    fn prepare_operands_reachable(
        &mut self,
        operands: &[ir::Operand],
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> bool {
        // Reload any operand currently held in memory.
        for operand in operands {
            if let ir::Operand::Value(value) = operand
                && self.spills.is_spilled(*value)
                && !self.reload_value(*value, stack, out, changed)
            {
                return false;
            }
        }

        // The value operands that must end up reachable on the stack.
        let needed: Vec<ir::ValueId> = operands
            .iter()
            .filter_map(|operand| match operand {
                ir::Operand::Value(value) => Some(*value),
                _ => None,
            })
            .collect();

        // While any needed value is buried out of reach, spill the deepest
        // reachable value that is not itself needed by this instruction. Each
        // spill removes one word and lifts everything below it up by one, so a
        // buried value eventually rises into range.
        loop {
            let deepest_needed = needed
                .iter()
                .filter_map(|value| {
                    stack.iter().position(|item| *item == ScheduledStackItem::Value(*value))
                })
                .max();
            let Some(depth) = deepest_needed else { return true };
            if depth < MAX_STACK_REACH {
                return true;
            }
            if !self.spills.enabled {
                return false;
            }

            // Pick a spill victim: the deepest reachable value-word that this
            // instruction does not itself need. Anonymous words cannot be spilled
            // (they have no SSA identity to reload by), so they are skipped.
            let victim = (0..MAX_STACK_REACH.min(stack.len())).rev().find_map(|d| match stack[d] {
                ScheduledStackItem::Value(value) if !needed.contains(&value) => Some((d, value)),
                _ => None,
            });
            let Some((victim_depth, victim_value)) = victim else {
                return false;
            };
            if !self.spill_value(victim_depth, victim_value, stack, out, changed) {
                return false;
            }
        }
    }

    /// Spills the value at `victim_depth` to its memory slot, removing it from
    /// the model stack. Emits `swapN` (to bring it to the top), `push <offset>`,
    /// then a scheduled `mstore` (stack effect `2->0`). The value can later be
    /// reloaded on demand from its slot. Only a value with a single live copy is
    /// spilled, so no stale duplicate is left behind.
    fn spill_value(
        &mut self,
        victim_depth: usize,
        victim_value: ir::ValueId,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> bool {
        if victim_depth >= MAX_STACK_REACH {
            return false;
        }
        // Spilling a value that is also still on the stack elsewhere would leave
        // a stale duplicate; only spill a value with a single live copy.
        let copies =
            stack.iter().filter(|item| **item == ScheduledStackItem::Value(victim_value)).count();
        if copies != 1 {
            return false;
        }
        let slot = self.spills.allocate(victim_value);

        // Bring the victim to the top of the stack so `mstore` can pop it.
        if victim_depth != 0
            && !self.emit_stack_op(op::swap(victim_depth as u8), stack, out, changed)
        {
            return false;
        }
        debug_assert_eq!(stack.first().copied(), Some(ScheduledStackItem::Value(victim_value)));

        // Push the slot's memory offset, then store: `mstore(offset, value)`.
        self.emit_push_immediate(byte_offset(slot), stack, out, changed);
        // `mstore` pops the offset and the value (2 inputs, no output).
        let mut mstore = ir::Instruction::opcode(op::MSTORE);
        mstore.metadata.stack = Some(ir::StackEffect::new(2, 0));
        out.push(mstore);
        if stack.len() < 2 {
            return false;
        }
        stack.drain(0..2);
        *changed = true;

        // The victim is no longer on the stack. If it still has remaining uses it
        // will be reloaded from its slot before each one; if it is already dead
        // the slot simply goes unread.
        true
    }

    /// Reloads a spilled value back onto the top of the stack by pushing its slot
    /// offset and emitting a scheduled `mload` (stack effect `1->1`). The reloaded
    /// word is modeled as the spilled value again so later uses find it.
    fn reload_value(
        &mut self,
        value: ir::ValueId,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> bool {
        let Some(slot) = self.spills.slot(value) else {
            return false;
        };
        // Push the slot offset, then load: `mload(offset)`.
        self.emit_push_immediate(byte_offset(slot), stack, out, changed);
        let mut mload = ir::Instruction::opcode(op::MLOAD);
        mload.metadata.stack = Some(ir::StackEffect::new(1, 1));
        out.push(mload);
        if stack.is_empty() {
            return false;
        }
        // `mload` pops the offset and pushes the loaded word; model that word as
        // the reloaded value so later arrangement finds it on top.
        stack[0] = ScheduledStackItem::Value(value);
        self.spills.mark_reloaded(value);
        *changed = true;
        true
    }

    /// Emits an encoded `push <immediate>` and tracks a fresh anonymous word.
    fn emit_push_immediate(
        &mut self,
        immediate: U256,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) {
        let item = self.fresh_anonymous();
        out.push(ir::Instruction::push(ir::Operand::Immediate(immediate)));
        stack.insert(0, item);
        *changed = true;
    }

    /// Builds the desired top-of-stack arrangement for an instruction's operands,
    /// pushing immediates inline. Returns `None` if a value operand is not live.
    fn materialize_operand_stack(
        &mut self,
        operands: &[ir::Operand],
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> Option<Vec<ScheduledStackItem>> {
        let mut target = Vec::with_capacity(operands.len());
        for operand in operands {
            match operand {
                ir::Operand::Value(value) => target.push(ScheduledStackItem::Value(*value)),
                ir::Operand::Immediate(_) | ir::Operand::Block(_) => {
                    let item = self.fresh_anonymous();
                    out.push(ir::Instruction::push(operand.clone()));
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
        remaining: &FxHashMap<ir::ValueId, usize>,
        out: &mut Vec<ir::Instruction>,
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
                if !self.emit_stack_op(op::swap(source_depth as u8), stack, out, changed) {
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
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> bool {
        if target_depth >= MAX_STACK_REACH {
            return false;
        }
        if !self.emit_stack_op(op::swap(target_depth as u8), stack, out, changed) {
            return false;
        }
        let Some(new_depth) = stack.iter().position(|item| *item == target_item) else {
            return false;
        };
        if new_depth == 0 || new_depth >= MAX_STACK_REACH {
            return false;
        }
        if !self.emit_stack_op(op::swap(new_depth as u8), stack, out, changed) {
            return false;
        }
        self.emit_stack_op(op::swap(target_depth as u8), stack, out, changed)
    }

    /// Duplicates values until the stack holds a copy for every remaining use,
    /// so later uses (and the terminator) still find them after consumption.
    fn ensure_multiplicities(
        &mut self,
        stack: &mut Vec<ScheduledStackItem>,
        target: &[ScheduledStackItem],
        remaining: &FxHashMap<ir::ValueId, usize>,
        out: &mut Vec<ir::Instruction>,
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
                if !self.emit_stack_op(op::dup((depth + 1) as u8), stack, out, changed) {
                    return false;
                }
                have += 1;
            }
        }
        true
    }

    fn emit_stack_op(
        &mut self,
        opcode: u8,
        stack: &mut Vec<ScheduledStackItem>,
        out: &mut Vec<ir::Instruction>,
        changed: &mut bool,
    ) -> bool {
        if !Self::apply_stack_op(stack, opcode) {
            return false;
        }
        out.push(ir::Instruction::opcode(opcode));
        *changed = true;
        true
    }

    fn apply_stack_op(stack: &mut Vec<ScheduledStackItem>, opcode: u8) -> bool {
        match opcode {
            op::DUP1..=op::DUP16 => {
                let depth = usize::from(opcode - op::DUP1);
                let Some(value) = stack.get(depth).copied() else {
                    return false;
                };
                stack.insert(0, value);
            }
            op::SWAP1..=op::SWAP16 => {
                let depth = usize::from(opcode - op::SWAP1 + 1);
                if depth >= stack.len() {
                    return false;
                }
                stack.swap(0, depth);
            }
            op::POP => {
                if stack.is_empty() {
                    return false;
                }
                stack.remove(0);
            }
            _ => return false,
        }
        true
    }

    fn fresh_anonymous(&mut self) -> ScheduledStackItem {
        let id = self.next_anonymous;
        self.next_anonymous += 1;
        ScheduledStackItem::Anonymous(id)
    }
}

/// The memory byte offset of a spill slot, as an EVM immediate word.
fn byte_offset(slot: SpillSlot) -> U256 {
    U256::from(slot.byte_offset())
}

/// The successor blocks a block's terminator can transfer control to.
fn block_successors(block: &ir::Block) -> Vec<ir::BlockId> {
    let mut targets = Vec::new();
    let Some(term) = &block.terminator else {
        return targets;
    };
    match &term.kind {
        ir::TerminatorKind::Jump(target) => {
            targets.push(*target);
        }
        ir::TerminatorKind::Branch { then_block, else_block, .. } => {
            targets.push(*then_block);
            targets.push(*else_block);
        }
        ir::TerminatorKind::Switch { default, cases, .. } => {
            targets.push(*default);
            for (_, target) in cases {
                targets.push(*target);
            }
        }
        ir::TerminatorKind::Return { .. }
        | ir::TerminatorKind::Revert { .. }
        | ir::TerminatorKind::Stop
        | ir::TerminatorKind::Invalid
        | ir::TerminatorKind::SelfDestruct { .. }
        | ir::TerminatorKind::RawOpcode(_) => {}
    }
    targets
}

/// The entry value-word stack a predecessor hands to a successor across one CFG
/// edge, given the predecessor block and its recorded exit stack (top first).
///
/// Only the linear `jump` edge propagates words: the verifier
/// keeps a `br`/`switch` discriminant live on top of the predecessor's exit and
/// requires the successor's declared entry to be a *prefix* of that exit, so a
/// conditional successor can only safely inherit the empty prefix. For a linear
/// edge the inherited entry is the longest prefix of the exit stack made up
/// entirely of known SSA value words; an anonymous `push`/synthesized word (or
/// any deeper word below it) is left as an implicit inherited floor the
/// successor does not name.
fn edge_entry_stack(pred: &ir::Block, exit: &[ScheduledStackItem]) -> Vec<ir::ValueId> {
    let linear = matches!(
        pred.terminator.as_ref().map(|term| &term.kind),
        Some(ir::TerminatorKind::Jump(_))
    );
    if !linear {
        return Vec::new();
    }
    exit.iter()
        .map_while(|item| match item {
            ScheduledStackItem::Value(value) => Some(*value),
            ScheduledStackItem::Anonymous(_) => None,
        })
        .collect()
}

fn instruction_stack_effect(inst: &ir::Instruction) -> ir::StackEffect {
    inst.metadata.stack.unwrap_or_else(|| ir::default_instruction_stack_effect(inst))
}

fn instruction_keeps_encoded_operands(inst: &ir::Instruction) -> bool {
    inst.is_encoded_push()
}

/// The value operands a terminator needs arranged on top of the stack, in the
/// order EVM expects them (result `[0]` ends up on top, `[1]` below it, etc.).
///
/// Switch case immediates stay encoded and are not arranged, so only the
/// discriminant is returned for a `switch`. Operand-less terminators (`jump`,
/// `stop`, `invalid`, raw opcodes) return an empty list.
fn terminator_arrange_operands(kind: &ir::TerminatorKind) -> Vec<ir::ValueId> {
    let mut operands = Vec::new();
    let mut push = |operand: &ir::Operand| {
        if let ir::Operand::Value(value) = operand {
            operands.push(*value);
        }
    };
    match kind {
        ir::TerminatorKind::Branch { condition, .. } => push(condition),
        ir::TerminatorKind::Switch { value, .. } => push(value),
        ir::TerminatorKind::Return { offset, size }
        | ir::TerminatorKind::Revert { offset, size } => {
            push(offset);
            push(size);
        }
        ir::TerminatorKind::SelfDestruct { recipient } => push(recipient),
        ir::TerminatorKind::Jump(_)
        | ir::TerminatorKind::Stop
        | ir::TerminatorKind::Invalid
        | ir::TerminatorKind::RawOpcode(_) => {}
    }
    operands
}

/// Invokes `visit` for each value operand referenced by a terminator.
fn visit_terminator_value_operands(
    kind: &ir::TerminatorKind,
    visit: &mut impl FnMut(&ir::Operand),
) {
    match kind {
        ir::TerminatorKind::Branch { condition, .. } => visit(condition),
        ir::TerminatorKind::Switch { value, cases, .. } => {
            visit(value);
            for (case_value, _) in cases {
                visit(case_value);
            }
        }
        ir::TerminatorKind::Return { offset, size }
        | ir::TerminatorKind::Revert { offset, size } => {
            visit(offset);
            visit(size);
        }
        ir::TerminatorKind::SelfDestruct { recipient } => visit(recipient),
        ir::TerminatorKind::Jump(_)
        | ir::TerminatorKind::Stop
        | ir::TerminatorKind::Invalid
        | ir::TerminatorKind::RawOpcode(_) => {}
    }
}
