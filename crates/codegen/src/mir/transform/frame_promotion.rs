//! Compiler-local scalar promotion.
//!
//! This is Solar's EVM-shaped version of LLVM's mem2reg: promote compiler-owned
//! local slots from memory traffic into SSA values. The pass is deliberately
//! conservative. A slot is promotable only when its address is used as the exact
//! address of full-word `mload`/`mstore` instructions, and the function has no
//! observations that could make removing that memory traffic visible.
//!
//! Safety contract:
//! - promote only compiler-owned internal-frame or external-local slots
//! - reject escaped addresses, partial stores, dynamic memory aliases, calls, returndata
//!   observations, and ABI return-buffer overlap
//! - preserve SSA values across control flow with explicit phi insertion

use crate::mir::{
    BlockId, Callee, FrameMode, Function, InstId, InstKind, Instruction, MirType, Module,
    PtrLocation, Terminator, ValueId,
    analysis::{AliasAnalysis, CfgInfo, LocationSize, MemoryAddress, MemoryLocation},
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass},
    utils as mir_utils,
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Function pass for internal-frame scalar promotion.
pub(crate) struct FrameSlotPromotion;

impl MirPass for FrameSlotPromotion {
    fn name(&self) -> &'static str {
        "frame-slot-promotion"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| FrameSlotPromoter::run(func))
    }
}

/// Promotes non-escaping compiler-local slots to SSA values.
struct FrameSlotPromoter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PromotableSlot {
    InternalFrame(u64),
    ExternalLocal(u64),
    /// A constant word within a non-escaping memory object.
    Object(ValueId, u64),
}

/// A load or store of a promotable slot.
#[derive(Clone, Copy, Debug)]
struct SlotAccess {
    block: BlockId,
    inst: InstId,
}

#[derive(Clone, Debug)]
pub(super) struct SlotAccessInfo {
    accesses: FxHashMap<InstId, Option<ValueId>>,
    resets: GrowableBitSet<InstId>,
    slot: PromotableSlot,
    loads: Vec<SlotAccess>,
    stores: Vec<SlotAccess>,
    def_blocks: DenseBitSet<BlockId>,
    access_blocks: DenseBitSet<BlockId>,
}

impl SlotAccessInfo {
    fn new(slot: PromotableSlot, block_count: usize) -> Self {
        Self {
            accesses: FxHashMap::default(),
            resets: GrowableBitSet::new_empty(),
            slot,
            loads: Vec::new(),
            stores: Vec::new(),
            def_blocks: DenseBitSet::new_empty(block_count),
            access_blocks: DenseBitSet::new_empty(block_count),
        }
    }

    pub(super) fn note_load(&mut self, block: BlockId, inst: InstId) {
        self.accesses.insert(inst, None);
        self.loads.push(SlotAccess { block, inst });
        self.access_blocks.insert(block);
    }

    pub(super) fn note_store(&mut self, block: BlockId, inst: InstId, value: ValueId) {
        self.accesses.insert(inst, Some(value));
        self.stores.push(SlotAccess { block, inst });
        self.def_blocks.insert(block);
        self.access_blocks.insert(block);
    }

    pub(super) fn note_reset(&mut self, block: BlockId, inst: InstId) {
        self.resets.insert(inst);
        self.def_blocks.insert(block);
        self.access_blocks.insert(block);
    }

    pub(super) fn object(object: ValueId, slot: u64, blocks: usize) -> Self {
        Self::new(PromotableSlot::Object(object, slot), blocks)
    }
}

#[derive(Clone, Debug)]
struct PendingPhi {
    block: BlockId,
    inst: InstId,
    value: ValueId,
    incoming: Vec<(BlockId, ValueId)>,
}

struct SlotSsaBuilder<'a> {
    info: &'a SlotAccessInfo,
    cfg: &'a CfgInfo,
    replacements: FxHashMap<ValueId, ValueId>,
    /// Pointer loads that read their reaching word back through a cast.
    casts: Vec<(InstId, ValueId)>,
    dead: GrowableBitSet<InstId>,
    phis: FxHashMap<BlockId, PendingPhi>,
    failed: bool,
}

