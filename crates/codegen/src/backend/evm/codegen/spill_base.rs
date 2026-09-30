//! Runtime-addressed spill areas for functions that clobber low memory.
//!
//! Spill slots normally live at fixed low addresses. A dynamic-length write from a low base,
//! such as `calldatacopy(0, 0, calldatasize())` in a forwarding proxy or the output of a calldata
//! decompressor, can cover every one of them, and the written buffer stays readable until some
//! later call, log, hash, or return. Neither a spill store nor a reload of an older slot is safe
//! while the buffer may still be read.
//!
//! The first codegen attempt keeps values that cross such a write on the stack. When that does
//! not fit, or a spill would touch low memory after the write, the function is regenerated with
//! a dynamic spill base: one stack word holds the address of its spill area, and every slot is
//! addressed as `base + offset`. The area starts at its ordinary static location. Before every
//! memory write whose range is not a compile-time constant, the function checks whether the write
//! can overlap the area. If it can, the live slots move to
//! `max(round_up(dest + size), msize(), floor)`, where `floor` is above all static compiler
//! memory and every constant-address range of the runtime, and the base word is updated in place.
//! Constant-address writes stay below `floor`, so they cannot reach a moved area. An internal
//! function's base addresses its frame from the first argument, so its frame arguments move with
//! its spills; its return words and address-taken locals keep their fixed addresses.
//!
//! The base word is a stack-only value: liveness pins it through every block, the resident
//! layout carries it across edges, and internal calls keep it below their return address. The
//! emitter swaps it back near the top of the stack before each instruction. When scheduling
//! buries it beyond `DUP` reach within one instruction, the words above it are parked at
//! `msize()`, where memory holds nothing, and restored and cleared afterwards.
//!
//! Callees allocate from the free-memory pointer, which can point below a moved area. Before an
//! internal call, the pointer is raised past the area when a dataflow proves that no earlier
//! write can have left something else in the `0x40` word: that word may hold part of a buffer,
//! and writing it would corrupt the buffer.
//!
//! NOTE: A callee can still write a moved area when the pointer word may hold buffer data, or
//! when it writes memory without allocating. A recursive function's frame pointer lives at a
//! fixed word that no move can protect; a clobber that may cover it is reported as an error.

use super::{
    ArgIdx, BlockId, CfgInfo, DenseBitSet, DynamicSpillBase, EvmCodegen, EvmMemoryLayout, Function,
    FunctionId, FxHashMap, FxHashSet, IndexVec, InstId, InstKind, Liveness, MirType, SpillSlot,
    StackEffect, StackModel, StackOp, StackPush, Terminator, U256, Value, ValueId,
    cross_block_values, op,
};
use crate::mir::{
    Callee, Instruction, MemoryRegion, Module,
    analysis::{AliasAnalysis, MemoryBase},
};

/// Deepest position the spill base may occupy before an instruction emits. An instruction pushes
/// at most seven operands, so a spill reload for its last operand still reaches the base.
const SPILL_BASE_MAX_DEPTH: usize = 4;

/// Size operand of a memory write.
#[derive(Clone, Copy)]
pub(super) enum WriteSize {
    Const(u64),
    Value(ValueId),
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Starts emitting `func` with a dynamic spill base and returns the function to emit.
    ///
    /// The returned copy defines the base word with a placeholder at the top of its entry block;
    /// [`Self::emit_spill_base_init`] emits the initial address in its place.
    pub(super) fn begin_dynamic_spill_base(
        &mut self,
        func_id: FunctionId,
        func: &Function,
    ) -> Function {
        let mut func = func.clone();
        // spill_base = <initial spill-area address>
        let (inst, value) =
            func.alloc_value_inst(Instruction::new(InstKind::MSize, Some(MirType::I256)));
        func.blocks[BlockId::ENTRY].instructions.insert(0, inst);
        let area = self.asm.new_deferred_const();
        self.spill_base_area_consts.push((area, func_id, self.in_internal_function));
        let floor =
            *self.spill_base_floor_const.get_or_insert_with(|| self.asm.new_deferred_const());
        self.spill_base = Some(DynamicSpillBase { value, inst, area, floor });
        func
    }

    /// Returns whether `func_id` addresses its spill area through a runtime base word.
    pub(super) fn uses_dynamic_spill_base(&self, func_id: FunctionId) -> bool {
        func_id.index() < self.dynamic_spill_base_functions.domain_size()
            && self.dynamic_spill_base_functions.contains(func_id)
    }

    /// Regenerates `func_id` with a dynamic spill base on the next attempt.
    ///
    /// Its arguments then arrive in its frame, where they move with the spill area; a stack-only
    /// argument convention would need words the moves cannot reach.
    pub(super) fn request_dynamic_spill_base(&mut self, func_id: FunctionId) {
        if func_id.index() < self.dynamic_spill_base_functions.domain_size() {
            self.dynamic_spill_base_functions.insert(func_id);
            self.disabled_stack_only_functions.insert(func_id);
        }
    }

