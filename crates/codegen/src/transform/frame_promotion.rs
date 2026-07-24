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
        ValueId,
        utils::{self as mir_utils, repair_reachability_phis},
    },
    pass::{MirPass, run_function_pass},
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
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let changed = FrameSlotPromoter::new().run(func).total() != 0;
            let repaired = repair_reachability_phis(func);
            changed || repaired
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
}

#[derive(Clone, Copy, Debug)]
struct SlotStore {
    block: BlockId,
    inst: InstId,
    value: ValueId,
}

#[derive(Clone, Debug)]
struct SlotAccessInfo {
    slot: PromotableSlot,
    loads: Vec<SlotLoad>,
    stores: Vec<SlotStore>,
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
            use_blocks: DenseBitSet::new_empty(block_count),
            def_blocks: DenseBitSet::new_empty(block_count),
            access_blocks: DenseBitSet::new_empty(block_count),
        }
    }

    fn note_load(&mut self, block: BlockId, inst: InstId) {
        self.loads.push(SlotLoad { block, inst });
        self.use_blocks.insert(block);
        self.access_blocks.insert(block);
    }

    fn note_store(&mut self, block: BlockId, inst: InstId, value: ValueId) {
        self.stores.push(SlotStore { block, inst, value });
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
    inst_results: &'a FxHashMap<InstId, ValueId>,
    aa: &'a AliasAnalysis,
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

        for info in slots {
            let inst_results = func.inst_results();
            let mut builder =
                SlotSsaBuilder::new(&info, &cfg, &inst_results, &aa, func.instructions.len());
            if builder.run(func) {
                self.stats.slots_promoted += 1;
                self.stats.loads_promoted += builder.loads_promoted;
                self.stats.stores_promoted += builder.stores_promoted;
                self.stats.phis_inserted += builder.phis.len();
                self.summaries.push(builder.summary());
                builder.apply(func);
                aa.clear_cached_addresses();
            }
        }

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

            for &inst_id in &block.instructions {
                let kind = &func.inst(inst_id).kind;
                match *kind {
                    InstKind::MLoad(addr) => {
                        if let Some(slot) = Self::promotable_slot(func, aa, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot, func.blocks.len()))
                                .note_load(block_id, inst_id);
                        }
                    }
                    InstKind::MStore(addr, value) => {
                        if let Some(slot) = Self::promotable_slot(func, aa, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot, func.blocks.len()))
                                .note_store(block_id, inst_id, value);
                        }
                    }
                    _ => {}
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
            })
            .collect();
        slots.sort_by_key(|info| info.slot);
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

        if func.instructions().any(|inst_id| {
            Self::inst_may_observe_external_slot(func, aa, &func.inst(inst_id).kind, slot_addr)
        }) {
            return false;
        }
        for block in func.blocks.iter() {
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_external_slot(func, aa, term, slot_addr)
            {
                return false;
            }
        }

        true
    }

    fn internal_frame_slot_safe(func: &Function, aa: &AliasAnalysis, slot_offset: u64) -> bool {
        if func.instructions().any(|inst_id| {
            Self::inst_may_observe_internal_slot(func, aa, &func.inst(inst_id).kind, slot_offset)
        }) {
            return false;
        }
        for block in func.blocks.iter() {
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_internal_slot(func, aa, term, slot_offset)
            {
                return false;
            }
        }

        true
    }

    fn inst_may_observe_internal_slot(
        func: &Function,
        aa: &AliasAnalysis,
        kind: &InstKind,
        slot_offset: u64,
    ) -> bool {
        match *kind {
            InstKind::MLoad(addr) => {
                !Self::is_exact_internal_slot_access(func, aa, addr, slot_offset)
                    && Self::internal_frame_range_may_overlap(func, aa, addr, Some(32), slot_offset)
            }
            InstKind::MStore(addr, value) => {
                (!Self::is_exact_internal_slot_access(func, aa, addr, slot_offset)
                    && Self::internal_frame_range_may_overlap(
                        func,
                        aa,
                        addr,
                        Some(32),
                        slot_offset,
                    ))
                    || Self::internal_frame_offset(func, aa, value) == Some(slot_offset)
            }
            InstKind::MStore8(addr, _) => {
                Self::internal_frame_range_may_overlap(func, aa, addr, Some(1), slot_offset)
            }
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => Self::internal_frame_range_may_overlap(
                func,
                aa,
                addr,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::MCopy(dest, src, size) => {
                let size = func.value_u64(size);
                Self::internal_frame_range_may_overlap(func, aa, dest, size, slot_offset)
                    || Self::internal_frame_range_may_overlap(func, aa, src, size, slot_offset)
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => Self::internal_frame_range_may_overlap(
                func,
                aa,
                dest,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => Self::internal_frame_range_may_overlap(
                func,
                aa,
                addr,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                Self::internal_frame_range_may_overlap(
                    func,
                    aa,
                    args_offset,
                    func.value_u64(args_size),
                    slot_offset,
                ) || Self::internal_frame_range_may_overlap(
                    func,
                    aa,
                    ret_offset,
                    func.value_u64(ret_size),
                    slot_offset,
                )
            }
            InstKind::Add(a, b) => {
                let exact_frame_addr = Self::internal_frame_add_offset(func, aa, a, b, 0)
                    .or_else(|| Self::internal_frame_add_offset(func, aa, b, a, 0))
                    .is_some();
                !exact_frame_addr
                    && kind.operands().iter().any(|&value| {
                        Self::internal_frame_offset(func, aa, value) == Some(slot_offset)
                    })
            }
            _ => kind
                .operands()
                .iter()
                .any(|&value| Self::internal_frame_offset(func, aa, value) == Some(slot_offset)),
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

    fn is_exact_internal_slot_access(
        func: &Function,
        aa: &AliasAnalysis,
        addr: ValueId,
        slot_offset: u64,
    ) -> bool {
        Self::internal_frame_offset(func, aa, addr) == Some(slot_offset)
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
            MemoryLocation::new(address, LocationSize::Const(size)),
            MemoryLocation::new(
                MemoryAddress::internal_frame(slot_offset),
                LocationSize::Const(32),
            ),
        )
        .may_alias()
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
        inst_results: &'a FxHashMap<InstId, ValueId>,
        aa: &'a AliasAnalysis,
        instruction_count: usize,
    ) -> Self {
        Self {
            info,
            cfg,
            inst_results,
            aa,
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
        let mut kill = DenseBitSet::new_empty(func.blocks.len());

        for block in func.blocks.indices() {
            if !self.cfg.is_reachable(block) {
                continue;
            }

            let mut saw_store = false;
            for &inst_id in &func.blocks[block].instructions {
                match func.inst(inst_id).kind {
                    InstKind::MLoad(addr)
                        if !saw_store
                            && FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                                == Some(self.info.slot) =>
                    {
                        gen_set.insert(block);
                    }
                    InstKind::MStore(addr, _)
                        if FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                            == Some(self.info.slot) =>
                    {
                        saw_store = true;
                    }
                    _ => {}
                }
            }
            if saw_store {
                kill.insert(block);
            }
        }

        // Backward fixpoint. `live_in` only grows, so a block already in the set
        // can be skipped on later rounds.
        let mut live_in = gen_set;
        let mut changed = true;
        while changed {
            changed = false;
            for block in func.blocks.indices() {
                if !self.cfg.is_reachable(block) || live_in.contains(block) || kill.contains(block)
                {
                    continue;
                }
                let live_out =
                    self.cfg.successors(block).iter().any(|&succ| live_in.contains(succ));
                if live_out {
                    live_in.insert(block);
                    changed = true;
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

    fn apply(self, func: &mut Function) {
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

        let mut current = None;
        let mut changed = false;
        for &inst_id in &func.blocks[block].instructions {
            match func.inst(inst_id).kind {
                InstKind::MLoad(addr)
                    if FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                        == Some(self.info.slot) =>
                {
                    let Some(value) = current else { return false };
                    self.replace_load(inst_id, value);
                    changed = true;
                }
                InstKind::MStore(addr, value)
                    if FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                        == Some(self.info.slot) =>
                {
                    current = Some(mir_utils::resolve_replacement(value, &self.replacements));
                    self.remove_store(inst_id);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    fn rewrite_single_store(&mut self, func: &Function) -> bool {
        let [store] = self.info.stores.as_slice() else { return false };
        let stored_value = mir_utils::resolve_replacement(store.value, &self.replacements);

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
            self.replace_load(load.inst, stored_value);
        }
        self.remove_store(store.inst);
        true
    }

    fn inst_position(func: &Function, block: BlockId, inst: InstId) -> Option<usize> {
        func.blocks[block].instructions.iter().position(|&candidate| candidate == inst)
    }

    fn replace_load(&mut self, inst_id: InstId, value: ValueId) {
        if let Some(&load_value) = self.inst_results.get(&inst_id) {
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

        let instruction_count = func.blocks[block].instructions.len();
        for index in 0..instruction_count {
            let inst_id = func.blocks[block].instructions[index];
            match func.inst(inst_id).kind {
                InstKind::MLoad(addr)
                    if FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                        == Some(self.info.slot) =>
                {
                    let Some(value) = current else {
                        self.failed = true;
                        return;
                    };
                    self.replace_load(inst_id, value);
                }
                InstKind::MStore(addr, value)
                    if FrameSlotPromoter::promotable_slot(func, self.aa, addr)
                        == Some(self.info.slot) =>
                {
                    current = Some(mir_utils::resolve_replacement(value, &self.replacements));
                    self.remove_store(inst_id);
                }
                _ => {}
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