impl FrameSlotPromoter {
    /// Runs compiler-local-slot promotion on a function, returning whether any slot was promoted.
    fn run(func: &mut Function) -> bool {
        if !Self::may_promote(func) {
            return false;
        }

        let cfg = CfgInfo::new(func);
        let aa = AliasAnalysis::new(func);
        let mut promoted = false;
        for info in Self::collect_promotable_slots(func, &cfg, &aa) {
            let mut builder = SlotSsaBuilder::new(&info, &cfg, func);
            if builder.run(func) {
                promoted = true;
                builder.apply(func);
                aa.clear_cached_addresses();
            }
        }

        promoted
    }

    /// Only internal-frame slots and the external entry's locals promote, and both need a
    /// frame. `gas()` is deliberately not a barrier: exact gas is never a semantic guarantee, and
    /// barring it blocked promotion in every forwarding proxy, whose `delegatecall(gas(), ...)` is
    /// exactly the construct that scribbles compiler locals and needs them off memory.
    fn may_promote(func: &Function) -> bool {
        func.internal_frame_size != 0
            || func
                .instructions()
                .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::InternalFrameAddr(_)))
    }

    fn collect_promotable_slots(
        func: &Function,
        cfg: &CfgInfo,
        aa: &AliasAnalysis,
    ) -> Vec<SlotAccessInfo> {
        let mut accesses: FxHashMap<PromotableSlot, SlotAccessInfo> = FxHashMap::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                continue;
            }

            for &inst_id in &block.instructions {
                let kind = &func.inst(inst_id).kind;
                let (addr, stored) = match *kind {
                    InstKind::Load(addr, PtrLocation::Memory) => (addr, None),
                    InstKind::Store(addr, value, PtrLocation::Memory) => (addr, Some(value)),
                    _ => continue,
                };
                let Some(slot) = Self::promotable_slot(func, aa, addr) else { continue };
                let info = accesses
                    .entry(slot)
                    .or_insert_with(|| SlotAccessInfo::new(slot, func.blocks.len()));
                match stored {
                    Some(value) => info.note_store(block_id, inst_id, value),
                    None => info.note_load(block_id, inst_id),
                }
            }
        }

        let mut slots: Vec<SlotAccessInfo> = accesses
            .into_values()
            .filter(|info| !info.loads.is_empty() && !info.stores.is_empty())
            .filter(|info| match info.slot {
                PromotableSlot::InternalFrame(offset) => {
                    Self::internal_frame_slot_safe(func, aa, offset)
                }
                PromotableSlot::ExternalLocal(addr) => {
                    Self::external_local_slot_safe(func, aa, addr)
                }
                PromotableSlot::Object(..) => unreachable!("object slots are supplied by SROA"),
            })
            .collect();
        slots.sort_unstable_by_key(|info| info.slot);
        slots
    }

    fn promotable_slot(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
    ) -> Option<PromotableSlot> {
        Self::internal_frame_offset(func, aa, value).map(PromotableSlot::InternalFrame).or_else(
            || Self::external_local_addr(func, aa, value).map(PromotableSlot::ExternalLocal),
        )
    }

    fn external_local_addr(func: &Function, aa: &AliasAnalysis, value: ValueId) -> Option<u64> {
        let address = aa.memory_address(func, value)?.as_absolute()?;
        Self::external_local_addr_in_range(func, address)
    }

    fn external_local_addr_in_range(func: &Function, addr: u64) -> Option<u64> {
        let local_end = EvmMemoryLayout::HEAP_START.checked_add(func.internal_frame_size)?;
        (addr >= EvmMemoryLayout::HEAP_START
            && addr < local_end
            && (addr - EvmMemoryLayout::HEAP_START).is_multiple_of(EvmMemoryLayout::WORD_SIZE))
        .then_some(addr)
    }

    fn internal_frame_offset(func: &Function, aa: &AliasAnalysis, value: ValueId) -> Option<u64> {
        aa.memory_address(func, value)?.as_internal_frame_offset()
    }

    fn internal_frame_add_offset(
        func: &Function,
        aa: &AliasAnalysis,
        base: ValueId,
        offset: ValueId,
    ) -> Option<u64> {
        let base = Self::internal_frame_offset(func, aa, base)?;
        base.checked_add(func.value_u64(offset)?)
    }

    fn external_local_slot_safe(func: &Function, aa: &AliasAnalysis, slot_addr: u64) -> bool {
        if aa
            .memory_alias(
                func,
                MemoryLocation::new(MemoryAddress::absolute(slot_addr), LocationSize::Const(32)),
                MemoryLocation::new(
                    MemoryAddress::absolute(EvmMemoryLayout::HEAP_START),
                    LocationSize::Const(func.external_static_return_size),
                ),
            )
            .may_alias()
        {
            return false;
        }

        Self::none_observe(
            func,
            |kind| Self::inst_may_observe_external_slot(func, aa, kind, slot_addr),
            |term| Self::terminator_may_observe_external_slot(func, aa, term, slot_addr),
        )
    }

    fn internal_frame_slot_safe(func: &Function, aa: &AliasAnalysis, slot_offset: u64) -> bool {
        Self::none_observe(
            func,
            |kind| Self::inst_may_observe_internal_slot(func, aa, kind, slot_offset),
            |term| Self::terminator_may_observe_internal_slot(func, aa, term, slot_offset),
        )
    }

    /// Returns whether no instruction or terminator of `func` satisfies its observation predicate.
    fn none_observe(
        func: &Function,
        mut inst_observes: impl FnMut(&InstKind) -> bool,
        mut term_observes: impl FnMut(&Terminator) -> bool,
    ) -> bool {
        func.blocks.iter().all(|block| {
            block.instructions.iter().all(|&inst_id| !inst_observes(&func.inst(inst_id).kind))
                && block.terminator.as_ref().is_none_or(|term| !term_observes(term))
        })
    }

    fn inst_may_observe_internal_slot(
        func: &Function,
        aa: &AliasAnalysis,
        kind: &InstKind,
        slot_offset: u64,
    ) -> bool {
        match *kind {
            InstKind::PtrToInt(_, 256) | InstKind::IntToPtr(_) => false,
            // A logical frame slot in the same internal frame. A stored slot address escapes.
            InstKind::FrameLoad { offset, mode: FrameMode::Internal, kind: slot_kind }
            | InstKind::FrameStore { offset, mode: FrameMode::Internal, kind: slot_kind, .. } => {
                let escapes = matches!(*kind, InstKind::FrameStore { value, .. }
                    if Self::internal_frame_offset(func, aa, value) == Some(slot_offset));
                escapes
                    || AliasAnalysis::frame_location(func, offset, FrameMode::Internal, slot_kind)
                        .is_none_or(|location| {
                            aa.memory_alias(
                                func,
                                location,
                                MemoryLocation::new(
                                    MemoryAddress::internal_frame(slot_offset),
                                    LocationSize::Const(32),
                                ),
                            )
                            .may_alias()
                        })
            }
            InstKind::Load(addr, PtrLocation::Memory) => {
                Self::internal_frame_offset(func, aa, addr) != Some(slot_offset)
                    && Self::internal_frame_range_may_overlap(func, aa, addr, Some(32), slot_offset)
            }
            InstKind::Store(addr, value, PtrLocation::Memory) => {
                (Self::internal_frame_offset(func, aa, addr) != Some(slot_offset)
                    && Self::internal_frame_range_may_overlap(
                        func,
                        aa,
                        addr,
                        Some(32),
                        slot_offset,
                    ))
                    || Self::internal_frame_offset(func, aa, value) == Some(slot_offset)
            }
            InstKind::Add(a, b) => {
                let exact_frame_addr = Self::internal_frame_add_offset(func, aa, a, b)
                    .or_else(|| Self::internal_frame_add_offset(func, aa, b, a))
                    .is_some();
                !exact_frame_addr
                    && kind.operands().iter().any(|&value| {
                        Self::internal_frame_offset(func, aa, value) == Some(slot_offset)
                    })
            }
            _ => {
                // NOTE: `Create` ranges are scanned by operand here, unlike in
                // `inst_may_observe_external_slot`.
                let ranges = if matches!(*kind, InstKind::Create(..) | InstKind::Create2(..)) {
                    None
                } else {
                    Self::raw_memory_reaches(func, kind, |addr, size| {
                        Self::internal_frame_range_may_overlap(func, aa, addr, size, slot_offset)
                    })
                };
                ranges.unwrap_or_else(|| {
                    kind.operands().iter().any(|&value| {
                        Self::internal_frame_offset(func, aa, value) == Some(slot_offset)
                    })
                })
            }
        }
    }

    fn terminator_may_observe_internal_slot(
        func: &Function,
        aa: &AliasAnalysis,
        term: &Terminator,
        slot_offset: u64,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::internal_frame_range_may_overlap(
                    func,
                    aa,
                    *offset,
                    func.value_u64(*size),
                    slot_offset,
                )
            }
            _ => term
                .operands()
                .iter()
                .any(|&value| Self::internal_frame_offset(func, aa, value) == Some(slot_offset)),
        }
    }

    fn inst_may_observe_external_slot(
        func: &Function,
        aa: &AliasAnalysis,
        kind: &InstKind,
        slot_addr: u64,
    ) -> bool {
        match *kind {
            // A logical frame slot at an absolute address in the local region.
            InstKind::FrameLoad { offset, mode, kind }
            | InstKind::FrameStore { offset, mode, kind, .. }
                if mode != FrameMode::Internal =>
            {
                AliasAnalysis::frame_location(func, offset, mode, kind).is_none_or(|location| {
                    aa.memory_alias(
                        func,
                        location,
                        MemoryLocation::new(
                            MemoryAddress::absolute(slot_addr),
                            LocationSize::Const(32),
                        ),
                    )
                    .may_alias()
                })
            }
            InstKind::Load(addr, PtrLocation::Memory)
            | InstKind::Store(addr, _, PtrLocation::Memory) => {
                Self::external_local_addr(func, aa, addr) != Some(slot_addr)
                    && Self::external_range_reaches_slot(func, aa, addr, Some(32), slot_addr)
            }
            // Internal callees address their own frame through the frame
            // pointer and stage data in scratch or heap memory; they never
            // reference a caller's compiler-owned absolute local slots.
            InstKind::ICall { function: Callee::Function(_), .. } => false,
            InstKind::MappingSlotBytes(key, _) if func.value_ty(key) == Some(MirType::MEMPTR) => {
                true
            }
            InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => true,
            _ => Self::raw_memory_reaches(func, kind, |addr, size| {
                Self::external_range_reaches_slot(func, aa, addr, size, slot_addr)
            })
            .unwrap_or(false),
        }
    }

    /// Whether a memory access can reference a compiler-owned external local
    /// slot. Locals are unaddressable from source: only lowering-emitted absolute
    /// addresses inside the local region reference them, so a base that is
    /// unresolvable, pointer-derived, or outside the region cannot reach the
    /// slot. Raw user-assembly scribbles — a proxy's
    /// `calldatacopy(0, 0, calldatasize())`, `mstore(0x40, returndatasize())`
    /// tricks — fall in that bucket, and promoting the local out of memory is
    /// exactly what restores solc's locals-on-stack semantics against them.
    fn external_range_reaches_slot(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        size: Option<u64>,
        slot_addr: u64,
    ) -> bool {
        let Some(address) = aa.memory_address(func, addr) else { return false };
        let Some(base) = address.as_absolute() else { return false };
        let Some(local_end) = EvmMemoryLayout::HEAP_START.checked_add(func.internal_frame_size)
        else {
            return true;
        };
        if base < EvmMemoryLayout::HEAP_START || base >= local_end {
            return false;
        }
        match size {
            Some(size) => {
                base < slot_addr.saturating_add(32) && slot_addr < base.saturating_add(size)
            }
            None => base < slot_addr.saturating_add(32),
        }
    }

    fn terminator_may_observe_external_slot(
        func: &Function,
        aa: &AliasAnalysis,
        term: &Terminator,
        slot_addr: u64,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::external_range_reaches_slot(
                    func,
                    aa,
                    *offset,
                    func.value_u64(*size),
                    slot_addr,
                )
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::RevertReturndata
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::TailCall { .. }
            | Terminator::SelfDestruct { .. } => false,
        }
    }

    fn internal_frame_range_may_overlap(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        size: Option<u64>,
        slot_offset: u64,
    ) -> bool {
        let Some(address) = aa.memory_address(func, addr) else {
            return false;
        };
        if address.as_internal_frame_offset().is_none() {
            return false;
        }
        let Some(size) = size else { return true };
        aa.memory_alias(
            func,
            MemoryLocation::new(address, LocationSize::Const(size)),
            MemoryLocation::new(
                MemoryAddress::internal_frame(slot_offset),
                LocationSize::Const(32),
            ),
        )
        .may_alias()
    }

    /// Returns whether any raw memory range of `kind` satisfies `reaches`, or `None` when the
    /// instruction accesses no raw memory range.
    fn raw_memory_reaches(
        func: &Function,
        kind: &InstKind,
        mut reaches: impl FnMut(ValueId, Option<u64>) -> bool,
    ) -> Option<bool> {
        let mut result = None;
        kind.visit_raw_memory(|_, addr, size| {
            *result.get_or_insert(false) |= reaches(addr, size.constant(func));
        });
        result
    }
}

