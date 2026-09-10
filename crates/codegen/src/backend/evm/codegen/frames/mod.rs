//! Internal frame placement, address resolution, and spill memory layout.
//!
//! After function emission and spill DSE, scalar static frames retain only referenced words.
//! Every surviving deferred address keeps its identity while its word moves to a dense offset;
//! address-taken locals, contiguous multiword return buffers, and recursive frames keep their
//! existing layouts. External entries likewise reserve only surviving spill words.
//!
//! Static helpers overlay when their activations cannot coexist. The conservative placement
//! reserves a region above every external entry; a final call-graph relaxation can lower each
//! helper above its actual callers instead. Shared helpers take the maximum caller end, and an
//! entry's heap starts above all reachable frames. This refinement must converge, never increase
//! a frame address, and preserves the separate recursive-frame prefix and heap-prefix guards.
//! All decisions use executable references and are independent of debug metadata.

use super::{
    ArgIdx, CallGraphInfo, DebugFunction, DebugFunctionExit, DeferredConst, DenseBitSet,
    EvmCodegen, EvmMemoryLayout, Function, FunctionId, FxHashMap, FxHashSet, InstKind, MirType,
    Module, RelayoutAddress, SpillSlot, StackEffect, StackOp, StackPush, Terminator, U256, Value,
    ValueId, WORD_BYTES, immutable_staging_end, op, preserves_push_width,
};

/// A dynamic-length write to a low absolute base below this bound above
/// `HEAP_START` is treated as possibly reaching the spill area.
const SPILL_HAZARD_BOUND: u64 = 0x2000;

mod hazards;

impl<'gcx> EvmCodegen<'gcx> {
    /// Packs compiler-owned scalar words after all bodies have emitted and dead spill stores
    /// have been removed. These words have no address arithmetic or escaping pointers: every
    /// access uses its own deferred binding. Keeping the binding ID preserves aliasing between
    /// callers and callees while unused signature and spill words disappear.
    ///
    /// Address-taken locals, multiword return buffers and recursive frames retain their layouts.
    /// Constructors retain their independent frame convention.
    pub(in crate::backend::evm::codegen) fn pack_scalar_static_frames(&mut self, module: &Module) {
        if !self.runtime_stack_args
            || !(self.gcx.sess.opts.optimization.is_gas()
                || self.gcx.sess.opts.optimization.is_size())
        {
            return;
        }
        let referenced = self.asm.referenced_deferred_constants().collect::<FxHashSet<_>>();
        self.static_frame_addr_consts.retain(|_, (id, _)| referenced.contains(id));
        let mut packed = DenseBitSet::new_empty(module.functions.len());
        for func_id in self.static_frame_functions.iter() {
            let func = &module.functions[func_id];
            if !self.recursive_frame_functions.contains(func_id)
                && func.internal_frame_size == 0
                && func.returns.len() <= 1
                && !func
                    .instructions()
                    .any(|inst| matches!(func.inst(inst).kind, InstKind::InternalFrameAddr(_)))
            {
                packed.insert(func_id);
                self.packed_static_frame_sizes.insert(func_id, 0);
            }
        }
        let mut addresses =
            std::mem::take(&mut self.static_frame_addr_consts).into_iter().collect::<Vec<_>>();
        addresses.sort_unstable_by_key(|&(key, _)| key);
        for ((func_id, offset), constant) in addresses {
            let offset = if packed.contains(func_id) {
                let size = self.packed_static_frame_sizes.get_mut(&func_id).unwrap();
                let rank = *size;
                *size += EvmMemoryLayout::WORD_SIZE;
                rank
            } else {
                offset
            };
            // load/store frame[old_offset] -> load/store frame[packed_offset]
            self.static_frame_addr_consts.insert((func_id, offset), constant);
        }
        for &entry in &self.runtime_entry_funcs {
            let size = if let Some(slots) = self.external_spill_addr_consts.get_mut(&entry) {
                slots.retain(|(id, _)| referenced.contains(id));
                slots.len() as u64 * EvmMemoryLayout::WORD_SIZE
            } else {
                0
            };
            self.function_spill_sizes.insert(entry, size);
        }
    }

    /// Records the exact spill area size of the function body that just emitted.
    pub(in crate::backend::evm::codegen) fn record_function_spill_size(
        &mut self,
        func_id: FunctionId,
    ) -> u64 {
        let spill_size = u64::from(self.scheduler.spills.spill_area_size());
        self.function_spill_sizes.insert(func_id, spill_size);
        spill_size
    }

    pub(in crate::backend::evm::codegen) fn mark_debug_function_invoke(&mut self, func: &Function) {
        if self.capture_debug_info
            && !func.declaration_span.is_dummy()
            && let Some(identifier) = func.debug_identifier
        {
            self.asm.mark_function_invoke(DebugFunction {
                identifier,
                declaration: func.declaration_span,
            });
        }
    }

    pub(in crate::backend::evm::codegen) fn mark_debug_function_exit(
        &mut self,
        func: &Function,
        exit: DebugFunctionExit,
    ) {
        if self.capture_debug_info
            && !func.declaration_span.is_dummy()
            && func.debug_identifier.is_some()
        {
            self.asm.mark_function_exit(exit);
        }
    }

