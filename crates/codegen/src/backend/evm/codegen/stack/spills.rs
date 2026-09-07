//! Spill reservation, live ranges, stores, and reload availability.

use super::super::{
    BlockId, CfgInfo, CopyDest, CopySource, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function,
    FunctionId, FxHashMap, FxHashSet, IndexVec, InstKind, Liveness, MAX_STACK_ACCESS, OnceCell,
    OptimizationMode, ParallelCopy, ScheduledOp, SmallVec, SpillSlot, SpillStore, StackOp,
    StdEntry, Terminator, U256, Value, ValueId, cross_block_values, index_vec, ir,
    is_cross_block_recomputable_kind, is_rematerializable_leaf, op, rematerializable_nullary_value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend::evm::codegen) struct SpillLiveRange {
    pub(in crate::backend::evm::codegen) start: usize,
    pub(in crate::backend::evm::codegen) end: usize,
}

pub(in crate::backend::evm::codegen) struct SpillColor {
    pub(in crate::backend::evm::codegen) values: DenseBitSet<ValueId>,
    pub(in crate::backend::evm::codegen) ranges: FxHashMap<BlockId, SmallVec<[SpillLiveRange; 4]>>,
}

type SpillInterferences = FxHashMap<ValueId, SmallVec<[ValueId; 4]>>;

impl SpillColor {
    pub(in crate::backend::evm::codegen) fn new(value_count: usize) -> Self {
        Self { values: DenseBitSet::new_empty(value_count), ranges: FxHashMap::default() }
    }

    pub(in crate::backend::evm::codegen) fn accepts(
        &self,
        value: ValueId,
        ranges: &FxHashMap<BlockId, SpillLiveRange>,
        interferences: &SpillInterferences,
    ) -> bool {
        interferences
            .get(&value)
            .is_none_or(|conflicts| !conflicts.iter().any(|&other| self.values.contains(other)))
            && ranges.iter().all(|(block, candidate)| {
                self.ranges.get(block).is_none_or(|assigned| {
                    assigned
                        .iter()
                        .all(|range| candidate.end < range.start || range.end < candidate.start)
                })
            })
    }