/// Promotes a set of prevalidated object fields as one transaction. A shared
/// zero fill defines every field, so all plans must be built before applying any.
pub(super) fn promote_object_slots(func: &mut Function, slots: &[SlotAccessInfo]) -> bool {
    let cfg = CfgInfo::new(func);
    let snapshot = func.clone();
    let mut builders = Vec::new();
    for info in slots {
        let mut builder = SlotSsaBuilder::new(info, &cfg, func);
        if !builder.run(func) {
            // Discard speculative phi allocations when a reaching definition is absent.
            *func = snapshot;
            return false;
        }
        builders.push(builder);
    }
    // store field, value; ...; result = load field => result = phi/reaching_value
    let mut replacements = FxHashMap::default();
    let mut dead = GrowableBitSet::with_capacity(func.num_insts());
    for builder in builders {
        builder.apply_phis(func);
        builder.apply_casts(func);
        replacements.extend(builder.replacements);
        for inst in builder.dead.iter() {
            dead.insert(inst);
        }
    }
    func.replace_uses_canonicalized(&replacements);
    for block in &mut func.blocks {
        block.instructions.retain(|&inst| !dead.contains(inst));
    }
    true
}

impl<'a> SlotSsaBuilder<'a> {
    fn new(info: &'a SlotAccessInfo, cfg: &'a CfgInfo, func: &Function) -> Self {
        Self {
            info,
            cfg,
            replacements: FxHashMap::default(),
            casts: Vec::new(),
            dead: GrowableBitSet::with_capacity(func.num_insts()),
            phis: FxHashMap::default(),
            failed: false,
        }
    }