    /// Returns the blocks reachable from a block containing a low-memory clobber.
    pub(super) fn post_spill_hazard_blocks(
        &self,
        func: &Function,
        cfg: &CfgInfo,
    ) -> DenseBitSet<BlockId> {
        let inst_blocks = func.inst_blocks();
        let mut blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = self
            .spill_hazard_insts
            .iter()
            .filter_map(|inst| inst_blocks.get(inst).copied())
            .flat_map(|block| cfg.successors(block).iter().copied())
            .collect::<Vec<_>>();
        while let Some(block) = worklist.pop() {
            if blocks.insert(block) {
                worklist.extend(cfg.successors(block).iter().copied());
            }
        }
        blocks
    }

    /// Requests a dynamic spill base when a spill slot or frame argument at a fixed address is
    /// accessed after a low-memory clobber, where the word may lie inside the written buffer.
    pub(super) fn note_fixed_memory_access(&mut self) {
        if self.spill_base.is_none()
            && self.after_spill_hazard
            && let Some(func_id) = self.emitting_function
        {
            self.request_dynamic_spill_base(func_id);
        }
    }

    /// Returns whether a low-memory clobber can overwrite the frame-pointer word of a function
    /// with a dynamic internal frame. The word has no stack copy, so a moved spill area cannot
    /// protect the frame.
    pub(super) fn spill_hazard_clobbers_frame_pointer(&self, func: &Function) -> bool {
        let frame_pointer_end =
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE;
        self.in_internal_function
            && self.own_frame_addr_is_dynamic()
            && self.spill_hazard_insts.iter().any(|&inst| {
                Self::dynamic_spill_write_dest(func, inst).is_some_and(|dest| {
                    func.value_u64(dest).is_none_or(|dest| dest < frame_pointer_end)
                })
            })
    }

    /// Returns whether `inst` is the placeholder that defines the dynamic spill base.
    pub(super) fn is_spill_base_inst(&self, inst: InstId) -> bool {
        self.spill_base.as_ref().is_some_and(|base| base.inst == inst)
    }

    /// Pushes the static address of the current function's spill area as its dynamic base.
    pub(super) fn emit_spill_base_init(&mut self, func_id: FunctionId) {
        let value = self.spill_base.as_ref().expect("dynamic spill base is active").value;
        if self.in_internal_function {
            // spill_base = frame + first_argument_offset
            self.emit_own_frame_addr(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE);
        } else if self.in_constructor {
            // spill_base = constructor_spill_base
            let base = self.constructor_spill_base(self.immutable_encodings.len());
            self.asm.emit_push(U256::from(base));
        } else {
            // spill_base = external_spill_base(func)
            let id = self.asm.new_deferred_const();
            self.external_spill_base_consts.insert(func_id, id);
            self.asm.emit_push_deferred(id);
        }
        self.scheduler.stack.push(value);
    }

    /// Returns the physical stack depth of the dynamic spill base, if the function has one.
    fn spill_base_depth(&self) -> Option<usize> {
        let base = self.spill_base.as_ref()?;
        Some(self.spill_base_depth_override.unwrap_or_else(|| {
            self.scheduler.stack.find(base.value).unwrap_or_else(|| {
                panic!("dynamic spill base was lost: stack={:?}", self.scheduler.stack)
            })
        }))
    }

    /// Returns the model depth of the dynamic spill base, if it is on the modeled stack.
    pub(super) fn spill_base_model_depth(&self) -> Option<usize> {
        let base = self.spill_base.as_ref()?;
        self.scheduler.stack.find(base.value)
    }

    /// Returns the byte offset of an internal frame word from the frame's first argument.
    fn frame_word_offset(&self, offset: u64) -> u64 {
        let header = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE;
        match self.current_internal_function {
            Some(func_id) if self.static_frame_functions.contains(func_id) => {
                self.compact_static_frame_offset(func_id, offset)
                    - self.compact_static_frame_offset(func_id, header)
            }
            _ => offset - header,
        }
    }

    /// Returns the byte offset of a spill slot from the dynamic spill base.
    ///
    /// An internal function's base addresses its frame from the first argument, so arguments
    /// move with the spills. Other functions address only their spill area.
    fn spill_slot_base_offset(&self, func: &Function, slot: SpillSlot) -> u64 {
        if self.in_internal_function {
            self.frame_word_offset(self.internal_spill_slot_offset(func, slot))
        } else {
            u64::from(slot.offset) * EvmMemoryLayout::WORD_SIZE
        }
    }

