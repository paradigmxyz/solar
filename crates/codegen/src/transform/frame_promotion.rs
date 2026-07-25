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

use crate::{
    analysis::{AliasAnalysis, CfgInfo, LocationSize, MemoryAddress, MemoryLocation},
    memory::EvmMemoryLayout,
    mir::{
        BlockId, Function, InstId, InstKind, Instruction, MirType, Module, Terminator, Value,
        ValueId, utils as mir_utils,
    },
    pass::{MirPass, run_function_pass_no_analyses},
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::cell::OnceCell;

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
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_no_analyses(module, analyses, |func| {
            FrameSlotPromoter::new().run(func).total() != 0
        })
    }
}

/// Statistics for one frame promotion run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FramePromotionStats {
    /// Number of distinct compiler-local slots promoted.
    slots_promoted: usize,
    /// Number of local-slot loads replaced by SSA values.
    loads_promoted: usize,
    /// Number of local-slot stores removed.
    stores_promoted: usize,
    /// Number of phi nodes inserted.
    phis_inserted: usize,
}

/// A compiler-owned memory slot promoted to SSA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PromotedSlot {
    /// Slot addressed relative to the internal-call frame pointer.
    InternalFrame(u64),
    /// Slot addressed in the external entry's compiler-owned low-memory locals.
    ExternalLocal(u64),
}

/// Per-slot information produced by frame-slot promotion.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PromotedSlotSummary {
    /// Promoted compiler-owned slot.
    slot: PromotedSlot,
    /// Blocks where the slot had an upward-exposed load before promotion.
    use_blocks: Vec<BlockId>,
    /// Blocks where the slot was defined before promotion.
    def_blocks: Vec<BlockId>,
    /// Blocks where SSA phis were inserted.
    phi_blocks: Vec<BlockId>,
    /// SSA phi values inserted for this slot.
    phi_values: Vec<ValueId>,
    /// Number of loads replaced by SSA values.
    loads_promoted: usize,
    /// Number of stores removed.
    stores_promoted: usize,
}

impl FramePromotionStats {
    /// Returns the total number of MIR edits made by this pass.
    const fn total(self) -> usize {
        self.loads_promoted + self.stores_promoted + self.phis_inserted
    }
}