    /// Computes the set of blocks where `self.slot` is live-in (a load of the
    /// slot may observe a value defined before the block).
    ///
    /// This is single-variable backward liveness:
    /// - `gen` (upward-exposed use): a promotable load of the slot precedes any store of the slot
    ///   in the block.
    /// - `kill` (def): the block stores the slot, overwriting any entry value.
    /// - `live_in(b) = gen(b) ∨ (live_out(b) ∧ ¬kill(b))`, with `live_out(b) = ⋁ live_in(succ)`.
    ///
    /// Phis are only created at live-in blocks (pruned SSA): forcing a phi at a multi-predecessor
    /// block where the slot is dead can chain back to the entry with no reaching value and
    /// spuriously abort the whole promotion.
    fn compute_live_in(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut live_in = DenseBitSet::new_empty(func.blocks.len());
        let mut kill = DenseBitSet::new_empty(func.blocks.len());

        for block in func.blocks.indices() {
            if !self.cfg.is_reachable(block) {
                continue;
            }

            let mut saw_store = false;
            for &inst_id in &func.blocks[block].instructions {
                saw_store |= self.info.resets.contains(inst_id);
                match self.info.accesses.get(&inst_id) {
                    Some(None) if !saw_store => {
                        live_in.insert(block);
                    }
                    Some(Some(_)) => {
                        saw_store = true;
                    }
                    _ => {}
                }
            }
            if saw_store {
                kill.insert(block);
            }
        }

        // Backward worklist from the upward-exposed uses, stopping at killing blocks.
        let mut worklist: Vec<_> = live_in.iter().collect();
        while let Some(block) = worklist.pop() {
            for &pred in self.cfg.predecessors(block) {
                if self.cfg.is_reachable(pred) && !kill.contains(pred) && live_in.insert(pred) {
                    worklist.push(pred);
                }
            }
        }
        live_in
    }