    /// Returns the exact spill area recorded for `func_id` after emission.
    fn function_spill_size(&self, func_id: FunctionId) -> u64 {
        self.function_spill_sizes.get(&func_id).copied().unwrap_or_else(|| {
            panic!("spill size for emitted function {func_id:?} was not recorded")
        })
    }

    /// Resolves all pending internal-call frame-size constants.
    ///
    /// Every pending constant belongs to a labeled callee. Runtime and
    /// constructor emission record all labeled bodies before reaching this
    /// resolution point.
    pub(in crate::backend::evm::codegen) fn resolve_pending_frame_size_consts(
        &mut self,
        module: &Module,
    ) {
        for (id, callee) in std::mem::take(&mut self.pending_frame_size_consts) {
            self.asm.set_deferred_const(id, U256::from(self.emitted_frame_size(module, callee)));
        }
    }

    /// Whether a directly self-recursive Yul helper can reuse one static
    /// scratch frame while suspended activations carry their live state on the
    /// EVM stack. Void recursion has no child result to preserve; one-result
    /// recursion stages that word before restoring the suspended activation.
    pub(in crate::backend::evm::codegen) fn uses_reentrant_static_frame(
        func_id: FunctionId,
        func: &Function,
    ) -> bool {
        func.attributes.is_yul
            && func.returns.len() <= 1
            && Self::has_direct_self_call(func_id, func)
    }