    pub(in crate::backend::evm::codegen) fn insert(
        &mut self,
        value: ValueId,
        ranges: &FxHashMap<BlockId, SpillLiveRange>,
    ) {
        self.values.insert(value);
        for (&block, &range) in ranges {
            self.ranges.entry(block).or_default().push(range);
        }
    }
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Preallocates stable spill slots for values that may cross block boundaries.
    ///
    /// Blocks are emitted in layout order, not necessarily dominance order, so a block can be
    /// emitted before the predecessor that stores one of its live-in values. Reserving the slot up
    /// front lets the later load use a stable memory location; stores still happen only when the
    /// value is actually available on the stack.
    pub(in crate::backend::evm::codegen) fn preallocate_cross_block_spills(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
    ) {
        let cross_block_live =
            cross_block_live.get_or_init(|| Self::cross_block_live_values(func, liveness));
        let values = Self::cross_block_spill_values(func, cross_block_live);

        // Coloring minimizes the local frame, which reduces memory expansion in gas mode. It is
        // deliberately disabled in size mode because renumbering spill addresses disturbed
        // downstream block sharing and regressed aggregate CI bytecode despite smaller frames.
        if self.gcx.sess.opts.optimization.is_gas() {
            let colorable = cross_block_live;
            let recomputable =
                cross_block_values(func, |value| !self.scheduler.is_stack_only_value(value));
            let ranges = Self::spill_live_ranges(func, liveness, colorable, &recomputable);
            let interferences =
                Self::parallel_phi_interferences(func, liveness, colorable, &self.block_copies);

            let mut colors = Vec::<SpillColor>::new();
            for value in colorable {
                let value_ranges = &ranges[value];
                let color = colors
                    .iter()
                    .position(|color| color.accepts(value, value_ranges, &interferences))
                    .unwrap_or_else(|| {
                        colors.push(SpillColor::new(func.num_values()));
                        colors.len() - 1
                    });
                colors[color].insert(value, value_ranges);
                self.scheduler.spills.reserve_at(value, color as u32);
            }

            for value in &values {
                if !colorable.contains(value) {
                    self.scheduler.spills.reserve(value);
                }
            }
        } else {
            for value in &values {
                self.scheduler.spills.reserve(value);
            }
        }

        self.preallocate_spill_metadata(func, &values);

        // A free-memory-pointer load cannot be recomputed after the pointer moves. Reserve stable
        // slots for cross-block values, including direct uses that liveness does not carry. Size
        // mode keeps every FMP slot stable because block-local reuse can increase output size.
        let reserve_all = matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size);
        let reloaded = Self::cross_block_reload_values(func);
        for val in Self::fmp_load_values(func) {
            if reserve_all || values.contains(val) || reloaded.contains(val) {
                self.scheduler.spills.reserve(val);
                self.scheduler.spills.mark_reloadable(val);
            }
        }
    }

    fn preallocate_spill_metadata(&mut self, func: &Function, values: &DenseBitSet<ValueId>) {
        let recomputable =
            cross_block_values(func, |value| !self.scheduler.is_stack_only_value(value));
        for val in values {
            if recomputable.contains(val) {
                self.scheduler.spills.mark_recomputable(val);
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn cross_block_live_values(
        func: &Function,
        liveness: &Liveness,
    ) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.num_values());
        for block in func.blocks.indices() {
            for value in liveness.live_in(block).iter().chain(liveness.live_out(block).iter()) {
                if matches!(func.value(value), crate::mir::Value::Inst(_)) {
                    values.insert(value);
                }
            }
        }
        values
    }

    /// Returns the per-block interval of each colorable value's slot, keyed by block.
    ///
    /// The interval spans the points where the slot has to hold the value: its live-in and
    /// live-out ends, the instructions that define and consume it, and the range of every value
    /// the scheduler may rebuild from it. A rebuild materializes its operands where it happens,
    /// which for an operand with a slot is a load from that slot, so an operand's slot has to
    /// survive as long as the rebuilt value's, not only until liveness drops the operand.
    fn spill_live_ranges(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
        recomputable: &DenseBitSet<ValueId>,
    ) -> IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>> {
        let mut ranges = index_vec![FxHashMap::default(); func.num_values()];
        let mut operands = SmallVec::<[ValueId; 8]>::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            for value in liveness.live_in(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, 0);
            }
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                operands.clear();
                func.inst(inst_id).kind.collect_operands(&mut operands);
                for &value in &operands {
                    Self::extend_spill_live_range(
                        &mut ranges,
                        colorable,
                        value,
                        block_id,
                        inst_idx * 2,
                    );
                }
                if let Some(value) = func.inst_result_value(inst_id) {
                    Self::extend_spill_live_range(
                        &mut ranges,
                        colorable,
                        value,
                        block_id,
                        inst_idx * 2 + 1,
                    );
                }
            }
            if let Some(terminator) = &block.terminator {
                let point = block.instructions.len() * 2;
                for value in terminator.operands() {
                    Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
                }
            }
            let point = block.instructions.len() * 2 + 1;
            for value in liveness.live_out(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
            }
        }

        Self::extend_recomputed_operand_ranges(func, colorable, recomputable, &mut ranges);
        ranges
    }

    /// Widens every operand's range over the range of the values rebuilt from it.
    ///
    /// A rebuild is only chosen where the rebuilt value is needed, so the rebuilt value's own
    /// range covers every point an operand can be read at. Rebuilding is transitive and passes
    /// through values that never own a slot themselves, so the requirement propagates over the
    /// whole recomputable operand graph and only lands on the colorable values at the end.
    fn extend_recomputed_operand_ranges(
        func: &Function,
        colorable: &DenseBitSet<ValueId>,
        recomputable: &DenseBitSet<ValueId>,
        ranges: &mut IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>>,
    ) {
        let mut required = ranges.clone();
        let mut operands = SmallVec::<[ValueId; 8]>::new();
        let mut worklist: Vec<ValueId> =
            recomputable.iter().filter(|&value| !required[value].is_empty()).collect();
        while let Some(value) = worklist.pop() {
            let crate::mir::Value::Inst(inst_id) = func.value(value) else { continue };
            operands.clear();
            func.inst(*inst_id).kind.collect_operands(&mut operands);
            let value_required = required[value].clone();
            for &operand in &operands {
                if !recomputable.contains(operand) && !colorable.contains(operand) {
                    continue;
                }
                let mut grew = false;
                for (&block, &range) in &value_required {
                    grew |= Self::merge_spill_live_range(&mut required[operand], block, range);
                }
                if grew && recomputable.contains(operand) {
                    worklist.push(operand);
                }
            }
        }

        for value in colorable.iter() {
            for (&block, &range) in &required[value] {
                Self::merge_spill_live_range(&mut ranges[value], block, range);
            }
        }
    }

    /// Unions `range` into a value's interval for `block`, reporting whether it grew.
    fn merge_spill_live_range(
        ranges: &mut FxHashMap<BlockId, SpillLiveRange>,
        block: BlockId,
        range: SpillLiveRange,
    ) -> bool {
        match ranges.entry(block) {
            StdEntry::Occupied(mut entry) => {
                let merged = SpillLiveRange {
                    start: entry.get().start.min(range.start),
                    end: entry.get().end.max(range.end),
                };
                let grew = merged != *entry.get();
                entry.insert(merged);
                grew
            }
            StdEntry::Vacant(entry) => {
                entry.insert(range);
                true
            }
        }
    }

    /// Records spill-slot conflicts introduced by simultaneous phi edge copies.
    ///
    /// The ordinary live ranges do not model the sequentialized copy schedule. Every destination
    /// must coexist at the successor, and a destination store must not alias a source that the
    /// schedule loads later. Sources already loaded before a store may safely share its slot.
    pub(in crate::backend::evm::codegen) fn parallel_phi_interferences(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
        block_copies: &FxHashMap<BlockId, Vec<ParallelCopy>>,
    ) -> SpillInterferences {
        let mut interferences = FxHashMap::default();
        for (block_id, copies) in block_copies {
            // Copies of a multi-successor predecessor execute before the
            // branch, on every outgoing edge. Splitting keeps phi results
            // read on sibling paths out of this position, but a destination
            // could still reuse the spill slot of an unrelated value that is
            // live only on a sibling edge; keep those apart.
            let sibling_live = func.blocks[*block_id].terminator.as_ref().and_then(|term| {
                let successors = term.successors();
                (successors.len() > 1).then_some(successors)
            });
            for (index, copy) in copies.iter().enumerate() {
                let CopyDest::Value(destination) = &copy.dst else { continue };
                if let Some(successors) = &sibling_live {
                    for &successor in successors {
                        for value in liveness.live_in(successor).iter() {
                            Self::add_spill_interference(
                                &mut interferences,
                                colorable,
                                *destination,
                                value,
                            );
                        }
                    }
                }
                for other in copies {
                    let CopyDest::Value(other_destination) = &other.dst else { continue };
                    Self::add_spill_interference(
                        &mut interferences,
                        colorable,
                        *destination,
                        *other_destination,
                    );
                }
                for later in &copies[index + 1..] {
                    let CopySource::Value(source) = &later.src else { continue };
                    Self::add_spill_interference(
                        &mut interferences,
                        colorable,
                        *destination,
                        *source,
                    );
                }
                // Interference is modeled from the sequentialized copy schedule
                // plus the terminator's operands below. Extending destinations
                // through the whole predecessor live-out set is provably safe
                // but was measured to cost 12% runtime gas on the LibString hot
                // workload by defeating slot reuse; the residual (a destination
                // sharing with a non-source value live only on a sibling edge)
                // has never been reproduced and is accepted deliberately.
                let own_source = match &copy.src {
                    CopySource::Value(source) => Some(*source),
                    _ => None,
                };
                // A value consumed only by this predecessor's terminator is not
                // live-out, but the copy stores execute before the terminator: a
                // destination sharing the condition's slot would clobber a
                // pending reload and take the wrong branch.
                if let Some(term) =
                    func.blocks.get(*block_id).and_then(|block| block.terminator.as_ref())
                {
                    for operand in term.operands() {
                        if Some(operand) != own_source {
                            Self::add_spill_interference(
                                &mut interferences,
                                colorable,
                                *destination,
                                operand,
                            );
                        }
                    }
                }
            }
        }
        interferences
    }

    fn add_spill_interference(
        interferences: &mut SpillInterferences,
        colorable: &DenseBitSet<ValueId>,
        lhs: ValueId,
        rhs: ValueId,
    ) {
        if lhs == rhs || !colorable.contains(lhs) || !colorable.contains(rhs) {
            return;
        }
        for (value, conflict) in [(lhs, rhs), (rhs, lhs)] {
            let conflicts = interferences.entry(value).or_default();
            if !conflicts.contains(&conflict) {
                conflicts.push(conflict);
            }
        }
    }

    fn extend_spill_live_range(
        ranges: &mut IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>>,
        colorable: &DenseBitSet<ValueId>,
        value: ValueId,
        block: BlockId,
        point: usize,
    ) {
        if !colorable.contains(value) {
            return;
        }
        ranges[value]
            .entry(block)
            .and_modify(|range| {
                range.start = range.start.min(point);
                range.end = range.end.max(point);
            })
            .or_insert(SpillLiveRange { start: point, end: point });
    }

    /// Returns values directly consumed outside their defining block. Phi inputs are edge uses:
    /// codegen consumes them in the predecessor or carries them on the edge, so they do not need a
    /// reload route under the source value's identity.
    pub(in crate::backend::evm::codegen) fn cross_block_reload_values(
        func: &Function,
    ) -> DenseBitSet<ValueId> {
        let mut definitions =
            IndexVec::<ValueId, Option<BlockId>>::from_vec(vec![None; func.num_values()]);
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let Some(result) = func.inst_result_value(inst_id) {
                    definitions[result] = Some(block_id);
                }
            }
        }

        let mut reloaded = DenseBitSet::new_empty(func.num_values());
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                    continue;
                }
                for operand in func.inst(inst_id).kind.operands() {
                    if definitions[operand].is_some_and(|definition| definition != block_id) {
                        reloaded.insert(operand);
                    }
                }
            }
            if let Some(terminator) = &func.blocks[block_id].terminator {
                for operand in terminator.operands() {
                    if definitions[operand].is_some_and(|definition| definition != block_id) {
                        reloaded.insert(operand);
                    }
                }
            }
        }
        reloaded
    }

    /// Every live free-memory-pointer load result in the function.
    fn fmp_load_values(func: &Function) -> Vec<ValueId> {
        let mut values = Vec::new();
        for inst_id in func.instructions() {
            if matches!(
                func.inst(inst_id).kind,
                InstKind::MLoad(addr)
                    if func.value_u64(addr) == Some(EvmMemoryLayout::FMP_SLOT)
            ) && let Some(val) = func.inst_result_value(inst_id)
            {
                values.push(val);
            }
        }
        values
    }

    fn cross_block_spill_values(
        func: &Function,
        cross_block_live: &DenseBitSet<ValueId>,
    ) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.num_values());
        for value in cross_block_live {
            if Self::can_own_spill_slot(func, value)
                || Self::is_always_rematerializable_value(func, value)
            {
                values.insert(value);
            }
        }
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                    && let Some(val) = func.inst_result_value(inst_id)
                {
                    values.insert(val);
                }
            }
        }
        values
    }

    pub(in crate::backend::evm::codegen) fn is_cross_block_recomputable_inst(
        func: &Function,
        value: ValueId,
    ) -> bool {
        let Value::Inst(inst_id) = func.value(value) else { return false };
        is_cross_block_recomputable_kind(&func.inst(*inst_id).kind)
    }

    /// Spills stack-resident values a successor reads under their own identity.
    /// Phi inputs are consumed by their predecessor edge copies.
    pub(in crate::backend::evm::codegen) fn spill_live_out_values(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
    ) {
        self.spill_live_out_values_except(func, liveness, block_id, &[]);
    }

    pub(in crate::backend::evm::codegen) fn spill_live_out_values_except(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        exempt: &[ValueId],
    ) {
        let mut exempt_values = DenseBitSet::new_empty(func.num_values());
        for &value in exempt {
            exempt_values.insert(value);
        }
        let successors = func.blocks[block_id]
            .terminator
            .as_ref()
            .map(Terminator::successors)
            .unwrap_or_default();
        for val in liveness.live_out(block_id) {
            if !exempt_values.contains(val)
                && successors.iter().any(|&succ| liveness.live_in(succ).contains(val))
            {
                self.spill_value_if_needed(func, val);
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn pop_stack_values_not_needed_by(
        &mut self,
        needed: &[ValueId],
    ) {
        while let Some(depth) = self.first_stack_value_not_needed_by(needed) {
            if depth > 0 {
                assert!(
                    depth <= self.stack_access_limit(),
                    "resident stack discard exceeded SWAP reach"
                );
                self.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.emit_stack_op(StackOp::Pop);
        }
    }

    pub(in crate::backend::evm::codegen) fn first_stack_value_not_needed_by(
        &self,
        needed: &[ValueId],
    ) -> Option<usize> {
        let mut remaining = Self::value_counts(needed.iter().copied());
        for (depth, slot) in self.scheduler.stack.iter().enumerate() {
            let Some(value) = slot else {
                return Some(depth);
            };
            let Some(count) = remaining.get_mut(&value) else {
                return Some(depth);
            };
            if *count == 0 {
                return Some(depth);
            }
            *count -= 1;
        }
        None
    }

    /// A phi defined in this block is a new loop iteration's value. A phi
    /// defined elsewhere retains its spill only when every incoming path has
    /// already established it.
    pub(in crate::backend::evm::codegen) fn invalidate_carried_phi_spills(
        &mut self,
        func: &Function,
        block_id: BlockId,
    ) {
        let carried: Vec<ValueId> = self.scheduler.stack.iter().flatten().collect();
        for value in carried {
            if let crate::mir::Value::Inst(inst_id) = func.value(value)
                && matches!(func.inst(*inst_id).kind, InstKind::Phi(_))
                && (func.blocks[block_id].instructions.contains(inst_id)
                    || self
                        .spill_available
                        .as_ref()
                        .is_none_or(|available| !available.contains(&value)))
            {
                self.scheduler.spills.invalidate_stored(value);
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn mark_live_in_spills(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
    ) {
        // Values already on the stack (carried in from a preserved predecessor
        // edge) are read directly; marking them reloadable would point at a
        // spill slot that may never have been stored.
        for val in liveness.live_in(block_id) {
            if !self.scheduler.stack.contains(val) && self.scheduler.spills.get(val).is_some() {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
        for &inst_id in &func.blocks[block_id].instructions {
            if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                && let Some(val) = func.inst_result_value(inst_id)
                && !self.scheduler.stack.contains(val)
                && self.scheduler.spills.get(val).is_some()
            {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn spill_values_before_stack_clear(
        &mut self,
        func: &Function,
        values: &[ValueId],
    ) {
        for &value in values {
            self.spill_value_if_needed(func, value);
        }
    }

    /// Parks stack-resident operands in their spill slots before an
    /// `emit_value_fresh` sequence. The sequence re-materializes each value,
    /// and definitions such as free-memory-pointer loads cannot be recomputed
    /// once memory has moved on: reaching them through a reload keeps the
    /// original definition.
    pub(in crate::backend::evm::codegen) fn prepare_fresh_operands(
        &mut self,
        func: &Function,
        operands: &[ValueId],
    ) {
        // The spill here is only a burial fallback: `emit_value_fresh` DUPs an
        // on-stack operand and recomputes a cheap one, reaching this spill copy
        // only if the operand sinks past DUP16 during argument emission. In a
        // forwarding proxy the call reads the low memory the spill area lives in
        // (a `delegatecall` whose args are `[0, calldatasize())`), so writing
        // the backup there corrupts the call's own input. Such a function keeps
        // its few operands stack-resident instead — a simple forwarder never
        // buries them.
        if !self.spill_hazard_insts.is_empty() {
            return;
        }
        for &operand in operands {
            self.spill_value_if_needed(func, operand);
        }
    }

    /// Duplicates a stack-only operand before earlier fresh operands bury it past `DUP` reach.
    /// `operands` are ordered deepest-first, exactly as the following emission sequence pushes
    /// them.
    pub(in crate::backend::evm::codegen) fn stage_stack_only_fresh_operands(
        &mut self,
        operands: &[ValueId],
    ) {
        if !self.scheduler.has_stack_only_values() {
            return;
        }
        let stack_access_limit = self.stack_access_limit();

        loop {
            let mut stack = self.scheduler.stack.clone();
            let mut inaccessible = None;
            for &operand in operands {
                if self.scheduler.is_stack_only_value(operand) {
                    match stack.find(operand) {
                        Some(depth) if depth < stack_access_limit => {
                            stack.dup((depth + 1) as u8);
                        }
                        _ => {
                            inaccessible = Some(operand);
                            break;
                        }
                    }
                } else {
                    stack.push_unknown();
                }
            }
            let Some(operand) = inaccessible else { break };
            let Some(depth) = self.scheduler.stack.find(operand) else {
                if self.recover_lost_internal_stack_value(operand) {
                    return;
                }
                panic!("stack-only CALL operand {operand:?} was lost before its use");
            };
            assert!(depth < stack_access_limit, "stack-only CALL operand exceeded DUP reach");
            self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
        }
    }

    pub(in crate::backend::evm::codegen) fn stack_access_limit(&self) -> usize {
        self.gcx.sess.opts.evm_version.reachable_stack_depth()
    }

    /// Returns true when `val` is reachable from a successor block: it is on the stack, it has a
    /// valid store, its slot is available on every emitted path into this block, or
    /// [`Self::spill_value_if_needed`] gives it no slot in the first place because it is
    /// stack-only, rematerializable, or reloadable from its argument address.
    pub(in crate::backend::evm::codegen) fn has_spill_home(
        &self,
        func: &Function,
        val: ValueId,
    ) -> bool {
        self.scheduler.stack.contains(val)
            || self.scheduler.spills.is_stored(val)
            || self.spill_store_available(val)
            || self.scheduler.is_stack_only_value(val)
            || !Self::can_own_spill_slot(func, val)
            || Self::is_reloadable_argument_address(func, val)
    }

    /// Returns true when `val`'s slot holds it on every emitted forward path
    /// into the current block.
    ///
    /// The scheduler's stored flag is one function-wide bit, so it is the
    /// weaker record of the two. A block that carries a value in on the stack
    /// while the slot is not available there clears the bit, which is right for
    /// that block but also forgets the store for the blocks whose predecessors
    /// all did write the slot. The store-availability intersection is the
    /// per-path record and still names the value there, so a cleared bit alone
    /// does not mean the value lost its memory home.
    fn spill_store_available(&self, val: ValueId) -> bool {
        self.scheduler.spills.get(val).is_some()
            && self.spill_available.as_ref().is_some_and(|available| available.contains(&val))
    }

    /// Returns whether every forward predecessor of `block`, which sits at `pos` in the emission
    /// order, was already emitted. Only then does a value live into `block` have to own a home
    /// already: a predecessor emitted later stores its live-out values when its own turn comes,
    /// which is later in the stream but earlier at runtime.
    pub(in crate::backend::evm::codegen) fn forward_predecessors_emitted(
        func: &Function,
        store_cfg: &CfgInfo,
        block_pos: &FxHashMap<BlockId, usize>,
        block: BlockId,
        pos: usize,
    ) -> bool {
        func.blocks[block].predecessors.iter().all(|&pred| {
            store_cfg.dominators().dominates(block, pred)
                || block_pos.get(&pred).is_some_and(|&pred_pos| pred_pos < pos)
        })
    }

    /// Spills an instruction result if it is on the stack and not already stored.
    pub(in crate::backend::evm::codegen) fn spill_value_if_needed(
        &mut self,
        func: &Function,
        val: ValueId,
    ) {
        if self.scheduler.is_stack_only_value(val) || !Self::can_own_spill_slot(func, val) {
            return;
        }
        if self.scheduler.should_recompute_unstored_spill(val)
            && Self::is_reloadable_argument_address(func, val)
        {
            return;
        }

        // `stored` is a global emission flag; a store emitted by a sibling
        // branch arm sets it without covering this path. Trust it only when
        // the store is available on every emitted path into this block. The
        // current availability set is updated whenever this block stores.
        if self.scheduler.spills.is_stored(val)
            && self.spill_available.as_ref().is_none_or(|avail| avail.contains(&val))
        {
            return;
        }

        if let Some(depth) = self.scheduler.stack.find(val) {
            let slot = self.scheduler.spills.allocate(val);
            if depth >= self.stack_access_limit() {
                self.spill_deep_stack_value(func, val, slot, depth);
                return;
            }

            self.spill_accessible_stack_value(func, val, slot, depth);
        }
    }

    fn is_reloadable_argument_address(func: &Function, value: ValueId) -> bool {
        let Value::Inst(inst_id) = func.value(value) else { return false };
        let InstKind::Add(left, right) = func.inst(*inst_id).kind else { return false };
        if !matches!(func.value(left), Value::Arg(_)) && !matches!(func.value(right), Value::Arg(_))
        {
            return false;
        }

        let mut store = false;
        let mut load = false;
        for inst_id in func.instructions() {
            match func.inst(inst_id).kind {
                InstKind::MStore(address, _) if address == value => store = true,
                InstKind::MLoad(address) if address == value => load = true,
                _ => {}
            }
        }
        store && load
    }

    pub(in crate::backend::evm::codegen) fn spill_value_to_reserved_slot(
        &mut self,
        func: &Function,
        val: ValueId,
    ) -> bool {
        if self.scheduler.is_stack_only_value(val)
            || Self::is_rematerializable_value(func, val)
            || Self::is_reloadable_argument_address(func, val)
            || self.scheduler.spills.get(val).is_none()
        {
            return false;
        }

        let Some(depth) = self.scheduler.stack.find(val) else {
            return false;
        };
        let slot = self.scheduler.spills.allocate(val);
        if depth >= self.stack_access_limit() {
            self.spill_deep_stack_value(func, val, slot, depth);
        } else {
            self.spill_accessible_stack_value(func, val, slot, depth);
        }
        true
    }

    pub(in crate::backend::evm::codegen) fn spill_reserved_result_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        // This is not the normal first-store path; `generate_inst` handles live-out results.
        // It repairs physical emission orders where a successor block emitted first has already
        // marked this reserved cross-block slot as stored/reloadable before the defining block
        // materializes the value.
        if self.scheduler.spills.get(value).is_none()
            || !self.scheduler.spills.is_stored(value)
            || liveness.is_dead_after(value, block, inst_idx)
        {
            return;
        }

        self.spill_value_to_reserved_slot(func, value);
    }

    pub(in crate::backend::evm::codegen) fn spill_accessible_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        debug_assert!(depth < self.stack_access_limit());

        // DUP the value to top of stack for storing.
        // We need to DUP (not just use ensure_on_top) because:
        // 1. If value is on top, ensure_on_top does nothing but we need a copy
        // 2. MSTORE will consume the value, and we want to preserve the original
        let (block, start) = self.asm.next_instruction_position();
        let dup_n = (depth + 1) as u8;
        self.emit_stack_op(StackOp::Dup(dup_n));

        self.store_stack_top_to_spill(func, val, slot);
        let (end_block, end) = self.asm.next_instruction_position();
        if end_block == block {
            self.spill_stores.push(SpillStore { value: val, slot, block, range: start..end });
        }
    }

    /// Drops stores of values that remain on the stack on every live branch
    /// arm. A later block stores the value again if it needs a memory home.
    pub(in crate::backend::evm::codegen) fn remove_dead_carried_spill_stores(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        preserved: &[BlockId],
    ) {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block_id].terminator.as_ref()
        else {
            return;
        };
        let successors = [*then_block, *else_block];
        let current_block = self.asm.next_instruction_position().0;
        let mut removals = Vec::new();
        self.spill_stores.retain(|store| {
            let defined_here = matches!(func.value(store.value), Value::Inst(inst)
                if func.blocks[block_id].instructions.contains(inst));
            let reloaded_here = self
                .spill_loads
                .iter()
                .any(|&(slot, block, _)| block == store.block && slot == store.slot);
            let remove = store.block == current_block
                && store.value != *condition
                && defined_here
                && !reloaded_here
                && self.scheduler.stack.contains(store.value)
                && successors.iter().all(|&successor| {
                    preserved.contains(&successor)
                        || !liveness.live_in(successor).contains(store.value)
                });
            if remove {
                removals.push(store.clone());
            }
            !remove
        });
        for store in &removals {
            if let Some((_, references)) =
                self.spill_addr_consts.get_mut(&u64::from(store.slot.offset))
            {
                *references = references.saturating_sub(1);
            }
            self.scheduler.spills.invalidate_stored(store.value);
            if let Some(available) = &mut self.spill_available {
                available.remove(&store.value);
            }
        }
        self.early_spill_removals
            .extend(removals.into_iter().map(|store| (store.block, store.range)));
    }

    pub(in crate::backend::evm::codegen) fn remove_dead_spill_stores(&mut self) {
        enum Event {
            Store(usize),
            Load(SpillSlot),
        }

        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        // A spill store is dead when every path either overwrites its slot before a reload or
        // leaves the function. The scheduler keeps these stores while forming blocks, then drops
        // them after their final control flow is known.
        let stores = std::mem::take(&mut self.spill_stores);
        let loads = std::mem::take(&mut self.spill_loads);
        if stores.is_empty() {
            return;
        }

        let mut events = FxHashMap::<ir::BlockId, Vec<(usize, Event)>>::default();
        for (index, store) in stores.iter().enumerate() {
            events.entry(store.block).or_default().push((store.range.start, Event::Store(index)));
        }
        for (slot, block, index) in loads {
            events.entry(block).or_default().push((index, Event::Load(slot)));
        }
        for events in events.values_mut() {
            events.sort_unstable_by_key(|&(index, _)| index);
        }

        let range = self.function_ir_block_start..self.asm.block_count();
        let mut successors = FxHashMap::<ir::BlockId, Vec<ir::BlockId>>::default();
        for (source, target) in self.asm.dataflow_edges(range.clone()) {
            successors.entry(source).or_default().push(target);
        }
        let blocks = range.map(ir::BlockId::from_usize).collect::<Vec<_>>();
        let mut live_in = FxHashMap::<ir::BlockId, FxHashSet<SpillSlot>>::default();
        loop {
            let mut changed = false;
            for &block in blocks.iter().rev() {
                let mut live = successors
                    .get(&block)
                    .into_iter()
                    .flatten()
                    .filter_map(|successor| live_in.get(successor))
                    .flatten()
                    .copied()
                    .collect::<FxHashSet<_>>();
                for (_, event) in events.get(&block).into_iter().flatten().rev() {
                    match event {
                        Event::Store(index) => {
                            live.remove(&stores[*index].slot);
                        }
                        Event::Load(slot) => {
                            live.insert(*slot);
                        }
                    }
                }
                if live_in.get(&block) != Some(&live) {
                    live_in.insert(block, live);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut dead = FxHashSet::default();
        for &block in &blocks {
            let mut live = successors
                .get(&block)
                .into_iter()
                .flatten()
                .filter_map(|successor| live_in.get(successor))
                .flatten()
                .copied()
                .collect::<FxHashSet<_>>();
            for (_, event) in events.get(&block).into_iter().flatten().rev() {
                match event {
                    Event::Store(index) if !live.remove(&stores[*index].slot) => {
                        dead.insert(*index);
                    }
                    Event::Store(_) => {}
                    Event::Load(slot) => {
                        live.insert(*slot);
                    }
                }
            }
        }
        let mut removals = dead
            .into_iter()
            .map(|index| &stores[index])
            .map(|store| {
                if let Some((_, references)) =
                    self.spill_addr_consts.get_mut(&u64::from(store.slot.offset))
                {
                    *references = references.saturating_sub(1);
                }
                (store.block, store.range.clone())
            })
            .collect::<Vec<_>>();
        removals.extend(std::mem::take(&mut self.early_spill_removals));
        self.asm.remove_instructions(&mut removals);
    }

    pub(in crate::backend::evm::codegen) fn spill_deep_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        let stack_access_limit = self.stack_access_limit();
        debug_assert!(depth >= stack_access_limit);

        let mut saved_above = Vec::with_capacity(depth + 1 - stack_access_limit);
        for _ in 0..(depth + 1 - stack_access_limit) {
            let Some(top) = self.scheduler.stack.top() else {
                panic!("cannot spill deep stack value {val:?}: untracked stack entry above it");
            };
            let restore = if let Some(op) = Self::always_rematerializable_op(func, top) {
                self.emit_stack_op(StackOp::Pop);
                ScheduledOp::RematerializeNullary(op)
            } else {
                let top_slot = self.scheduler.spills.allocate(top);
                if self.scheduler.reloadable_spill(top).is_some() {
                    self.emit_stack_op(StackOp::Pop);
                } else {
                    self.store_stack_top_to_spill(func, top, top_slot);
                }
                ScheduledOp::LoadSpill(top_slot)
            };
            saved_above.push((top, restore));
        }

        let Some(accessible_depth) = self.scheduler.stack.find(val) else {
            panic!("cannot spill deep stack value {val:?}: value disappeared while exposing it");
        };
        self.spill_accessible_stack_value(func, val, slot, accessible_depth);

        for (saved, restore) in saved_above.into_iter().rev() {
            let stack_depth = self.scheduler.depth();
            self.record_scheduled_ops_peak(stack_depth, std::slice::from_ref(&restore));
            self.emit_scheduled_ops(func, [restore]);
            self.scheduler.stack.push(saved);
        }
    }

    /// Establishes reload routes for dynamic-call arguments before the anonymous frame base adds
    /// one word above them. This is a fallback, not a stack-depth limit: accessible arguments keep
    /// their ordinary stack convention and arbitrarily deep layouts spill through memory.
    pub(in crate::backend::evm::codegen) fn materialize_deep_dynamic_call_args(
        &mut self,
        func: &Function,
        args: &[ValueId],
    ) {
        let stack_access_limit = self.stack_access_limit();
        for &arg in args {
            let Some(depth) = self.scheduler.stack.find(arg) else { continue };
            if depth + 1 < stack_access_limit
                || self.scheduler.reloadable_spill(arg).is_some()
                || Self::is_rematerializable_value(func, arg)
            {
                continue;
            }

            let slot = self.scheduler.spills.allocate(arg);
            if depth >= stack_access_limit {
                self.spill_deep_stack_value(func, arg, slot, depth);
            } else {
                self.spill_accessible_stack_value(func, arg, slot, depth);
            }
            self.scheduler.materialize_stack_only_value(arg);
        }
    }

    fn store_stack_top_to_spill(&mut self, func: &Function, value: ValueId, slot: SpillSlot) {
        // Store to spill slot: PUSH offset, MSTORE.
        // The PUSH creates an untracked stack entry, so we track it as unknown.
        self.emit_spill_slot_addr(func, slot);
        self.scheduler.stack.push_unknown();

        self.asm.emit_op(op::MSTORE);
        // MSTORE consumes 2 values: the untracked offset and the value being spilled.
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.spills.mark_stored(value);
        if let Some(available) = &mut self.spill_available {
            available.insert(value);
        }
    }

    /// Spills operands that are live-out before an instruction consumes them.
    /// This ensures cross-block values are preserved in memory.
    pub(in crate::backend::evm::codegen) fn spill_live_out_operands(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        operands: &[ValueId],
    ) {
        let live_out = liveness.live_out(block_id);

        for &op in operands {
            if live_out.contains(op) && !self.is_stack_phi_source(block_id, op) {
                self.spill_value_if_needed(func, op);
            }
        }
    }

    /// Values that are always re-emitted at each use instead of being kept on
    /// the stack or spilled.
    ///
    /// `Arg` MUST stay in this set. With static frames an argument reload is a
    /// 3-4 byte `PUSH addr; MLOAD`/`CALLDATALOAD`, cheaper than the spill
    /// traffic that tracking would create — and the spill machinery assumes
    /// arguments never own slots: making `Arg` non-rematerializable was
    /// measured to REGRESS every bench contract's size (erc20 +61 B, maple
    /// +72 B, fractional +127 B) and to break 4 of 8 bench harnesses at
    /// runtime. The one exception is a stack-only argument that sinks below
    /// the target's DUP reach: [`Self::emit_value_impl`] gives that otherwise stranded
    /// word a spill slot. Do not make ordinary frame-backed arguments own
    /// slots without redesigning argument spilling.
    pub(in crate::backend::evm::codegen) fn is_rematerializable_value(
        func: &Function,
        value: ValueId,
    ) -> bool {
        is_rematerializable_leaf(func.value(value))
    }

    pub(in crate::backend::evm::codegen) fn is_always_rematerializable_value(
        func: &Function,
        value: ValueId,
    ) -> bool {
        Self::always_rematerializable_op(func, value).is_some()
    }

    pub(in crate::backend::evm::codegen) fn always_rematerializable_op(
        func: &Function,
        value: ValueId,
    ) -> Option<u8> {
        rematerializable_nullary_value(func, value)
    }

    pub(in crate::backend::evm::codegen) fn can_own_spill_slot(
        func: &Function,
        value: ValueId,
    ) -> bool {
        matches!(func.value(value), crate::mir::Value::Inst(_))
            && !Self::is_always_rematerializable_value(func, value)
    }

    /// Returns true when `value` needs no spill before the instruction that
    /// is about to consume it: it owns no reserved cross-block slot, it is
    /// not live out of the block, and more stack copies exist at this point
    /// than the instruction will consume net of the emissions still to come
    /// (`consumed`). Later in-block uses DUP the survivor, or deep-spill it
    /// on demand if it sinks past the target's `DUP` reach, so skipping the store
    /// cannot strand the value and adds no stack depth.
    pub(in crate::backend::evm::codegen) fn block_local_copy_survives(
        &self,
        liveness: &Liveness,
        block: BlockId,
        value: ValueId,
        consumed: usize,
    ) -> bool {
        self.scheduler.spills.get(value).is_none()
            && !liveness.live_out(block).contains(value)
            && self.scheduler.stack.iter().flatten().filter(|&v| v == value).count() > consumed
    }

    pub(in crate::backend::evm::codegen) fn spill_top_value_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        if self.scheduler.is_stack_only_value(value) || Self::is_rematerializable_value(func, value)
        {
            return;
        }

        let has_reserved_cross_block_slot = self.scheduler.spills.get(value).is_some();
        if liveness.is_dead_after(value, block, inst_idx) && !has_reserved_cross_block_slot {
            return;
        }

        debug_assert_eq!(self.scheduler.stack.top(), Some(value));
        if !self.spill_value_to_reserved_slot(func, value) {
            self.spill_value_if_needed(func, value);
        }
        if has_reserved_cross_block_slot && !Self::is_reloadable_argument_address(func, value) {
            assert!(
                self.scheduler.reloadable_spill(value).is_some(),
                "reserved operand {value:?} was not stored before consumption in `{}`",
                func.name
            );
        }
    }

    /// Keeps stack-only operands alive when an instruction is emitted without an operand plan.
    /// Planned operations preserve these values as part of the plan itself, so doing this before
    /// every instruction duplicates both liveness queries and stack scans on the hot path.
    pub(in crate::backend::evm::codegen) fn preserve_stack_only_operands(
        &mut self,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if !self.scheduler.has_stack_only_values() {
            return;
        }

        let mut uses = SmallVec::<[(ValueId, usize); 4]>::new();
        for &operand in operands {
            if self.scheduler.is_stack_only_value(operand) {
                if let Some((_, count)) = uses.iter_mut().find(|(value, _)| *value == operand) {
                    *count += 1;
                } else {
                    uses.push((operand, 1));
                }
            }
        }
        for (operand, consumed) in uses {
            if liveness.is_dead_after(operand, block, inst_idx) {
                continue;
            }
            while self.scheduler.stack.iter().filter(|slot| *slot == Some(operand)).count()
                <= consumed
            {
                let depth = self.scheduler.stack.find(operand).unwrap_or_else(|| {
                    if self.recover_lost_internal_stack_value(operand) {
                        return 0;
                    }
                    panic!("resident stack argument {operand:?} was lost before its final use")
                });
                assert!(depth < MAX_STACK_ACCESS, "resident stack argument exceeded DUP16 reach");
                self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
            }
        }
    }

    /// Abandons a speculative internal stack ABI after one of its values was lost.
    ///
    /// The emitted placeholder belongs to an attempt that the outer codegen loop discards. The
    /// next attempt excludes this function from stack-only argument and return plans, so every
    /// value has a frame-backed reload route.
    pub(in crate::backend::evm::codegen) fn recover_lost_internal_stack_value(
        &mut self,
        value: ValueId,
    ) -> bool {
        let Some(func_id) = self.current_internal_function else { return false };
        self.disabled_stack_only_functions.insert(func_id);
        self.asm.emit_push(U256::ZERO);
        self.scheduler.stack.push(value);
        true
    }

    pub(in crate::backend::evm::codegen) fn stack_only_function_disabled(
        &self,
        func_id: FunctionId,
    ) -> bool {
        func_id.index() < self.disabled_stack_only_functions.domain_size()
            && self.disabled_stack_only_functions.contains(func_id)
    }
}