    fn compute_phi_blocks(
        &self,
        func: &Function,
        live_in: &DenseBitSet<BlockId>,
    ) -> DenseBitSet<BlockId> {
        let frontiers = self.compute_dominance_frontiers(func);
        let mut phi_blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist: Vec<_> = self.info.def_blocks.iter().collect();

        while let Some(block) = worklist.pop() {
            let Some(frontier) = frontiers.get(block) else { continue };
            for &frontier_block in frontier {
                if !live_in.contains(frontier_block) || !phi_blocks.insert(frontier_block) {
                    continue;
                }
                worklist.push(frontier_block);
            }
        }

        phi_blocks
    }

    fn compute_dominance_frontiers(&self, func: &Function) -> IndexVec<BlockId, Vec<BlockId>> {
        let mut frontiers = index_vec![Vec::new(); func.blocks.len()];
        for block in func.blocks.indices() {
            if !self.cfg.is_reachable(block) {
                continue;
            }

            let preds: Vec<_> = func.blocks[block]
                .predecessors
                .iter()
                .copied()
                .filter(|&pred| self.cfg.is_reachable(pred))
                .collect();
            if preds.len() < 2 {
                continue;
            }

            let Some(idom) = self.cfg.dominators().idom(block) else { continue };
            for mut runner in preds {
                while runner != idom {
                    if !frontiers[runner].contains(&block) {
                        frontiers[runner].push(block);
                    }

                    let Some(next) = self.cfg.dominators().idom(runner) else { break };
                    if next == runner {
                        break;
                    }
                    runner = next;
                }
            }
        }

        frontiers
    }