    pub(in crate::backend::evm::codegen) fn has_direct_self_call(
        func_id: FunctionId,
        func: &Function,
    ) -> bool {
        func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::ICall { function, .. }
                if function == func_id)
        }) || func.blocks.iter().any(|block| {
            matches!(block.terminator, Some(Terminator::TailCall { function, .. })
                if function == func_id)
        })
    }

    pub(in crate::backend::evm::codegen) fn is_external_entry(func: &Function) -> bool {
        Self::is_runtime_function(func)
            && (func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback)
    }

    pub(in crate::backend::evm::codegen) fn is_runtime_function(func: &Function) -> bool {
        !func.attributes.is_constructor
    }

    /// Returns whether every explicit frame address belongs to the local region above the dynamic
    /// header and signature slots.
    ///
    /// Static frames omit the header, while stack-only arguments and returns may omit signature
    /// slots. Parsed MIR can address those regions directly, without identifying the aliased
    /// component, so such a function must keep the ordinary dynamic-frame convention.
    pub(in crate::backend::evm::codegen) fn static_frame_offsets_are_local(
        func: &Function,
    ) -> bool {
        let Some(signature_slots) = func.params.len().checked_add(func.returns.len()) else {
            return false;
        };
        let Some(signature_size) = u64::try_from(signature_slots)
            .ok()
            .and_then(|slots| slots.checked_mul(EvmMemoryLayout::WORD_SIZE))
        else {
            return false;
        };
        let Some(local_start) =
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)
        else {
            return false;
        };

        let Some(local_end) = local_start.checked_add(func.internal_frame_size) else {
            return false;
        };

        func.instructions().all(|inst_id| match func.inst(inst_id).kind {
            InstKind::InternalFrameAddr(offset) => offset >= local_start && offset < local_end,
            _ => true,
        })
    }

    pub(in crate::backend::evm::codegen) fn emit_new_internal_frame_base_tracked(&mut self) {
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
    }

    pub(in crate::backend::evm::codegen) fn emit_internal_frame_store_from_top_preserving_base(
        &mut self,
        offset: u64,
    ) {
        self.emit_stack_op(StackOp::Dup(2));
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::ADD,
                StackEffect { pops: 2, pushes: 1 },
                StackPush::Unknown,
            );
        }
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    pub(in crate::backend::evm::codegen) fn emit_store_frame_base_to_current_frame_slot(&mut self) {
        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    pub(in crate::backend::evm::codegen) fn emit_store_new_free_pointer_from_frame_base(
        &mut self,
        frame_size: DeferredConst,
    ) {
        self.asm.emit_push_deferred(frame_size);
        self.scheduler.stack.push_unknown();
        self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, StackPush::Unknown);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    /// Address of `offset` within whatever frame the frame-pointer slot
    /// currently holds. Dynamic call sites use this to reach the callee frame
    /// right after a call (before the pointer is restored); dynamic functions
    /// use it for their own frame. For accesses that are statically about the
    /// CURRENT function's own frame, use [`Self::emit_own_frame_addr`], which
    /// resolves to an absolute address when the function has a static frame.
    pub(in crate::backend::evm::codegen) fn emit_current_internal_frame_addr(
        &mut self,
        offset: u64,
    ) {
        let growth = if offset == 0 { 1 } else { 2 };
        self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(growth));
        self.emit_current_internal_frame_addr_untracked(offset);
    }

    fn emit_current_internal_frame_addr_untracked(&mut self, offset: u64) {
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
    }

    pub(in crate::backend::evm::codegen) fn emit_constructor_args_base(&mut self) {
        let id = self
            .constructor_args_base_const
            .expect("constructor argument base used outside constructor codegen");
        self.asm.emit_push_deferred(id);
    }

    pub(in crate::backend::evm::codegen) fn emit_constructor_args_end(&mut self) {
        let offset = self
            .constructor_args_offset_const
            .expect("constructor argument end used outside constructor codegen");
        // base = constructor_args_base
        // end = base + (codesize - constructor_args_offset)
        self.emit_constructor_args_base();
        self.asm.emit_push_deferred(offset);
        self.asm.emit_op(op::CODESIZE);
        self.asm.emit_op(op::SUB);
        self.asm.emit_op(op::ADD);
    }

    pub(in crate::backend::evm::codegen) fn emit_constructor_arg_load(&mut self, index: ArgIdx) {
        self.emit_constructor_args_base();
        let offset = index.index() as u64 * EvmMemoryLayout::WORD_SIZE;
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
        self.asm.emit_op(op::MLOAD);
    }

    /// Address of `offset` within the current function's own frame: a single
    /// absolute push for static-frame functions, the frame-pointer indirection
    /// otherwise.
    pub(in crate::backend::evm::codegen) fn emit_own_frame_addr(&mut self, offset: u64) {
        if self.own_frame_addr_is_dynamic() {
            let growth = if offset == 0 { 1 } else { 2 };
            self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(growth));
        }
        self.emit_own_frame_addr_untracked(offset);
    }

    fn emit_own_frame_addr_untracked(&mut self, offset: u64) {
        if let Some(func_id) = self.current_internal_function
            && self.static_frame_functions.contains(func_id)
        {
            let addr = self.static_frame_addr(func_id, offset);
            self.asm.emit_push_deferred(addr);
            return;
        }
        if !self.in_internal_function && !self.in_constructor {
            self.asm.emit_push(U256::from(EvmMemoryLayout::HEAP_START + offset));
            return;
        }
        self.emit_current_internal_frame_addr_untracked(offset);
    }

    fn own_frame_addr_is_dynamic(&self) -> bool {
        self.current_internal_function
            .is_none_or(|func_id| !self.static_frame_functions.contains(func_id))
            && (self.in_internal_function || self.in_constructor)
    }

    /// Removes the unused dynamic-frame header and single stack-return word from a static frame.
    pub(in crate::backend::evm::codegen) fn compact_static_frame_offset(
        &self,
        func_id: FunctionId,
        offset: u64,
    ) -> u64 {
        // Single-word stack returns remove their backing slot even on the
        // frame-backed fallback. Multiword returns retain their ordinary area
        // so a failed bounded projection has compiler-owned staging memory.
        let mut compact = if self.runtime_stack_args {
            offset
                .checked_sub(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE)
                .expect("static frame header is still referenced")
        } else {
            offset
        };
        if let Some(plan) = self.stack_return_plan(func_id)
            && plan.arity == 1
        {
            let return_size = plan.arity as u64 * EvmMemoryLayout::WORD_SIZE;
            let return_base = plan.local_base - return_size;
            debug_assert!(
                !(return_base..plan.local_base).contains(&offset),
                "removed stack-return slot is still referenced: func={func_id:?}"
            );
            if offset >= plan.local_base {
                compact -= return_size;
            }
        }
        compact
    }

    pub(in crate::backend::evm::codegen) fn static_frame_addr(
        &mut self,
        func_id: FunctionId,
        offset: u64,
    ) -> DeferredConst {
        let offset = self.compact_static_frame_offset(func_id, offset);
        if let Some((id, references)) = self.static_frame_addr_consts.get_mut(&(func_id, offset)) {
            *references += 1;
            return *id;
        }
        let id = self.asm.new_deferred_const();
        self.static_frame_addr_consts.insert((func_id, offset), (id, 1));
        id
    }

    /// Total emitted frame size of `func_id`, including its exact spill area.
    pub(in crate::backend::evm::codegen) fn emitted_frame_size(
        &self,
        module: &Module,
        func_id: FunctionId,
    ) -> u64 {
        if let Some(&size) = self.packed_static_frame_sizes.get(&func_id) {
            return size;
        }
        let func = &module.functions[func_id];
        let header = if self.runtime_stack_args && self.static_frame_functions.contains(func_id) {
            0
        } else {
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        };
        let size = header
            + ((func.params.len() + func.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE
            + func.internal_frame_size
            + self.function_spill_size(func_id);
        if let Some(plan) = self.stack_return_plan(func_id)
            && plan.arity == 1
        {
            size - plan.arity as u64 * EvmMemoryLayout::WORD_SIZE
        } else {
            size
        }
    }

    /// Places every referenced static frame and resolves the address and
    /// free-memory-pointer constants recorded during this pass.
    ///
    /// Placement is an overlay: `base(f) = region_start + depth(f)`, where
    /// `depth(f)` is the longest chain of static frames that can be live below
    /// an activation of `f`. Depth propagates along every call edge — a static
    /// caller contributes its frame size, while an external entry whose locals
    /// live below the region only forwards its depth. Supported recursive Yul
    /// components occupy a disjoint prefix with one frame per function; their
    /// call edges are weight-zero because a nested activation reuses the same
    /// function frame after carrying its suspended state on the EVM stack.
    /// Every remaining cycle is therefore weight-zero and the relaxation
    /// converges. Functions that can never be simultaneously live end up
    /// sharing addresses; that is the point of the overlay.
    ///
    /// The heap floor moves up to `region_end`: each entry's free-pointer
    /// constant accounts for its exact spill area and every accepted static
    /// allocation, plus the overlaid helper region when one is referenced.
    pub(in crate::backend::evm::codegen) fn resolve_static_frames(&mut self, module: &Module) {
        let uses_dynamic_internal_frames = !self.runtime_stack_args
            || module.functions.iter().any(|func| {
                func.instructions().any(|inst_id| {
                    matches!(
                        func.inst(inst_id).kind,
                        InstKind::ICall { function, .. }
                            if !self.static_frame_functions.contains(function)
                    )
                }) || func.blocks.iter().any(|block| {
                    // Dispatch and external-fusion tail calls never touch internal
                    // frames; only a selector-less callee outside the static set
                    // could imply dynamic frames (a shape `lower-evm-shaped` does
                    // not currently form).
                    matches!(
                        &block.terminator,
                        Some(Terminator::TailCall { function, .. })
                            if module.functions[*function].selector.is_none()
                                && !self.static_frame_functions.contains(*function)
                    )
                })
            });
        let low_memory_end = if uses_dynamic_internal_frames {
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        let runtime_entries = std::mem::take(&mut self.runtime_entry_funcs);
        let reachable_memory_marks = runtime_entries
            .iter()
            .copied()
            .map(|entry| {
                let mark = self
                    .runtime_entry_reachability
                    .get(&entry)
                    .into_iter()
                    .flat_map(|reachable| reachable.iter())
                    .map(|func_id| {
                        Self::constant_memory_high_water_mark(&module.functions[func_id])
                    })
                    .max()
                    .unwrap_or_else(|| {
                        Self::constant_memory_high_water_mark(&module.functions[entry])
                    });
                (entry, mark)
            })
            .collect::<FxHashMap<_, _>>();
        let entry_bases: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| {
                (
                    func_id,
                    Self::external_spill_base(
                        &module.functions[func_id],
                        uses_dynamic_internal_frames,
                        reachable_memory_marks[&func_id],
                    ),
                )
            })
            .collect();
        let mut entry_ends: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| (func_id, entry_bases[&func_id] + self.function_spill_size(func_id)))
            .collect();

        // Longest live-chain depth below each function, over all call edges.
        // Only emitted callers count: an unemitted function (an internal
        // `.body` clone nobody calls, unreachable dead code) stacks no real
        // frame below its callees.
        let mut edges = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !self.function_labels.contains_key(&func_id) {
                continue;
            }
            for inst_id in func.instructions() {
                if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
                    edges.push((func_id, function));
                }
            }
            for block in func.blocks.iter() {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    edges.push((func_id, *function));
                }
            }
        }
        let mut depth: FxHashMap<FunctionId, u64> = FxHashMap::default();
        for _ in 0..=module.functions.len() {
            let mut changed = false;
            for &(caller, callee) in &edges {
                let mut contribution = depth.get(&caller).copied().unwrap_or(0);
                if self.static_frame_functions.contains(caller)
                    && !self.recursive_frame_functions.contains(caller)
                {
                    contribution += self.emitted_frame_size(module, caller);
                }
                if contribution > depth.get(&callee).copied().unwrap_or(0) {
                    depth.insert(callee, contribution);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let placed: FxHashSet<FunctionId> =
            self.static_frame_addr_consts.keys().map(|&(func_id, _)| func_id).collect();
        let mut recursive_placed: Vec<_> = placed
            .iter()
            .copied()
            .filter(|&func_id| self.recursive_frame_functions.contains(func_id))
            .collect();
        recursive_placed.sort_unstable();
        let recursive_span = recursive_placed
            .iter()
            .map(|&func_id| self.emitted_frame_size(module, func_id))
            .sum::<u64>();
        let mut frame_relative = FxHashMap::default();
        let mut static_span = 0;
        for &func_id in &placed {
            let frame_size = self.emitted_frame_size(module, func_id);
            assert!(
                self.static_frame_addr_consts
                    .keys()
                    .filter(|&&(referenced, _)| referenced == func_id)
                    .all(|&(_, offset)| offset
                        .checked_add(EvmMemoryLayout::WORD_SIZE)
                        .is_some_and(|end| end <= frame_size)),
                "static frame reference exceeds emitted frame size for `{}`",
                module.functions[func_id].name
            );
            if self.recursive_frame_functions.contains(func_id) {
                continue;
            }
            let relative = *frame_relative
                .entry(func_id)
                .or_insert_with(|| depth.get(&func_id).copied().unwrap_or(0));
            static_span = static_span.max(relative + frame_size);
        }
        // ordinary static frames: [region_start, ordinary_end)
        // recursive scratch frames: [ordinary_end, static_end)
        //
        // Keeping the scratch suffix after ordinary helpers prevents a large
        // recursive function from raising shared helper addresses. An entry
        // reserves the suffix only when its reachability set includes that
        // recursive function.
        for func_id in recursive_placed {
            frame_relative.insert(func_id, static_span);
            static_span += self.emitted_frame_size(module, func_id);
        }

        let layout = |max_entry_end: u64| {
            if placed.is_empty() {
                (max_entry_end, max_entry_end)
            } else {
                let start = max_entry_end.max(low_memory_end);
                (start, start + static_span)
            }
        };
        let reachable_static_spans: FxHashMap<FunctionId, u64> = self
            .runtime_entry_reachability
            .iter()
            .map(|(&entry, reachable)| {
                let span = placed
                    .iter()
                    .copied()
                    .filter(|&func_id| reachable.contains(func_id))
                    .map(|func_id| {
                        frame_relative[&func_id] + self.emitted_frame_size(module, func_id)
                    })
                    .max()
                    .unwrap_or(0);
                (entry, span)
            })
            .collect();
        let heap_prefix_returns = Self::heap_prefix_return_offsets(module);
        let reachable_heap_prefix_guards: FxHashMap<FunctionId, u64> = self
            .runtime_entry_reachability
            .iter()
            .map(|(&entry, reachable)| {
                let guard = reachable
                    .iter()
                    .map(|func_id| {
                        Self::heap_prefix_guard(&module.functions[func_id], &heap_prefix_returns)
                    })
                    .max()
                    .unwrap_or(0);
                (entry, guard)
            })
            .collect();
        let free_memory_floor =
            |entry: FunctionId, entry_ends: &FxHashMap<FunctionId, u64>, region_start: u64| {
                let mut floor = entry_ends.get(&entry).copied().unwrap_or(low_memory_end);
                if let Some(&span) = reachable_static_spans.get(&entry)
                    && span != 0
                {
                    floor = floor.max(region_start + span);
                }
                if let Some(&guard) = reachable_heap_prefix_guards.get(&entry) {
                    floor = floor.checked_add(guard).expect("runtime heap prefix overflow");
                }
                floor.max(low_memory_end)
            };

        // Prefer eligible allocations before each entry's exact spill area,
        // then fall back to appending them after spills when only spill pushes
        // prevent the lower placement.
        // Entries overlay because only one runtime entry executes per call.
        // Reject any proposal that widens a shared heap/static-frame or
        // ranked-spill push.
        let mut static_alloc_sizes: FxHashMap<FunctionId, u64> = FxHashMap::default();
        let mut post_spill_entries = FxHashSet::default();
        for func_id in runtime_entries {
            let Some(allocations) = self.pending_static_allocs.remove(&func_id) else { continue };
            for (alloc, size) in allocations {
                let current_static_size = static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
                let proposed_static_size = current_static_size + size;
                let current_end = entry_ends[&func_id];
                let proposed_end = current_end + size;
                let before_max = entry_ends.values().copied().max().unwrap_or(0);
                let after_max = entry_ends
                    .iter()
                    .map(|(&entry, &end)| if entry == func_id { proposed_end } else { end })
                    .max()
                    .unwrap_or(proposed_end);
                let (before_start, _) = layout(before_max);
                let (after_start, _) = layout(after_max);

                let mut addresses = Vec::with_capacity(self.static_frame_addr_consts.len() + 1);
                for &entry in self.runtime_free_memory_consts.keys() {
                    addresses.push(RelayoutAddress {
                        before: free_memory_floor(entry, &entry_ends, before_start),
                        after: free_memory_floor(entry, &entry_ends, after_start),
                        references: 1,
                    });
                }
                addresses.extend(self.static_frame_addr_consts.iter().map(
                    |(&(static_func, offset), &(_, references))| {
                        let relative = frame_relative[&static_func] + offset;
                        RelayoutAddress {
                            before: before_start + relative,
                            after: after_start + relative,
                            references,
                        }
                    },
                ));
                let global_width_neutral = preserves_push_width(addresses.iter().copied());
                let spills_width_neutral =
                    self.external_spill_addr_consts.get(&func_id).is_none_or(|spills| {
                        let base = entry_bases[&func_id];
                        preserves_push_width(spills.iter().enumerate().map(
                            |(rank, &(_, references))| {
                                let offset = rank as u64 * WORD_BYTES as u64;
                                RelayoutAddress {
                                    before: base + current_static_size + offset,
                                    after: base + proposed_static_size + offset,
                                    references,
                                }
                            },
                        ))
                    });

                if global_width_neutral
                    && spills_width_neutral
                    && !post_spill_entries.contains(&func_id)
                {
                    let static_address = entry_bases[&func_id] + current_static_size;
                    self.asm.set_deferred_alloc_static(alloc, U256::from(static_address));
                    entry_ends.insert(func_id, proposed_end);
                    static_alloc_sizes.insert(func_id, proposed_static_size);
                } else if global_width_neutral {
                    // If inserting before spills would widen one of their
                    // pushes, append after the exact spill area instead. Once
                    // an entry uses this suffix, later allocations must stay
                    // there so already-emitted static addresses never move.
                    self.asm.set_deferred_alloc_static(alloc, U256::from(current_end));
                    entry_ends.insert(func_id, proposed_end);
                    post_spill_entries.insert(func_id);
                } else {
                    self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
                }
            }
        }

        // A retained candidate should always belong to an emitted external
        // entry. Lower defensively to the dynamic form if an unusual pipeline
        // shape leaves one behind.
        for (_, allocations) in self.pending_static_allocs.drain() {
            for (alloc, size) in allocations {
                self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
            }
        }

        for (func_id, spills) in self.external_spill_addr_consts.drain() {
            let base =
                entry_bases[&func_id] + static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
            for (rank, (id, _)) in spills.into_iter().enumerate() {
                self.asm.set_deferred_const(id, U256::from(base + rank as u64 * WORD_BYTES as u64));
            }
        }

        let max_entry_end = entry_ends.values().copied().max().unwrap_or(0);
        let (region_start, _) = layout(max_entry_end);
        let mut frame_bases = placed
            .iter()
            .map(|&func| (func, region_start + frame_relative[&func]))
            .collect::<FxHashMap<_, _>>();
        // Refine the global region after allocation placement. An unrelated entry's high
        // spill or assembly-memory bound should not raise every helper's addresses. Relax
        // absolute ends from each actual entry; shared callees must satisfy every caller.
        // Recursive scratch frames retain the established disjoint-prefix convention.
        if recursive_span == 0 {
            // frame_base(callee) >= frame_base(caller) + frame_size(caller)
            // frame_base(first_helper) >= entry_end
            let mut bounds = entry_ends.clone();
            let mut converged = false;
            for _ in 0..=module.functions.len() {
                let mut changed = false;
                for &(caller, callee) in &edges {
                    if let Some(&base) = bounds.get(&caller) {
                        let end = base.max(low_memory_end)
                            + if self.static_frame_functions.contains(caller) {
                                self.emitted_frame_size(module, caller)
                            } else {
                                0
                            };
                        if end > bounds.get(&callee).copied().unwrap_or(0) {
                            bounds.insert(callee, end);
                            changed = true;
                        }
                    }
                }
                if !changed {
                    converged = true;
                    break;
                }
            }
            if converged
                && placed
                    .iter()
                    .all(|func| bounds.get(func).is_none_or(|base| *base <= frame_bases[func]))
            {
                for (&func, base) in &mut frame_bases {
                    if let Some(&bound) = bounds.get(&func) {
                        *base = bound.max(low_memory_end);
                    }
                }
            }
        }
        // frame[offset] -> absolute(frame_base + offset)
        for (&(func_id, offset), &(id, _)) in &self.static_frame_addr_consts {
            self.asm.set_deferred_const(id, U256::from(frame_bases[&func_id] + offset));
        }
        let free_memory_floors: FxHashMap<FunctionId, u64> = self
            .runtime_free_memory_consts
            .keys()
            .copied()
            .map(|entry| {
                let static_end = self
                    .runtime_entry_reachability
                    .get(&entry)
                    .into_iter()
                    .flat_map(|reachable| reachable.iter())
                    .filter_map(|func| {
                        frame_bases
                            .get(&func)
                            .map(|base| base + self.emitted_frame_size(module, func))
                    })
                    .max()
                    .unwrap_or(low_memory_end);
                let floor = entry_ends
                    .get(&entry)
                    .copied()
                    .unwrap_or(low_memory_end)
                    .max(static_end)
                    .checked_add(reachable_heap_prefix_guards.get(&entry).copied().unwrap_or(0))
                    .expect("runtime heap prefix overflow");
                (entry, floor.max(low_memory_end))
            })
            .collect();
        for (entry, id) in self.runtime_free_memory_consts.drain() {
            let floor = free_memory_floors[&entry];
            self.asm.set_deferred_const(id, U256::from(floor));
        }
        self.runtime_entry_reachability.clear();
    }

    fn external_spill_base(
        func: &Function,
        dynamic_frames_enabled: bool,
        reachable_memory_mark: u64,
    ) -> u64 {
        let low_memory_start = if dynamic_frames_enabled && Self::uses_internal_frame_slot(func) {
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        let base =
            low_memory_start + func.internal_frame_size.max(func.external_static_return_size);
        // Hand-written assembly may own low memory above the compiler's own
        // frame through constant addresses; spill only above everything it
        // names, so a reload never reads a byte of the user's image and a
        // store never lands inside it.
        let mark = Self::constant_memory_high_water_mark(func).max(reachable_memory_mark);
        base.max(mark.next_multiple_of(EvmMemoryLayout::WORD_SIZE))
    }

    /// Returns the working-memory prefix a hand-written heap image needs.
    ///
    /// Static frames end where the runtime heap begins. Creation-code builders
    /// such as CWIA intentionally save, write, and restore words immediately
    /// before a `bytes` object, then consume that prefix with `create2` or
    /// `keccak256`. Reserve the largest constant backward offset for entries
    /// that reach such a builder so its temporary image cannot overlap the
    /// highest static-frame spill slots.
    fn heap_prefix_guard(func: &Function, returned_offsets: &FxHashMap<FunctionId, u64>) -> u64 {
        func.instructions()
            .filter_map(|inst_id| {
                let offset = match func.inst(inst_id).kind {
                    InstKind::Keccak256(offset, _)
                    | InstKind::Create(_, offset, _)
                    | InstKind::Create2(_, offset, _, _)
                    | InstKind::Call { args_offset: offset, .. }
                    | InstKind::CallCode { args_offset: offset, .. }
                    | InstKind::StaticCall { args_offset: offset, .. }
                    | InstKind::DelegateCall { args_offset: offset, .. } => Some(offset),
                    _ => None,
                }?;
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                Self::heap_prefix_offset(func, offset, returned_offsets, &mut visiting, &mut memo)
            })
            .max()
            .unwrap_or(0)
            .next_multiple_of(EvmMemoryLayout::WORD_SIZE)
    }

    /// Computes the largest backward heap offset returned by each helper.
    fn heap_prefix_return_offsets(module: &Module) -> FxHashMap<FunctionId, u64> {
        let mut offsets = FxHashMap::default();
        for _ in 0..module.functions.len() {
            let mut changed = false;
            for (func_id, func) in module.functions.iter_enumerated() {
                let mut offset = offsets.get(&func_id).copied().unwrap_or(0);
                for block in &func.blocks {
                    let Some(Terminator::Return { values }) = &block.terminator else { continue };
                    for &value in values {
                        let mut visiting = DenseBitSet::new_empty(func.num_values());
                        let mut memo = FxHashMap::default();
                        if let Some(returned) = Self::heap_prefix_offset(
                            func,
                            value,
                            &offsets,
                            &mut visiting,
                            &mut memo,
                        ) {
                            offset = offset.max(returned);
                        }
                    }
                }
                if offset > offsets.get(&func_id).copied().unwrap_or(0) {
                    offsets.insert(func_id, offset);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        offsets
    }

    /// Returns how far `value` can point before its underlying heap object.
    fn heap_prefix_offset(
        func: &Function,
        value: ValueId,
        returned_offsets: &FxHashMap<FunctionId, u64>,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, u64>,
    ) -> Option<u64> {
        if let Some(&offset) = memo.get(&value) {
            return Some(offset);
        }
        if !visiting.insert(value) {
            return None;
        }
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_prefix_offset(func, value, returned_offsets, visiting, memo)
        };
        let offset = match func.value(value) {
            Value::Arg(_) if func.value_ty(value).is_some_and(MirType::is_memory_reference) => {
                Some(0)
            }
            Value::Inst(inst_id) => match &func.inst(*inst_id).kind {
                InstKind::Fmp | InstKind::Alloc { .. } => Some(0),
                InstKind::MLoad(address)
                    if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    Some(0)
                }
                InstKind::ICall { function, returns: 1, .. } => {
                    returned_offsets.get(function).copied()
                }
                InstKind::Sub(base, amount) => {
                    let base = derive(*base, visiting, memo).or_else(|| {
                        func.value_ty(*base).is_some_and(MirType::is_memory_reference).then_some(0)
                    })?;
                    base.checked_add(func.value_u64(*amount)?)
                }
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .map(|&(_, incoming)| derive(incoming, visiting, memo))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .max(),
                InstKind::Select(_, then_value, else_value) => Some(
                    derive(*then_value, visiting, memo)?.max(derive(*else_value, visiting, memo)?),
                ),
                _ if func.value_ty(value).is_some_and(MirType::is_memory_reference) => Some(0),
                _ => None,
            },
            _ => None,
        };
        visiting.remove(value);
        if let Some(offset) = offset {
            memo.insert(value, offset);
        }
        offset
    }

    /// Returns the highest end address of any memory access in `func` whose
    /// offset and size are both compile-time constants, or zero without one.
    ///
    /// A routine that assembles an image at fixed low addresses legally uses
    /// the memory the spill area would otherwise occupy: the ERC-6551 registry
    /// lays its proxy initcode out at `[0x55, 0x10c)` with
    /// `calldatacopy(0x8c, 0x24, 0x80)` and reads it back through
    /// `create2(0, 0x55, 0xb7, salt)`, so a spill slot at `0xa0` ends up in
    /// the deployed footer. Reads count as well as writes. The compiler's own
    /// absolute accesses (the external return buffer, frame locals) never
    /// exceed the base they are placed under, so they never raise it. Ranges
    /// starting at or above `SPILL_HAZARD_BOUND` above `HEAP_START` are not low
    /// memory and are ignored. A range that starts below the bound still owns
    /// its complete extent, even when its end lies above the bound.
    pub(in crate::backend::evm::codegen) fn constant_memory_high_water_mark(
        func: &Function,
    ) -> u64 {
        let bound = EvmMemoryLayout::HEAP_START + SPILL_HAZARD_BOUND;
        let end_of = |offset: ValueId, size: u64| -> Option<u64> {
            let start = func.value_u64(offset)?;
            let end = start.checked_add(size)?;
            (start < bound).then_some(end)
        };
        let sized_end = |offset: ValueId, size: ValueId| end_of(offset, func.value_u64(size)?);
        let mut mark = 0;
        for inst_id in func.instructions() {
            let end = match func.inst(inst_id).kind {
                InstKind::MLoad(addr) | InstKind::MStore(addr, _) => {
                    end_of(addr, EvmMemoryLayout::WORD_SIZE)
                }
                InstKind::MStore8(addr, _) => end_of(addr, 1),
                InstKind::MCopy(dest, src, size) => sized_end(dest, size).max(sized_end(src, size)),
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::DataCopy(_, dest, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size)
                | InstKind::ExtCodeCopy(_, dest, _, size)
                | InstKind::Keccak256(dest, size)
                | InstKind::Log0(dest, size)
                | InstKind::Log1(dest, size, _)
                | InstKind::Log2(dest, size, _, _)
                | InstKind::Log3(dest, size, _, _, _)
                | InstKind::Log4(dest, size, _, _, _, _)
                | InstKind::Create(_, dest, size)
                | InstKind::Create2(_, dest, size, _) => sized_end(dest, size),
                InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                    sized_end(args_offset, args_size).max(sized_end(ret_offset, ret_size))
                }
                _ => None,
            };
            mark = mark.max(end.unwrap_or(0));
        }
        for block in func.blocks.iter() {
            if let Some(
                Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size },
            ) = &block.terminator
            {
                mark = mark.max(sized_end(*offset, *size).unwrap_or(0));
            }
        }
        mark
    }

    pub(in crate::backend::evm::codegen) fn constructor_spill_base(
        &self,
        immutable_count: usize,
    ) -> u64 {
        immutable_staging_end(self.immutable_staging_base, immutable_count)
    }

    pub(in crate::backend::evm::codegen) fn constructor_fixed_memory_end(
        &self,
        immutable_count: usize,
        spill_size: u64,
    ) -> u64 {
        self.constructor_spill_base(immutable_count)
            .checked_add(spill_size)
            .expect("constructor spill area overflow")
    }

    fn uses_internal_frame_slot(func: &Function) -> bool {
        func.instructions().any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::ICall { .. }))
    }

    pub(in crate::backend::evm::codegen) fn emit_entry_free_memory_start(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        entry: FunctionId,
    ) {
        let mut reachable = call_graph.reachable_callees_from([entry]);
        reachable.insert(entry);
        self.runtime_entry_reachability.insert(entry, reachable.clone());
        let needs_free_memory = reachable.iter().any(|func_id| {
            call_graph.is_recursive(func_id)
                || Self::function_may_observe_free_memory_slot(&module.functions[func_id])
                || module.functions[func_id].instructions().any(|inst_id| {
                    matches!(
                        module.functions[func_id].inst(inst_id).kind,
                        InstKind::ICall { function, returns, .. }
                            if returns > 1 || !self.static_frame_functions.contains(function)
                    )
                })
        });
        if !needs_free_memory {
            return;
        }

        let id = self.asm.new_deferred_const();
        self.asm.emit_push_deferred(id);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.asm.emit_op(op::MSTORE);
        self.runtime_free_memory_consts.insert(entry, id);
    }

    pub(in crate::backend::evm::codegen) fn emit_spill_slot_addr(
        &mut self,
        func: &Function,
        slot: SpillSlot,
    ) {
        if self.in_internal_function {
            self.emit_own_frame_addr(self.internal_spill_slot_offset(func, slot));
        } else {
            self.emit_spill_slot_addr_untracked(func, slot);
        }
    }

    fn emit_spill_slot_addr_untracked(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            self.emit_own_frame_addr_untracked(self.internal_spill_slot_offset(func, slot));
        } else if self.in_constructor {
            let spill_addr = self.constructor_spill_base(self.immutable_encodings.len())
                + u64::from(slot.offset) * EvmMemoryLayout::WORD_SIZE;
            self.asm.emit_push(U256::from(spill_addr));
        } else {
            // Route the address through a deferred constant and count the
            // reference; `assign_ranked_spill_addrs` renumbers the body's
            // slots hottest-first when it completes.
            let key = u64::from(slot.offset);
            let id = if let Some(entry) = self.spill_addr_consts.get_mut(&key) {
                entry.1 += 1;
                entry.0
            } else {
                let id = self.asm.new_deferred_const();
                self.spill_addr_consts.insert(key, (id, 1));
                id
            };
            self.asm.emit_push_deferred(id);
        }
    }

    pub(in crate::backend::evm::codegen) fn emit_spill_load(
        &mut self,
        func: &Function,
        slot: SpillSlot,
    ) {
        let (block, index) = self.asm.next_instruction_position();
        self.spill_loads.push((slot, block, index));
        self.emit_spill_slot_addr_untracked(func, slot);
        self.asm.emit_op(op::MLOAD);
    }

    fn internal_spill_slot_offset(&self, func: &Function, slot: SpillSlot) -> u64 {
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE
            + (func.returns.len() as u64) * EvmMemoryLayout::WORD_SIZE
            + func.internal_frame_size
            + u64::from(slot.offset) * EvmMemoryLayout::WORD_SIZE
    }

    /// Ranks the external body's spill slots by reference count, hottest
    /// first, so the most reloaded slots receive the shortest addresses after
    /// final layout. The ranking is a bijection over the same slot area —
    /// every site of a slot goes through one deferred constant — so sizes and
    /// disjointness are unchanged.
    pub(in crate::backend::evm::codegen) fn assign_ranked_spill_addrs(
        &mut self,
        func_id: FunctionId,
    ) {
        if self.spill_addr_consts.is_empty() {
            return;
        }
        let mut slots: Vec<(u64, (DeferredConst, usize))> =
            self.spill_addr_consts.drain().collect();
        slots.sort_unstable_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
        self.external_spill_addr_consts
            .insert(func_id, slots.into_iter().map(|(_, deferred)| deferred).collect());
    }

    pub(in crate::backend::evm::codegen) fn emit_internal_arg_load(&mut self, index: ArgIdx) {
        self.emit_own_frame_addr_untracked(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_op(op::MLOAD);
    }
}