    /// Emits `spill_base + offset` without touching the stack model.
    fn emit_spill_base_offset(&mut self, offset: u64) {
        let depth = self.spill_base_depth().expect("dynamic spill base is active");
        let limit = self.stack_access_limit();
        // Park the words above an unreachable base while copying it.
        let parked = (depth + 1).saturating_sub(limit);
        self.park_stack_words(parked);
        // dup spill_base
        // [push offset; add]
        // Unparking holds three scratch words above the address.
        let scratch = if parked == 0 { 2 } else { 4 };
        self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(scratch));
        self.asm.emit_stack_op(StackOp::Dup((depth - parked + 1) as u8));
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
        self.unpark_stack_words(parked);
    }

    /// Stores the top `count` physical stack words above `msize()`, top first, without
    /// touching the stack model.
    ///
    /// The EVM cannot reach below `DUP16`, and the spill base has no other home, so this is the
    /// only way back to it once scheduling has buried it. Memory at `msize()` holds nothing, and
    /// [`Self::unpark_stack_words`] clears it again.
    fn park_stack_words(&mut self, count: usize) {
        for _ in 0..count {
            // mstore(msize(), word)
            self.asm.emit_op(op::MSIZE);
            self.asm.emit_op(op::MSTORE);
        }
    }

    /// Restores `count` words parked by [`Self::park_stack_words`] below the current top word,
    /// in their original order, and clears their memory.
    fn unpark_stack_words(&mut self, count: usize) {
        for index in (0..count).rev() {
            // address = msize() - 32 * (count - index)
            // word = mload(address); mstore(address, 0)
            // [top, word]
            let back = U256::from((count - index) as u64 * EvmMemoryLayout::WORD_SIZE);
            self.asm.emit_push(back);
            self.asm.emit_op(op::MSIZE);
            self.asm.emit_op(op::SUB);
            self.asm.emit_stack_op(StackOp::Dup(1));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_op(op::MSTORE);
            self.asm.emit_stack_op(StackOp::Swap(1));
        }
    }

    /// Moves the dynamic spill base to the top of the stack, however deep it is.
    fn raise_spill_base(&mut self, depth: usize) {
        if depth == 0 {
            return;
        }
        let limit = self.stack_access_limit();
        if depth <= limit && StackOp::Swap(depth as u8).is_valid() {
            // swap spill_base to the top
            self.emit_stack_op(StackOp::Swap(depth as u8));
            return;
        }
        // [w0..wk, x, .., spill_base] => park w0..wk; swap spill_base with x; unpark
        // => [spill_base, w0..wk, .., x]
        let parked = depth - (limit - 1);
        self.park_stack_words(parked);
        self.asm.emit_stack_op(StackOp::Swap((limit - 1) as u8));
        self.unpark_stack_words(parked);
        let mut words = self.scheduler.stack.as_slice().to_vec();
        let base = words.remove(depth);
        let displaced = words.remove(parked);
        words.insert(0, base);
        words.insert(depth, displaced);
        let max_depth = self.scheduler.stack.max_depth();
        self.scheduler.stack = StackModel::from_top_to_bottom(words);
        self.scheduler.stack.inherit_max_depth(max_depth.max(self.scheduler.depth() + 2));
    }

    /// Emits the address of a spill slot relative to the dynamic spill base, without touching the
    /// stack model. Returns false when the function addresses its spill area statically.
    pub(super) fn emit_dynamic_spill_slot_addr(
        &mut self,
        func: &Function,
        slot: SpillSlot,
    ) -> bool {
        if self.spill_base.is_none() {
            return false;
        }
        self.emit_spill_base_offset(self.spill_slot_base_offset(func, slot));
        true
    }

    /// Emits the address of an internal function's own frame argument relative to the dynamic
    /// spill base. Returns false when the function addresses its frame statically.
    pub(super) fn emit_dynamic_frame_arg_addr(&mut self, index: ArgIdx) -> bool {
        if self.spill_base.is_none() || !self.in_internal_function {
            return false;
        }
        self.emit_spill_base_offset(index.index() as u64 * EvmMemoryLayout::WORD_SIZE);
        true
    }

    /// Removes the dynamic spill base before a function exit consumes the stack.
    pub(super) fn drop_spill_base(&mut self) {
        let Some(depth) = self.spill_base_model_depth() else { return };
        self.raise_spill_base(depth);
        // pop spill_base
        self.emit_stack_op(StackOp::Pop);
    }

    /// Moves the dynamic spill base toward the top of the stack before an instruction emits.
    pub(super) fn refresh_spill_base(&mut self) {
        let Some(depth) = self.spill_base_model_depth() else { return };
        if depth > SPILL_BASE_MAX_DEPTH {
            self.raise_spill_base(depth);
        }
    }

    /// Returns the destination and size of a memory write that may write at least one byte.
    pub(super) fn memory_write(func: &Function, inst: InstId) -> Option<(ValueId, WriteSize)> {
        let range = |dest: ValueId, size: ValueId| match func.value_u64(size) {
            Some(0) => None,
            Some(size) => Some((dest, WriteSize::Const(size))),
            None => Some((dest, WriteSize::Value(size))),
        };
        match func.inst(inst).kind {
            InstKind::MStore(dest, _) => Some((dest, WriteSize::Const(EvmMemoryLayout::WORD_SIZE))),
            InstKind::MStore8(dest, _) => Some((dest, WriteSize::Const(1))),
            InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::ExtCodeCopy(_, dest, _, size)
            | InstKind::MCopy(dest, _, size) => range(dest, size),
            InstKind::ReturnDataCopy(dest, offset, size) => {
                // `returndatacopy(_, returndatasize(), n)` writes nothing or traps.
                let starts_at_end = matches!(
                    func.value(offset),
                    Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize)
                );
                if starts_at_end { None } else { range(dest, size) }
            }
            InstKind::Call { ret_offset, ret_size, .. }
            | InstKind::CallCode { ret_offset, ret_size, .. }
            | InstKind::StaticCall { ret_offset, ret_size, .. }
            | InstKind::DelegateCall { ret_offset, ret_size, .. } => range(ret_offset, ret_size),
            _ => None,
        }
    }

    /// Returns the destination and size of a memory write whose range is not a compile-time
    /// constant. Constant ranges stay below the dynamic spill floor.
    fn dynamic_memory_write(func: &Function, inst: InstId) -> Option<(ValueId, WriteSize)> {
        Self::memory_write(func, inst).filter(|&(dest, size)| {
            func.value_u64(dest).is_none() || matches!(size, WriteSize::Value(_))
        })
    }

    /// Moves the spill area above a memory write that may overlap it.
    pub(super) fn relocate_spill_base_before_write(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        inst: InstId,
    ) {
        let Some(base) = &self.spill_base else { return };
        let (area, floor) = (base.area, base.floor);
        let Some((dest, size)) = Self::dynamic_memory_write(func, inst) else { return };

        // Words reloaded at or after the write: spill slots, and an internal function's frame
        // arguments. A later block may reload a slot stored on another path.
        let spills = &self.scheduler.spills;
        let mut live_words = spills
            .reloadable_values()
            .chain(spills.stored_values())
            .filter(|&value| liveness.is_used_at_or_after(value, block, inst_idx))
            .filter_map(|value| spills.get(value))
            .map(|slot| self.spill_slot_base_offset(func, slot))
            .collect::<Vec<_>>();
        if self.in_internal_function {
            live_words.extend(func.live_values().filter_map(|value| match func.value(value) {
                Value::Arg(index) if liveness.is_used_at_or_after(value, block, inst_idx) => {
                    Some(index.index() as u64 * EvmMemoryLayout::WORD_SIZE)
                }
                _ => None,
            }));
        }
        live_words.sort_unstable();
        live_words.dedup();

        // [size, dest]
        // Tracked operands emit first: a deep spill cannot save an anonymous word above them.
        match size {
            WriteSize::Const(size) => {
                self.emit_value(func, dest);
                self.asm.emit_push(U256::from(size));
                self.scheduler.stack.push_unknown();
                self.emit_stack_op(StackOp::Swap(1));
            }
            WriteSize::Value(size) => {
                self.emit_value(func, size);
                self.emit_value(func, dest);
            }
        }

        // end = dest + size
        self.emit_stack_op(StackOp::Dup(2));
        self.emit_stack_op(StackOp::Dup(2));
        self.emit_untracked_op(op::ADD);

        // overlap = spill_base < end && dest < spill_base + area [&& size != 0]
        self.emit_stack_op(StackOp::Dup(1));
        self.emit_spill_base_copy();
        self.emit_untracked_op(op::LT);
        self.asm.emit_push_deferred(area);
        self.scheduler.stack.push_unknown();
        self.emit_spill_base_copy();
        self.emit_untracked_op(op::ADD);
        self.emit_stack_op(StackOp::Dup(4));
        self.emit_untracked_op(op::LT);
        self.emit_untracked_op(op::AND);
        if matches!(size, WriteSize::Value(_)) {
            self.emit_stack_op(StackOp::Dup(4));
            self.emit_untracked_op(op::ISZERO);
            self.emit_untracked_op(op::ISZERO);
            self.emit_untracked_op(op::AND);
        }

        // jumpi skip, iszero(overlap)
        let skip = self.asm.new_label();
        self.emit_conditional_jump(skip, true);

        // new_base = max(round_up(end), msize(), floor)
        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(EvmMemoryLayout::WORD_SIZE - 1));
        self.scheduler.stack.push_unknown();
        self.emit_untracked_op(op::ADD);
        self.asm.emit_push(U256::from(EvmMemoryLayout::WORD_SIZE - 1));
        self.scheduler.stack.push_unknown();
        self.emit_untracked_op(op::NOT);
        self.emit_untracked_op(op::AND);
        self.emit_untracked_op(op::MSIZE);
        self.emit_max();
        self.asm.emit_push_deferred(floor);
        self.scheduler.stack.push_unknown();
        self.emit_max();

        // mstore(new_base + offset, mload(spill_base + offset)) for every live word
        for offset in live_words {
            self.emit_spill_base_copy();
            if offset != 0 {
                self.asm.emit_push(U256::from(offset));
                self.scheduler.stack.push_unknown();
                self.emit_untracked_op(op::ADD);
            }
            self.emit_untracked_op(op::MLOAD);
            self.emit_stack_op(StackOp::Dup(2));
            if offset != 0 {
                self.asm.emit_push(U256::from(offset));
                self.scheduler.stack.push_unknown();
                self.emit_untracked_op(op::ADD);
            }
            self.emit_untracked_op(op::MSTORE);
        }

        // spill_base = new_base
        self.replace_spill_base_with_top();

        // skip: pop end [; swap1; pop]
        self.asm.define_label(skip);
        self.emit_stack_op(StackOp::Pop);
        if matches!(size, WriteSize::Const(_)) {
            self.emit_stack_op(StackOp::Swap(1));
            self.emit_stack_op(StackOp::Pop);
        }
    }

    /// Replaces the dynamic spill base with the anonymous word on top of the stack. The model
    /// keeps the base's identity at its position.
    fn replace_spill_base_with_top(&mut self) {
        let depth = self.spill_base_depth().expect("dynamic spill base is active");
        assert!(StackOp::Swap(depth as u8).is_valid(), "dynamic spill base exceeded SWAP reach");
        // swap depth(spill_base); pop
        self.asm.emit_stack_op(StackOp::Swap(depth as u8));
        self.asm.emit_stack_op(StackOp::Pop);
        self.scheduler.stack.pop();
    }

    /// Forgets the spill stores of `values`, whose slots no longer hold them.
    fn forget_spill_stores(&mut self, values: impl IntoIterator<Item = ValueId>) {
        for value in values {
            self.scheduler.spills.invalidate_stored(value);
            if let Some(available) = &mut self.spill_available {
                available.remove(&value);
            }
        }
    }

    /// Pushes an anonymous copy of the dynamic spill base.
    fn emit_spill_base_copy(&mut self) {
        self.emit_spill_base_offset(0);
        self.scheduler.stack.push_unknown();
    }

    /// Emits an opcode over anonymous words and pushes its anonymous result, if any.
    fn emit_untracked_op(&mut self, opcode: u8) {
        let (pops, pushes) = op::stack_io(opcode).expect("opcode has a stack effect");
        let push = if pushes == 0 { StackPush::None } else { StackPush::Unknown };
        let effect = StackEffect { pops: pops.into(), pushes: pushes.into() };
        self.emit_op_with_effect(opcode, effect, push);
    }

    /// Replaces the top two anonymous words with their unsigned maximum.
    fn emit_max(&mut self) {
        // [a, b] => [a ^ ((a ^ b) * (a < b))]
        self.emit_stack_op(StackOp::Dup(2));
        self.emit_stack_op(StackOp::Dup(2));
        self.emit_untracked_op(op::LT);
        self.emit_stack_op(StackOp::Dup(3));
        self.emit_stack_op(StackOp::Dup(3));
        self.emit_untracked_op(op::XOR);
        self.emit_untracked_op(op::MUL);
        self.emit_untracked_op(op::XOR);
        self.emit_stack_op(StackOp::Swap(1));
        self.emit_stack_op(StackOp::Pop);
    }

    /// Returns whether `inst` may write the free-memory-pointer word with something other than
    /// a pointer. Compiler heap, frame, and return-buffer writes stay above it, and a word store
    /// to exactly `0x40` sets the pointer.
    fn may_clobber_free_memory_slot(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        inst: InstId,
    ) -> bool {
        let Some((dest, size)) = Self::memory_write(func, inst) else { return false };
        let dest_offset = func.value_u64(dest);
        if matches!(func.inst(inst).kind, InstKind::MStore(..))
            && dest_offset == Some(EvmMemoryLayout::FMP_SLOT)
        {
            return false;
        }
        let size = match size {
            WriteSize::Const(size) => Some(size),
            WriteSize::Value(_) => None,
        };
        Self::constant_memory_range_may_overlap_fmp(dest_offset, size)
            && !self.write_is_owned(func, aa, inst, dest)
    }

    /// Summarizes, directly or through calls, which functions may leave something other than a
    /// pointer in the free-memory-pointer word, may write memory they did not allocate, may
    /// write memory reached through the free-memory pointer, and may make a spill hazard before
    /// returning. Any internal call counts as the third, since a dynamic frame comes from that
    /// pointer.
    pub(super) fn collect_memory_summaries(&self, module: &Module) -> [DenseBitSet<FunctionId>; 4] {
        let [mut free_memory, mut unowned, mut heap, mut hazards] =
            std::array::from_fn(|_| DenseBitSet::new_empty(module.functions.len()));
        // A hazard on a path that ends the call frame cannot reach the caller's spills. Only
        // variable-length writes count: a sweeping word store is usually bounded in ways the
        // hazard analysis cannot see, such as a byte scan over scratch space.
        let returning = module
            .functions
            .iter()
            .map(Self::blocks_returning_to_caller)
            .collect::<IndexVec<FunctionId, _>>();
        for (func_id, func) in module.functions.iter_enumerated() {
            let mut direct = self.direct_spill_hazard_insts(func);
            direct.retain(|&inst| {
                !matches!(func.inst(inst).kind, InstKind::MStore(..) | InstKind::MStore8(..))
            });
            if func.blocks.iter_enumerated().any(|(block_id, block)| {
                returning[func_id].contains(block_id)
                    && block.instructions.iter().any(|inst| direct.contains(inst))
            }) {
                hazards.insert(func_id);
            }
            let aa = AliasAnalysis::new(func);
            for inst in func.instructions() {
                if matches!(func.inst(inst).kind, InstKind::ICall { .. }) {
                    heap.insert(func_id);
                    continue;
                }
                if self.may_clobber_free_memory_slot(func, &aa, inst) {
                    free_memory.insert(func_id);
                }
                if let Some((dest, _)) = Self::dynamic_memory_write(func, inst) {
                    if self.write_is_owned(func, &aa, inst, dest) {
                        heap.insert(func_id);
                    } else {
                        unowned.insert(func_id);
                    }
                }
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for (func_id, func) in module.functions.iter_enumerated() {
                for (block_id, block) in func.blocks.iter_enumerated() {
                    for &inst in &block.instructions {
                        let InstKind::ICall { function, .. } = &func.inst(inst).kind else {
                            continue;
                        };
                        for set in [&mut free_memory, &mut unowned, &mut heap] {
                            if !matches!(function, Callee::Function(callee) if !set.contains(*callee))
                            {
                                changed |= set.insert(func_id);
                            }
                        }
                        if returning[func_id].contains(block_id)
                            && matches!(function, Callee::Function(callee) if hazards.contains(*callee))
                        {
                            changed |= hazards.insert(func_id);
                        }
                    }
                }
            }
        }
        [free_memory, unowned, heap, hazards]
    }

    /// Returns the blocks of `func` from which control can return to its caller.
    fn blocks_returning_to_caller(func: &Function) -> DenseBitSet<BlockId> {
        let mut blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = func
            .blocks
            .iter_enumerated()
            .filter(|(_, block)| {
                matches!(
                    block.terminator,
                    Some(Terminator::Return { .. } | Terminator::TailCall { .. })
                )
            })
            .map(|(block_id, _)| block_id)
            .collect::<Vec<_>>();
        while let Some(block) = worklist.pop() {
            if blocks.insert(block) {
                worklist.extend(func.blocks[block].predecessors.iter().copied());
            }
        }
        blocks
    }

    /// Returns the internal calls before which the free-memory-pointer word still holds the
    /// pointer on every path. An internal function's caller may already have clobbered it.
    pub(super) fn free_memory_trusted_calls(&self, func: &Function) -> FxHashSet<InstId> {
        let callee_clobbers = |inst: InstId| match &func.inst(inst).kind {
            InstKind::ICall { function: Callee::Function(callee), .. } => {
                callee.index() >= self.free_memory_clobbering_functions.domain_size()
                    || self.free_memory_clobbering_functions.contains(*callee)
            }
            InstKind::ICall { .. } => true,
            _ => false,
        };
        let aa = AliasAnalysis::new(func);
        // clobbered_in[block]: some path to the block's entry may have clobbered the word.
        let mut clobbered_in = DenseBitSet::new_empty(func.blocks.len());
        if self.in_internal_function {
            clobbered_in.insert_all();
        }
        let transfer = |block: BlockId, mut clobbered: bool, trusted: &mut FxHashSet<InstId>| {
            for &inst in &func.blocks[block].instructions {
                if !clobbered && matches!(func.inst(inst).kind, InstKind::ICall { .. }) {
                    trusted.insert(inst);
                }
                if matches!(func.inst(inst).kind, InstKind::MStore(dest, _)
                    if func.value_u64(dest) == Some(EvmMemoryLayout::FMP_SLOT))
                {
                    clobbered = false;
                } else if self.may_clobber_free_memory_slot(func, &aa, inst)
                    || callee_clobbers(inst)
                {
                    clobbered = true;
                }
            }
            clobbered
        };
        let mut scratch = FxHashSet::default();
        let mut changed = true;
        while changed {
            changed = false;
            for (block_id, block) in func.blocks.iter_enumerated() {
                let clobbered = transfer(block_id, clobbered_in.contains(block_id), &mut scratch);
                if clobbered {
                    for successor in block.terminator.iter().flat_map(Terminator::successors) {
                        changed |= clobbered_in.insert(successor);
                    }
                }
            }
        }
        let mut trusted = FxHashSet::default();
        for block_id in func.blocks.indices() {
            transfer(block_id, clobbered_in.contains(block_id), &mut trusted);
        }
        trusted
    }

    /// Raises the free-memory pointer above the dynamic spill area before an internal call, so
    /// the callee's allocations and frames cannot land in it.
    pub(super) fn emit_free_memory_bump(&mut self) {
        let Some(base) = &self.spill_base else { return };
        let area = base.area;
        // mstore(FMP_SLOT, max(mload(FMP_SLOT), spill_base + area))
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.scheduler.stack.push_unknown();
        self.emit_untracked_op(op::MLOAD);
        self.emit_spill_base_copy();
        self.asm.emit_push_deferred(area);
        self.scheduler.stack.push_unknown();
        self.emit_untracked_op(op::ADD);
        self.emit_max();
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.scheduler.stack.push_unknown();
        self.emit_untracked_op(op::MSTORE);
    }

    /// Returns whether a write at the runtime address `dest` lands in memory its function owns:
    /// a heap allocation, an internal frame, or a return buffer.
    fn write_is_owned(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        inst: InstId,
        dest: ValueId,
    ) -> bool {
        if matches!(
            func.inst(inst).metadata.memory_region(),
            Some(MemoryRegion::Heap | MemoryRegion::InternalFrame | MemoryRegion::AbiReturn)
        ) {
            return true;
        }
        let Some(address) = aa.memory_address(func, dest) else { return false };
        if matches!(address.region, MemoryRegion::Heap | MemoryRegion::InternalFrame) {
            return true;
        }
        match address.base {
            MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::Param(_)
            | MemoryBase::InternalFrame => true,
            MemoryBase::Absolute => false,
            MemoryBase::Value(value) => {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                self.heap_pointer_provenance(func, aa, value, &mut visiting, &mut memo)
                    == Some(true)
            }
        }
    }

    /// Returns whether an internal call may write a dynamic spill area. The callee may write
    /// memory it did not allocate, or it allocates while the free-memory pointer was not raised
    /// past the area.
    pub(super) fn call_may_write_spill_area(
        &self,
        func: &Function,
        inst: InstId,
        pointer_raised: bool,
    ) -> bool {
        let InstKind::ICall { function, .. } = &func.inst(inst).kind else { return false };
        let Callee::Function(callee) = function else { return true };
        let known = |set: &DenseBitSet<FunctionId>| {
            callee.index() >= set.domain_size() || set.contains(*callee)
        };
        known(&self.unowned_memory_writers)
            || (!pointer_raised
                && (known(&self.heap_memory_writers)
                    || !self.static_frame_functions.contains(*callee)))
    }

    /// Emits every value used after an internal call onto the stack, where the call cannot
    /// write it, and forgets their spill stores. Frame arguments are carried with
    /// `include_args`: a recursive callee reuses the frame, and a dynamic spill base moves it.
    pub(super) fn carry_live_call_values(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        result: Option<ValueId>,
        include_args: bool,
    ) -> Vec<ValueId> {
        let mut seen = FxHashSet::default();
        let mut values = Vec::new();
        let mut recomputed = Vec::new();
        // A constructor's arguments live in memory the callee may write.
        let rebuildable = (!self.in_constructor)
            .then(|| cross_block_values(func, |value| !self.scheduler.is_stack_only_value(value)));
        let spill_base = self.spill_base.as_ref().map(|base| base.value);
        // A reserved slot can be reloadable before the block defines its value.
        let pending = &func.blocks[block].instructions[inst_idx..];
        // A later block may reload a slot stored on another path, so stored values count too.
        for value in self
            .scheduler
            .stack
            .iter()
            .flatten()
            .chain(self.scheduler.spills.reloadable_values())
            .chain(self.scheduler.spills.stored_values())
            .collect::<Vec<_>>()
        {
            if Some(value) != result
                && Some(value) != spill_base
                && !matches!(func.value(value), Value::Inst(def) if pending.contains(def))
                && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                && (self.scheduler.stack.contains(value)
                    || self.scheduler.can_emit_value(value, func))
                && seen.insert(value)
            {
                // Rebuild a value from its arguments after the call, unless a slot holds it.
                let spills = &mut self.scheduler.spills;
                if rebuildable.as_ref().is_some_and(|set| set.contains(value))
                    && !matches!(func.value(value), Value::Arg(_))
                    && !spills.is_stored(value)
                    && !spills.is_reloadable(value)
                {
                    recomputed.push(value);
                } else {
                    values.push(value);
                }
            }
        }
        // A rebuilt value recomputes its operands and reloads the arguments they depend on from
        // the moved frame.
        let mut used_args = DenseBitSet::new_empty(func.num_values());
        while let Some(value) = recomputed.pop() {
            match func.value(value) {
                Value::Arg(_) => {
                    used_args.insert(value);
                }
                Value::Inst(def) => {
                    let spills = &mut self.scheduler.spills;
                    if !spills.is_stored(value) && !spills.is_reloadable(value) {
                        spills.allocate(value);
                        spills.mark_recomputable(value);
                    }
                    recomputed.extend(func.inst(*def).kind.operands());
                }
                _ => {}
            }
        }
        if include_args {
            for value in func.live_values() {
                if Some(value) != result
                    && matches!(func.value(value), Value::Arg(_))
                    && (used_args.contains(value)
                        || liveness.is_used_at_or_after(value, block, inst_idx + 1))
                    && seen.insert(value)
                {
                    values.push(value);
                }
            }
        }
        for &value in &values {
            if !self.scheduler.stack.contains(value) {
                self.emit_value(func, value);
            }
        }
        let spills = &self.scheduler.spills;
        self.carried_spill_values = values
            .iter()
            .copied()
            .filter(|&value| spills.get(value).is_some() && spills.is_reloadable(value))
            .collect();
        self.forget_spill_stores(values.iter().copied());
        self.carried_call_values.clone_from(&values);
        values
    }

    /// Drops the stack words an internal call neither carries nor passes, then returns whether
    /// the carried values stay within stack reach with `words_above` more words pushed above
    /// them. Otherwise reports an error and stops carrying, so emission can finish.
    pub(super) fn carried_call_values_fit(
        &mut self,
        func: &Function,
        resident: &mut Vec<ValueId>,
        args: &[ValueId],
        words_above: usize,
    ) -> bool {
        let limit = self.stack_access_limit();
        let needed = resident.iter().chain(args).copied().collect::<Vec<_>>();
        while let Some(depth) = self.first_stack_value_not_needed_by(&needed)
            && depth <= limit
        {
            if depth > 0 {
                self.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.emit_stack_op(StackOp::Pop);
        }
        let deepest = resident.iter().filter_map(|&value| self.scheduler.stack.find(value)).max();
        if resident.len() + words_above < limit
            && deepest.is_none_or(|depth| depth + words_above < limit)
        {
            return true;
        }
        let carried = std::mem::take(&mut self.carried_call_values);
        if let Some(func_id) = self.emitting_function
            && self.carried_call_errors.insert(func_id)
        {
            self.gcx
                .dcx()
                .err(format!(
                    "codegen cannot keep {} values of `{}` on the stack across an internal call \
                     after a dynamic low-memory write",
                    carried.len(),
                    func.name
                ))
                .note("the callee may write any memory the values could be spilled to")
                .emit();
        }
        resident.retain(|value| !carried.contains(value));
        self.carry_live_across_call = false;
        false
    }

    /// Moves the dynamic spill base above everything an internal call may have written, after
    /// the caller's live values were carried across it on the stack.
    pub(super) fn move_spill_base_above_msize(&mut self) {
        let Some(base) = &self.spill_base else { return };
        let floor = base.floor;
        // spill_base = max(msize(), floor)
        self.emit_untracked_op(op::MSIZE);
        self.asm.emit_push_deferred(floor);
        self.scheduler.stack.push_unknown();
        self.emit_max();
        self.replace_spill_base_with_top();
        // Every stored slot now lies in the abandoned area.
        let stored = self.scheduler.spills.reloadable_values().collect::<Vec<_>>();
        self.forget_spill_stores(stored);
    }

    /// Stores carried values back into the moved area wherever the old area held them, so
    /// blocks that expect those slots, including blocks already emitted, still find them.
    pub(super) fn restore_carried_values(
        &mut self,
        func: &Function,
        values: &[ValueId],
        spilled: &[ValueId],
    ) {
        for &value in values {
            let Value::Arg(index) = func.value(value) else { continue };
            if !self.in_internal_function {
                break;
            }
            let Some(depth) = self.scheduler.stack.find(value) else { continue };
            assert!(depth < self.stack_access_limit(), "carried argument exceeded DUP reach");
            // mstore(spill_base + argument_offset, argument)
            self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
            self.emit_dynamic_frame_arg_addr(*index);
            self.scheduler.stack.push_unknown();
            self.emit_untracked_op(op::MSTORE);
        }
        for &value in spilled {
            let (Some(slot), Some(depth)) =
                (self.scheduler.spills.get(value), self.scheduler.stack.find(value))
            else {
                continue;
            };
            assert!(depth < self.stack_access_limit(), "carried value exceeded DUP reach");
            // mstore(spill_base + slot_offset, value)
            self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
            self.store_stack_top_to_spill(func, value, slot);
        }
    }
}