    fn run(&mut self, func: &mut Function) -> bool {
        if self.rewrite_single_block(func) || self.failed {
            return !self.failed;
        }
        if self.rewrite_single_store(func) || self.failed {
            return !self.failed;
        }

        let live_in = self.compute_live_in(func);
        for block in self.compute_phi_blocks(func, &live_in).iter() {
            self.create_phi(func, block);
        }
        self.rename_block(func, BlockId::ENTRY, None);
        !self.failed
    }

    fn apply_phis(&self, func: &mut Function) {
        // field = phi [predecessor: reaching_value], ...
        for pending in self.phis.values() {
            let mut incoming = pending.incoming.clone();
            incoming.sort_by_key(|(block, _)| block.index());
            func.inst_mut(pending.inst).kind = InstKind::Phi(incoming);
            let insert_pos = func.blocks[pending.block]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .count();
            func.blocks[pending.block].instructions.insert(insert_pos, pending.inst);
        }
    }

    fn apply(self, func: &mut Function) {
        self.apply_phis(func);
        self.apply_casts(func);
        // result = load slot => result = reaching_value
        func.replace_uses_canonicalized(&self.replacements);

        for block in func.blocks.iter_mut() {
            block.instructions.retain(|&id| !self.dead.contains(id));
        }
    }

    fn rewrite_single_block(&mut self, func: &Function) -> bool {
        if self.info.access_blocks.count() != 1 {
            return false;
        }
        let block = self.info.access_blocks.iter().next().expect("checked count above");
        if !self.cfg.is_reachable(block) {
            self.failed = true;
            return true;
        }

        self.rewrite_accesses(func, block, &mut None).unwrap_or(false)
    }

    /// Rewrites the slot accesses of `block` given the reaching word `current` on entry, and
    /// leaves the reaching word at the block end in `current`. Returns whether it rewrote any
    /// access, or `None` when a load has no reaching word.
    fn rewrite_accesses(
        &mut self,
        func: &Function,
        block: BlockId,
        current: &mut Option<ValueId>,
    ) -> Option<bool> {
        let mut changed = false;
        for &inst_id in &func.blocks[block].instructions {
            if self.info.resets.contains(inst_id) {
                *current = None;
            }
            match self.info.accesses.get(&inst_id) {
                Some(None) => {
                    self.replace_load(func, inst_id, (*current)?);
                    changed = true;
                }
                Some(Some(_)) => {
                    let value = self.store_value(func, inst_id);
                    *current = Some(mir_utils::resolve_replacement(value, &self.replacements));
                    self.remove_store(inst_id);
                    changed = true;
                }
                _ => {}
            }
        }
        Some(changed)
    }