/// Promotes non-escaping compiler-local slots to SSA values.
#[derive(Debug, Default)]
struct FrameSlotPromoter {
    stats: FramePromotionStats,
    summaries: Vec<PromotedSlotSummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PromotableSlot {
    InternalFrame(u64),
    ExternalLocal(u64),
}

impl From<PromotableSlot> for PromotedSlot {
    fn from(slot: PromotableSlot) -> Self {
        match slot {
            PromotableSlot::InternalFrame(offset) => Self::InternalFrame(offset),
            PromotableSlot::ExternalLocal(addr) => Self::ExternalLocal(addr),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SlotLoad {
    block: BlockId,
    inst: InstId,
    position: usize,
}

#[derive(Clone, Copy, Debug)]
struct SlotStore {
    block: BlockId,
    inst: InstId,
    position: usize,
    value: ValueId,
}

#[derive(Clone, Copy, Debug)]
enum SlotAccess {
    Load(SlotLoad),
    Store(SlotStore),
}

#[derive(Clone, Debug)]
struct SlotAccessInfo {
    slot: PromotableSlot,
    loads: Vec<SlotLoad>,
    stores: Vec<SlotStore>,
    accesses: FxHashMap<BlockId, Vec<SlotAccess>>,
    use_blocks: DenseBitSet<BlockId>,
    def_blocks: DenseBitSet<BlockId>,
    access_blocks: DenseBitSet<BlockId>,
}

impl SlotAccessInfo {
    fn new(slot: PromotableSlot, block_count: usize) -> Self {
        Self {
            slot,
            loads: Vec::new(),
            stores: Vec::new(),
            accesses: FxHashMap::default(),
            use_blocks: DenseBitSet::new_empty(block_count),
            def_blocks: DenseBitSet::new_empty(block_count),
            access_blocks: DenseBitSet::new_empty(block_count),
        }
    }

    fn note_load(&mut self, block: BlockId, inst: InstId, position: usize) {
        let load = SlotLoad { block, inst, position };
        self.loads.push(load);
        self.accesses.entry(block).or_default().push(SlotAccess::Load(load));
        self.use_blocks.insert(block);
        self.access_blocks.insert(block);
    }

    fn note_store(&mut self, block: BlockId, inst: InstId, position: usize, value: ValueId) {
        let store = SlotStore { block, inst, position, value };
        self.stores.push(store);
        self.accesses.entry(block).or_default().push(SlotAccess::Store(store));
        self.def_blocks.insert(block);
        self.access_blocks.insert(block);
    }

    fn sorted_use_blocks(&self) -> Vec<BlockId> {
        sorted_blocks(&self.use_blocks)
    }

    fn sorted_def_blocks(&self) -> Vec<BlockId> {
        sorted_blocks(&self.def_blocks)
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
    dominance_frontiers: &'a OnceCell<IndexVec<BlockId, Vec<BlockId>>>,
    replacements: FxHashMap<ValueId, ValueId>,
    dead: GrowableBitSet<InstId>,
    phis: FxHashMap<BlockId, PendingPhi>,
    /// Blocks where this slot is live-in. Used to place phis only where the slot
    /// is actually live (pruned SSA): forcing a phi at a multi-predecessor block
    /// where the slot is dead can chain back to the entry with no reaching value
    /// and spuriously abort the whole promotion.
    live_in: DenseBitSet<BlockId>,
    /// Blocks selected by pruned iterated-dominance-frontier phi placement.
    phi_blocks: DenseBitSet<BlockId>,
    failed: bool,
    loads_promoted: usize,
    stores_promoted: usize,
}

fn sorted_blocks(blocks: &DenseBitSet<BlockId>) -> Vec<BlockId> {
    blocks.iter().collect()
}

impl FrameSlotPromoter {
    /// Creates a new compiler-local-slot promoter.
    fn new() -> Self {
        Self::default()
    }

    /// Runs compiler-local-slot promotion on a function.
    fn run(&mut self, func: &mut Function) -> FramePromotionStats {
        self.stats = FramePromotionStats::default();
        self.summaries.clear();

        if Self::has_global_observation_barrier(func) {
            return self.stats;
        }

        let cfg = CfgInfo::new(func);
        let aa = AliasAnalysis::new(func);
        let slots = Self::collect_promotable_slots(func, &cfg, &aa);
        if slots.is_empty() {
            return self.stats;
        };

        let mut replacements = FxHashMap::default();
        let mut dead = GrowableBitSet::with_capacity(func.num_insts());
        let mut phis = Vec::new();
        let dominance_frontiers = OnceCell::new();
        for info in slots {
            let mut builder =
                SlotSsaBuilder::new(&info, &cfg, &dominance_frontiers, func.num_insts());
            if builder.run(func) {
                self.stats.slots_promoted += 1;
                self.stats.loads_promoted += builder.loads_promoted;
                self.stats.stores_promoted += builder.stores_promoted;
                self.stats.phis_inserted += builder.phis.len();
                self.summaries.push(builder.summary());
                replacements.extend(builder.replacements);
                for inst in &builder.dead {
                    dead.insert(inst);
                }
                phis.extend(builder.phis.into_values());
            }
        }
        Self::apply_promotions(func, replacements, &dead, phis);

        self.stats
    }

    fn has_global_observation_barrier(func: &Function) -> bool {
        func.instructions()
            .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::Gas | InstKind::MSize))
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

            for (position, &inst_id) in block.instructions.iter().enumerate() {
                let kind = &func.inst(inst_id).kind;
                match *kind {
                    InstKind::MLoad(addr) => {
                        if let Some(slot) = Self::promotable_slot(func, aa, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot, func.blocks.len()))
                                .note_load(block_id, inst_id, position);
                        }
                    }
                    InstKind::MStore(addr, value) => {
                        if let Some(slot) = Self::promotable_slot(func, aa, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot, func.blocks.len()))
                                .note_store(block_id, inst_id, position, value);
                        }
                    }
                    _ => {}
                }
            }
        }

        let internal_offsets = {
            let mut offsets: Vec<_> = accesses
                .keys()
                .filter_map(|slot| match *slot {
                    PromotableSlot::InternalFrame(offset) => Some(offset),
                    PromotableSlot::ExternalLocal(_) => None,
                })
                .collect();
            offsets.sort_unstable();
            offsets
        };
        let (all_internal_unsafe, unsafe_internal) =
            Self::unsafe_internal_frame_slots(func, aa, &internal_offsets);
        let mut slots: Vec<SlotAccessInfo> = accesses
            .into_values()
            .filter(|info| !info.loads.is_empty() && !info.stores.is_empty())
            .filter(|info| match info.slot {
                PromotableSlot::InternalFrame(offset) => {
                    !all_internal_unsafe && !unsafe_internal.contains(&offset)
                }
                PromotableSlot::ExternalLocal(addr) => {
                    Self::external_local_slot_safe(func, aa, addr)
                }
            })
            .collect();
        slots.sort_by_key(|info| info.slot);
        slots
    }

    fn apply_promotions(
        func: &mut Function,
        replacements: FxHashMap<ValueId, ValueId>,
        dead: &GrowableBitSet<InstId>,
        phis: Vec<PendingPhi>,
    ) {
        let mut new_phis = index_vec![Vec::new(); func.blocks.len()];
        for pending in phis {
            let mut incoming = pending.incoming;
            incoming.sort_by_key(|(block, _)| block.index());
            func.inst_mut(pending.inst).kind = InstKind::Phi(incoming);
            new_phis[pending.block].push(pending.inst);
        }
        for (block, phis) in new_phis.iter_mut_enumerated() {
            if phis.is_empty() {
                continue;
            }
            let insert_pos = func.blocks[block]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .count();
            func.blocks[block].instructions.splice(insert_pos..insert_pos, phis.drain(..));
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|&id| !dead.contains(id));
        }
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
        _depth: usize,
    ) -> Option<u64> {
        let base = Self::internal_frame_offset(func, aa, base)?;
        base.checked_add(func.value_u64(offset)?)
    }

    fn external_local_slot_safe(func: &Function, aa: &AliasAnalysis, slot_addr: u64) -> bool {
        if aa
            .memory_alias(
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

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if Self::inst_may_observe_external_slot(
                    func,
                    aa,
                    &func.inst(inst_id).kind,
                    slot_addr,
                ) {
                    return false;
                }
            }
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_external_slot(func, aa, term, slot_addr)
            {
                return false;
            }
        }

        true
    }

    fn unsafe_internal_frame_slots(
        func: &Function,
        aa: &AliasAnalysis,
        candidates: &[u64],
    ) -> (bool, FxHashSet<u64>) {
        let mut unsafe_slots = FxHashSet::default();
        for inst_id in func.instructions() {
            if Self::mark_internal_frame_inst(
                func,
                aa,
                &func.inst(inst_id).kind,
                candidates,
                &mut unsafe_slots,
            ) {
                return (true, unsafe_slots);
            }
        }
        for block in func.blocks.iter() {
            if let Some(term) = &block.terminator
                && Self::mark_internal_frame_terminator(
                    func,
                    aa,
                    term,
                    candidates,
                    &mut unsafe_slots,
                )
            {
                return (true, unsafe_slots);
            }
        }
        (false, unsafe_slots)
    }

    fn mark_internal_frame_inst(
        func: &Function,
        aa: &AliasAnalysis,
        kind: &InstKind,
        candidates: &[u64],
        unsafe_slots: &mut FxHashSet<u64>,
    ) -> bool {
        match *kind {
            InstKind::MLoad(addr) => {
                let exact = Self::internal_frame_offset(func, aa, addr);
                Self::mark_internal_frame_range(
                    func,
                    aa,
                    addr,
                    Some(32),
                    exact,
                    candidates,
                    unsafe_slots,
                )
            }
            InstKind::MStore(addr, value) => {
                Self::mark_internal_frame_value(func, aa, value, candidates, unsafe_slots);
                let exact = Self::internal_frame_offset(func, aa, addr);
                Self::mark_internal_frame_range(
                    func,
                    aa,
                    addr,
                    Some(32),
                    exact,
                    candidates,
                    unsafe_slots,
                )
            }
            InstKind::MStore8(addr, _) => Self::mark_internal_frame_range(
                func,
                aa,
                addr,
                Some(1),
                None,
                candidates,
                unsafe_slots,
            ),
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => Self::mark_internal_frame_range(
                func,
                aa,
                addr,
                func.value_u64(size),
                None,
                candidates,
                unsafe_slots,
            ),
            InstKind::MCopy(dest, src, size) => {
                let size = func.value_u64(size);
                Self::mark_internal_frame_range(
                    func,
                    aa,
                    dest,
                    size,
                    None,
                    candidates,
                    unsafe_slots,
                ) || Self::mark_internal_frame_range(
                    func,
                    aa,
                    src,
                    size,
                    None,
                    candidates,
                    unsafe_slots,
                )
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => Self::mark_internal_frame_range(
                func,
                aa,
                dest,
                func.value_u64(size),
                None,
                candidates,
                unsafe_slots,
            ),
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => Self::mark_internal_frame_range(
                func,
                aa,
                addr,
                func.value_u64(size),
                None,
                candidates,
                unsafe_slots,
            ),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                Self::mark_internal_frame_range(
                    func,
                    aa,
                    args_offset,
                    func.value_u64(args_size),
                    None,
                    candidates,
                    unsafe_slots,
                ) || Self::mark_internal_frame_range(
                    func,
                    aa,
                    ret_offset,
                    func.value_u64(ret_size),
                    None,
                    candidates,
                    unsafe_slots,
                )
            }
            InstKind::Add(a, b) => {
                let exact_frame_addr = Self::internal_frame_add_offset(func, aa, a, b, 0)
                    .or_else(|| Self::internal_frame_add_offset(func, aa, b, a, 0))
                    .is_some();
                if !exact_frame_addr {
                    Self::mark_internal_frame_operands(
                        func,
                        aa,
                        kind.operands(),
                        candidates,
                        unsafe_slots,
                    );
                }
                false
            }
            _ => {
                Self::mark_internal_frame_operands(
                    func,
                    aa,
                    kind.operands(),
                    candidates,
                    unsafe_slots,
                );
                false
            }
        }
    }

    fn mark_internal_frame_terminator(
        func: &Function,
        aa: &AliasAnalysis,
        term: &Terminator,
        candidates: &[u64],
        unsafe_slots: &mut FxHashSet<u64>,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::mark_internal_frame_range(
                    func,
                    aa,
                    *offset,
                    func.value_u64(*size),
                    None,
                    candidates,
                    unsafe_slots,
                )
            }
            _ => {
                Self::mark_internal_frame_operands(
                    func,
                    aa,
                    term.operands(),
                    candidates,
                    unsafe_slots,
                );
                false
            }
        }
    }

    fn mark_internal_frame_operands(
        func: &Function,
        aa: &AliasAnalysis,
        operands: impl IntoIterator<Item = ValueId>,
        candidates: &[u64],
        unsafe_slots: &mut FxHashSet<u64>,
    ) {
        for value in operands {
            Self::mark_internal_frame_value(func, aa, value, candidates, unsafe_slots);
        }
    }

    fn mark_internal_frame_value(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        candidates: &[u64],
        unsafe_slots: &mut FxHashSet<u64>,
    ) {
        if let Some(offset) = Self::internal_frame_offset(func, aa, value)
            && candidates.binary_search(&offset).is_ok()
        {
            unsafe_slots.insert(offset);
        }
    }

    fn mark_internal_frame_range(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        size: Option<u64>,
        exclude: Option<u64>,
        candidates: &[u64],
        unsafe_slots: &mut FxHashSet<u64>,
    ) -> bool {
        let Some(start) =
            aa.memory_address(func, addr).and_then(MemoryAddress::as_internal_frame_offset)
        else {
            return false;
        };
        let Some(size) = size else { return true };
        if size == 0 {
            return false;
        }
        let Some(end) = start.checked_add(size) else {
            for &candidate in candidates {
                if Some(candidate) != exclude {
                    unsafe_slots.insert(candidate);
                }
            }
            return false;
        };

        let first = candidates.partition_point(|&candidate| {
            candidate.checked_add(EvmMemoryLayout::WORD_SIZE).is_some_and(|end| end <= start)
        });
        for &candidate in &candidates[first..] {
            if candidate >= end && candidate.checked_add(EvmMemoryLayout::WORD_SIZE).is_some() {
                break;
            }
            if Some(candidate) != exclude {
                unsafe_slots.insert(candidate);
            }
        }
        false
    }

    fn inst_may_observe_external_slot(
        func: &Function,
        aa: &AliasAnalysis,
        kind: &InstKind,
        slot_addr: u64,
    ) -> bool {
        match *kind {
            InstKind::MLoad(addr) | InstKind::MStore(addr, _) => {
                !Self::is_exact_external_slot_access(func, aa, addr, slot_addr)
                    && Self::memory_range_may_overlap(func, aa, addr, Some(32), slot_addr)
            }
            InstKind::MStore8(addr, _) => {
                Self::memory_range_may_overlap(func, aa, addr, Some(1), slot_addr)
            }
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => {
                Self::memory_range_may_overlap(func, aa, addr, func.value_u64(size), slot_addr)
            }
            InstKind::MCopy(dest, src, size) => {
                let size = func.value_u64(size);
                Self::memory_range_may_overlap(func, aa, dest, size, slot_addr)
                    || Self::memory_range_may_overlap(func, aa, src, size, slot_addr)
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                Self::memory_range_may_overlap(func, aa, dest, func.value_u64(size), slot_addr)
            }
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => {
                Self::memory_range_may_overlap(func, aa, addr, func.value_u64(size), slot_addr)
            }
            InstKind::Call { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::InternalCall { .. }
            | InstKind::Create(_, _, _)
            | InstKind::Create2(_, _, _, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::AbiEncode { .. }
            | InstKind::MSize => true,
            _ => false,
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
                Self::memory_range_may_overlap(func, aa, *offset, func.value_u64(*size), slot_addr)
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::TailCall { .. }
            | Terminator::SelfDestruct { .. } => false,
        }
    }

    fn is_exact_external_slot_access(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        slot_addr: u64,
    ) -> bool {
        Self::external_local_addr(func, aa, addr) == Some(slot_addr)
    }

    fn memory_range_may_overlap(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        size: Option<u64>,
        slot_addr: u64,
    ) -> bool {
        let Some(size) = size else { return true };
        let Some(address) = aa.memory_address(func, addr) else {
            return true;
        };
        aa.memory_alias(
            MemoryLocation::new(address, LocationSize::Const(size)),
            MemoryLocation::new(MemoryAddress::absolute(slot_addr), LocationSize::Const(32)),
        )
        .may_alias()
    }
}

impl<'a> SlotSsaBuilder<'a> {
    fn new(
        info: &'a SlotAccessInfo,
        cfg: &'a CfgInfo,
        dominance_frontiers: &'a OnceCell<IndexVec<BlockId, Vec<BlockId>>>,
        instruction_count: usize,
    ) -> Self {
        Self {
            info,
            cfg,
            dominance_frontiers,
            replacements: FxHashMap::default(),
            dead: GrowableBitSet::with_capacity(instruction_count),
            phis: FxHashMap::default(),
            live_in: DenseBitSet::new_empty(info.use_blocks.domain_size()),
            phi_blocks: DenseBitSet::new_empty(info.use_blocks.domain_size()),
            failed: false,
            loads_promoted: 0,
            stores_promoted: 0,
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
    /// Phis are only created at live-in blocks (pruned SSA).
    fn summary(&self) -> PromotedSlotSummary {
        let mut phi_blocks = sorted_blocks(&self.phi_blocks);
        phi_blocks.retain(|block| self.phis.contains_key(block));

        let mut phi_values: Vec<_> = self.phis.values().map(|phi| phi.value).collect();
        phi_values.sort_by_key(|value| value.index());

        PromotedSlotSummary {
            slot: self.info.slot.into(),
            use_blocks: self.info.sorted_use_blocks(),
            def_blocks: self.info.sorted_def_blocks(),
            phi_blocks,
            phi_values,
            loads_promoted: self.loads_promoted,
            stores_promoted: self.stores_promoted,
        }
    }

    fn compute_live_in(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut gen_set = DenseBitSet::new_empty(func.blocks.len());
        let kill = self.info.def_blocks.clone();
        for (&block, accesses) in &self.info.accesses {
            let mut saw_store = false;
            for access in accesses {
                match access {
                    SlotAccess::Load(_) if !saw_store => {
                        gen_set.insert(block);
                    }
                    SlotAccess::Store(_) => saw_store = true,
                    SlotAccess::Load(_) => {}
                }
            }
        }

        let mut live_in = gen_set;
        let mut worklist: Vec<_> = live_in.iter().collect();
        while let Some(block) = worklist.pop() {
            for &predecessor in &func.blocks[block].predecessors {
                if self.cfg.is_reachable(predecessor)
                    && !kill.contains(predecessor)
                    && live_in.insert(predecessor)
                {
                    worklist.push(predecessor);
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
        let frontiers =
            self.dominance_frontiers.get_or_init(|| self.compute_dominance_frontiers(func));
        let mut phi_blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = sorted_blocks(&self.info.def_blocks);

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

        for frontier in &mut frontiers {
            frontier.sort_by_key(|block| block.index());
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

        self.live_in = self.compute_live_in(func);
        self.phi_blocks = self.compute_phi_blocks(func, &self.live_in);
        for block in sorted_blocks(&self.phi_blocks) {
            self.create_phi(func, block);
        }
        self.rename_block(func, BlockId::ENTRY, None);
        !self.failed
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

        let mut current = None;
        let mut changed = false;
        let mut loads = self.info.loads.iter().peekable();
        let mut stores = self.info.stores.iter().peekable();
        while loads.peek().is_some() || stores.peek().is_some() {
            let next_is_load = match (loads.peek(), stores.peek()) {
                (Some(load), Some(store)) => load.position < store.position,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => unreachable!(),
            };
            if next_is_load {
                let load = loads.next().expect("checked next load");
                debug_assert_eq!(load.block, block);
                let Some(value) = current else { return false };
                self.replace_load(func, load.inst, value);
                changed = true;
            } else {
                let store = stores.next().expect("checked next store");
                debug_assert_eq!(store.block, block);
                current = Some(mir_utils::resolve_replacement(store.value, &self.replacements));
                self.remove_store(store.inst);
                changed = true;
            }
        }
        changed
    }

    fn rewrite_single_store(&mut self, func: &Function) -> bool {
        let [store] = self.info.stores.as_slice() else { return false };
        let stored_value = mir_utils::resolve_replacement(store.value, &self.replacements);

        for load in &self.info.loads {
            let dominated = if load.block == store.block {
                store.position < load.position
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

    fn replace_load(&mut self, func: &Function, inst_id: InstId, value: ValueId) {
        if let Some(load_value) = func.inst_result_value(inst_id) {
            self.replacements
                .insert(load_value, mir_utils::resolve_replacement(value, &self.replacements));
            self.dead.insert(inst_id);
            self.loads_promoted += 1;
        }
    }

    fn remove_store(&mut self, inst_id: InstId) {
        self.dead.insert(inst_id);
        self.stores_promoted += 1;
    }

    fn rename_block(&mut self, func: &mut Function, block: BlockId, mut current: Option<ValueId>) {
        if !self.cfg.is_reachable(block) || self.failed {
            return;
        }
        if let Some(phi) = self.phis.get(&block) {
            current = Some(phi.value);
        }

        if let Some(accesses) = self.info.accesses.get(&block) {
            for access in accesses {
                match *access {
                    SlotAccess::Load(load) => {
                        let Some(value) = current else {
                            self.failed = true;
                            return;
                        };
                        self.replace_load(func, load.inst, value);
                    }
                    SlotAccess::Store(store) => {
                        current =
                            Some(mir_utils::resolve_replacement(store.value, &self.replacements));
                        self.remove_store(store.inst);
                    }
                }
            }
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

        let inst =
            func.alloc_inst(Instruction::new(InstKind::Phi(Vec::new()), Some(MirType::uint256())));
        let value = func.alloc_value(Value::Inst(inst));
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