    fn rewrite_single_store(&mut self, func: &Function) -> bool {
        if !self.info.resets.is_empty() {
            return false;
        }
        let [store] = self.info.stores.as_slice() else { return false };
        let stored_value =
            mir_utils::resolve_replacement(self.store_value(func, store.inst), &self.replacements);

        for load in &self.info.loads {
            let dominated = if load.block == store.block {
                let Some(store_pos) = Self::inst_position(func, store.block, store.inst) else {
                    return false;
                };
                let Some(load_pos) = Self::inst_position(func, load.block, load.inst) else {
                    return false;
                };
                store_pos < load_pos
            } else {
                self.cfg.dominators().dominates(store.block, load.block)
            };

            if !dominated {
                return false;
            }
        }

        for load in &self.info.loads {
            self.replace_load(func, load.inst, stored_value);
        }
        self.remove_store(store.inst);
        true
    }

    /// Earlier slot promotions rewrite store operands. Read the live store
    /// rather than its collection-time value; a semantic zero has no explicit
    /// value operand and keeps the scalar zero supplied by SROA.
    fn store_value(&self, func: &Function, inst: InstId) -> ValueId {
        match func.inst(inst).kind {
            InstKind::Store(_, value, PtrLocation::Memory) => value,
            InstKind::MemoryZero(..) => self.info.accesses[&inst].expect("zero defines the field"),
            _ => unreachable!("promoted definition must be a word store or zero fill"),
        }
    }

    fn inst_position(func: &Function, block: BlockId, inst: InstId) -> Option<usize> {
        func.blocks[block].instructions.iter().position(|&candidate| candidate == inst)
    }

    fn replace_load(&mut self, func: &Function, inst_id: InstId, value: ValueId) {
        if let Some(load_value) = func.inst_result_value(inst_id) {
            let value = mir_utils::resolve_replacement(value, &self.replacements);
            if func.value_ty(load_value) == func.value_ty(value) {
                self.replacements.insert(load_value, value);
                self.dead.insert(inst_id);
            } else {
                self.casts.push((inst_id, value));
            }
        }
    }

    /// Rewrites each pointer load into a cast of the word it reads.
    fn apply_casts(&self, func: &mut Function) {
        // result = load slot, ptr<memory> => result = inttoptr reaching_value to ptr<memory>
        for &(inst, value) in &self.casts {
            func.inst_mut(inst).replace_kind(InstKind::IntToPtr(value));
        }
    }

    fn remove_store(&mut self, inst_id: InstId) {
        self.dead.insert(inst_id);
    }

    fn rename_block(&mut self, func: &Function, block: BlockId, mut current: Option<ValueId>) {
        if !self.cfg.is_reachable(block) || self.failed {
            return;
        }
        if let Some(phi) = self.phis.get(&block) {
            current = Some(phi.value);
        }

        if self.rewrite_accesses(func, block, &mut current).is_none() {
            self.failed = true;
            return;
        }

        for &succ in self.cfg.successors(block) {
            if let Some(phi) = self.phis.get_mut(&succ) {
                let Some(value) = current else {
                    self.failed = true;
                    return;
                };
                phi.incoming
                    .push((block, mir_utils::resolve_replacement(value, &self.replacements)));
            }
        }

        let children = self.cfg.dominators().children(block).to_vec();
        for child in children {
            self.rename_block(func, child, current);
        }
    }

    fn create_phi(&mut self, func: &mut Function, block: BlockId) -> ValueId {
        if let Some(pending) = self.phis.get(&block) {
            return pending.value;
        }

        // field = phi []
        let (inst, value) = func.alloc_value_inst(
            Instruction::new(InstKind::Phi(Vec::new()), Some(MirType::I256))
                .with_debug_info_dropped(),
        );
        self.phis.insert(
            block,
            PendingPhi {
                block,
                inst,
                value,
                incoming: Vec::with_capacity(func.blocks[block].predecessors.len()),
            },
        );
        value
    }
}
