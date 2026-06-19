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
    analysis::CfgInfo,
    mir::{
        BlockId, Function, InstId, InstKind, Instruction, MirType, Terminator, Value, ValueId,
        utils::{self as mir_utils, repair_reachability_phis},
    },
    pass::FunctionPass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

const LOW_MEMORY_START: u64 = 0x80;

/// Statistics for one frame promotion run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramePromotionStats {
    /// Number of distinct compiler-local slots promoted.
    pub slots_promoted: usize,
    /// Number of local-slot loads replaced by SSA values.
    pub loads_promoted: usize,
    /// Number of local-slot stores removed.
    pub stores_promoted: usize,
    /// Number of phi nodes inserted.
    pub phis_inserted: usize,
}

/// A compiler-owned memory slot promoted to SSA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PromotedSlot {
    /// Slot addressed relative to the internal-call frame pointer.
    InternalFrame(u64),
    /// Slot addressed in the external entry's compiler-owned low-memory locals.
    ExternalLocal(u64),
}

/// Per-slot information produced by frame-slot promotion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotedSlotSummary {
    /// Promoted compiler-owned slot.
    pub slot: PromotedSlot,
    /// Blocks where the slot had an upward-exposed load before promotion.
    pub use_blocks: Vec<BlockId>,
    /// Blocks where the slot was defined before promotion.
    pub def_blocks: Vec<BlockId>,
    /// Blocks where SSA phis were inserted.
    pub phi_blocks: Vec<BlockId>,
    /// SSA phi values inserted for this slot.
    pub phi_values: Vec<ValueId>,
    /// Number of loads replaced by SSA values.
    pub loads_promoted: usize,
    /// Number of stores removed.
    pub stores_promoted: usize,
}

impl FramePromotionStats {
    /// Returns the total number of MIR edits made by this pass.
    pub const fn total(self) -> usize {
        self.loads_promoted + self.stores_promoted + self.phis_inserted
    }
}

/// Promotes non-escaping compiler-local slots to SSA values.
#[derive(Debug, Default)]
pub struct FrameSlotPromoter {
    stats: FramePromotionStats,
    summaries: Vec<PromotedSlotSummary>,
}

/// Function pass for internal-frame scalar promotion.
pub struct FrameSlotPromotionPass;

impl FunctionPass for FrameSlotPromotionPass {
    fn name(&self) -> &str {
        "frame-slot-promotion"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = FrameSlotPromoter::new().run(func).total() != 0;
        repair_reachability_phis(func);
        changed
    }
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
    use_blocks: FxHashSet<BlockId>,
    def_blocks: FxHashSet<BlockId>,
    access_blocks: FxHashSet<BlockId>,
}

impl SlotAccessInfo {
    fn new(slot: PromotableSlot) -> Self {
        Self {
            slot,
            loads: Vec::new(),
            stores: Vec::new(),
            use_blocks: FxHashSet::default(),
            def_blocks: FxHashSet::default(),
            access_blocks: FxHashSet::default(),
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
    replacements: FxHashMap<ValueId, ValueId>,
    dead: FxHashSet<InstId>,
    phis: FxHashMap<BlockId, PendingPhi>,
    /// Blocks where this slot is live-in. Used to place phis only where the slot
    /// is actually live (pruned SSA): forcing a phi at a multi-predecessor block
    /// where the slot is dead can chain back to the entry with no reaching value
    /// and spuriously abort the whole promotion.
    live_in: FxHashSet<BlockId>,
    /// Blocks selected by pruned iterated-dominance-frontier phi placement.
    phi_blocks: FxHashSet<BlockId>,
    failed: bool,
    loads_promoted: usize,
    stores_promoted: usize,
}

fn sorted_blocks(blocks: &FxHashSet<BlockId>) -> Vec<BlockId> {
    let mut blocks: Vec<_> = blocks.iter().copied().collect();
    blocks.sort_by_key(|block| block.index());
    blocks
}

impl FrameSlotPromoter {
    /// Creates a new compiler-local-slot promoter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the statistics from the most recent run.
    pub const fn stats(&self) -> FramePromotionStats {
        self.stats
    }

    /// Returns per-slot promotion summaries from the most recent run.
    pub fn summaries(&self) -> &[PromotedSlotSummary] {
        &self.summaries
    }

    /// Runs compiler-local-slot promotion on a function.
    pub fn run(&mut self, func: &mut Function) -> FramePromotionStats {
        self.stats = FramePromotionStats::default();
        self.summaries.clear();

        if Self::has_global_observation_barrier(func) {
            return self.stats;
        }

        let cfg = CfgInfo::new(func);
        let slots = Self::collect_promotable_slots(func, &cfg);
        if slots.is_empty() {
            return self.stats;
        };

        for info in slots {
            let inst_results = func.inst_results();
            let mut builder = SlotSsaBuilder::new(&info, &cfg, &inst_results);
            if builder.run(func) {
                self.stats.slots_promoted += 1;
                self.stats.loads_promoted += builder.loads_promoted;
                self.stats.stores_promoted += builder.stores_promoted;
                self.stats.phis_inserted += builder.phis.len();
                self.summaries.push(builder.summary());
                builder.apply(func);
            }
        }

        self.stats
    }

    fn has_global_observation_barrier(func: &Function) -> bool {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(func.instructions[inst_id].kind, InstKind::Gas | InstKind::MSize)
            })
        })
    }

    fn collect_promotable_slots(func: &Function, cfg: &CfgInfo) -> Vec<SlotAccessInfo> {
        let mut accesses: FxHashMap<PromotableSlot, SlotAccessInfo> = FxHashMap::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                continue;
            }

            for &inst_id in &block.instructions {
                let kind = &func.instructions[inst_id].kind;
                match *kind {
                    InstKind::MLoad(addr) => {
                        if let Some(slot) = Self::promotable_slot(func, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot))
                                .note_load(block_id, inst_id);
                        }
                    }
                    InstKind::MStore(addr, value) => {
                        if let Some(slot) = Self::promotable_slot(func, addr) {
                            accesses
                                .entry(slot)
                                .or_insert_with(|| SlotAccessInfo::new(slot))
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
                    Self::internal_frame_slot_safe(func, offset)
                }
                PromotableSlot::ExternalLocal(addr) => Self::external_local_slot_safe(func, addr),
            })
            .collect();
        slots.sort_by_key(|info| info.slot);
        slots
    }

    fn promotable_slot(func: &Function, value: ValueId) -> Option<PromotableSlot> {
        Self::internal_frame_offset(func, value)
            .map(PromotableSlot::InternalFrame)
            .or_else(|| Self::external_local_addr(func, value).map(PromotableSlot::ExternalLocal))
    }

    fn external_local_addr(func: &Function, value: ValueId) -> Option<u64> {
        Self::external_local_addr_with_depth(func, value, 0)
    }

    fn external_local_addr_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<u64> {
        if depth > 8 {
            return None;
        }

        if let Some(addr) = func.value_u64(value)
            && Self::external_local_addr_in_range(func, addr).is_some()
        {
            return Some(addr);
        }

        let Value::Inst(inst_id) = func.values[value] else { return None };
        match func.instructions[inst_id].kind {
            InstKind::Add(a, b) => Self::external_local_add_offset(func, a, b, depth)
                .or_else(|| Self::external_local_add_offset(func, b, a, depth)),
            _ => None,
        }
    }

    fn external_local_addr_in_range(func: &Function, addr: u64) -> Option<u64> {
        let local_end = LOW_MEMORY_START.checked_add(func.internal_frame_size)?;
        (addr >= LOW_MEMORY_START
            && addr < local_end
            && (addr - LOW_MEMORY_START).is_multiple_of(32))
        .then_some(addr)
    }

    fn external_local_add_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<u64> {
        let base = Self::external_local_addr_with_depth(func, base, depth + 1)?;
        let addr = base.checked_add(func.value_u64(offset)?)?;
        Self::external_local_addr_in_range(func, addr)
    }

    fn internal_frame_offset(func: &Function, value: ValueId) -> Option<u64> {
        Self::internal_frame_offset_with_depth(func, value, 0)
    }

    fn internal_frame_offset_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<u64> {
        if depth > 8 {
            return None;
        }

        match func.values[value] {
            Value::Inst(inst_id) => match func.instructions[inst_id].kind {
                InstKind::InternalFrameAddr(offset) => Some(offset),
                InstKind::Add(a, b) => Self::internal_frame_add_offset(func, a, b, depth)
                    .or_else(|| Self::internal_frame_add_offset(func, b, a, depth)),
                _ => None,
            },
            _ => None,
        }
    }

    fn internal_frame_add_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<u64> {
        let base = Self::internal_frame_offset_with_depth(func, base, depth + 1)?;
        base.checked_add(func.value_u64(offset)?)
    }

    fn external_local_slot_safe(func: &Function, slot_addr: u64) -> bool {
        if mir_utils::ranges_overlap(
            slot_addr,
            32,
            LOW_MEMORY_START,
            func.external_static_return_size,
        ) {
            return false;
        }

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if Self::inst_may_observe_external_slot(
                    func,
                    &func.instructions[inst_id].kind,
                    slot_addr,
                ) {
                    return false;
                }
            }
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_external_slot(func, term, slot_addr)
            {
                return false;
            }
        }

        true
    }

    fn internal_frame_slot_safe(func: &Function, slot_offset: u64) -> bool {
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if Self::inst_may_observe_internal_slot(
                    func,
                    &func.instructions[inst_id].kind,
                    slot_offset,
                ) {
                    return false;
                }
            }
            if let Some(term) = &block.terminator
                && Self::terminator_may_observe_internal_slot(func, term, slot_offset)
            {
                return false;
            }
        }

        true
    }

    fn inst_may_observe_internal_slot(func: &Function, kind: &InstKind, slot_offset: u64) -> bool {
        match *kind {
            InstKind::MLoad(addr) => {
                !Self::is_exact_internal_slot_access(func, addr, slot_offset)
                    && Self::internal_frame_range_may_overlap(func, addr, Some(32), slot_offset)
            }
            InstKind::MStore(addr, value) => {
                (!Self::is_exact_internal_slot_access(func, addr, slot_offset)
                    && Self::internal_frame_range_may_overlap(func, addr, Some(32), slot_offset))
                    || Self::internal_frame_offset(func, value) == Some(slot_offset)
            }
            InstKind::MStore8(addr, _) => {
                Self::internal_frame_range_may_overlap(func, addr, Some(1), slot_offset)
            }
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => Self::internal_frame_range_may_overlap(
                func,
                addr,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::MCopy(dest, src, size) => {
                let size = func.value_u64(size);
                Self::internal_frame_range_may_overlap(func, dest, size, slot_offset)
                    || Self::internal_frame_range_may_overlap(func, src, size, slot_offset)
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => Self::internal_frame_range_may_overlap(
                func,
                dest,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => Self::internal_frame_range_may_overlap(
                func,
                addr,
                func.value_u64(size),
                slot_offset,
            ),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                Self::internal_frame_range_may_overlap(
                    func,
                    args_offset,
                    func.value_u64(args_size),
                    slot_offset,
                ) || Self::internal_frame_range_may_overlap(
                    func,
                    ret_offset,
                    func.value_u64(ret_size),
                    slot_offset,
                )
            }
            InstKind::Add(a, b) => {
                let exact_frame_addr = Self::internal_frame_add_offset(func, a, b, 0)
                    .or_else(|| Self::internal_frame_add_offset(func, b, a, 0))
                    .is_some();
                !exact_frame_addr
                    && kind
                        .operands()
                        .iter()
                        .any(|&value| Self::internal_frame_offset(func, value) == Some(slot_offset))
            }
            _ => kind
                .operands()
                .iter()
                .any(|&value| Self::internal_frame_offset(func, value) == Some(slot_offset)),
        }
    }

    fn terminator_may_observe_internal_slot(
        func: &Function,
        term: &Terminator,
        slot_offset: u64,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::internal_frame_range_may_overlap(
                    func,
                    *offset,
                    func.value_u64(*size),
                    slot_offset,
                )
            }
            _ => term
                .operands()
                .iter()
                .any(|&value| Self::internal_frame_offset(func, value) == Some(slot_offset)),
        }
    }

    fn inst_may_observe_external_slot(func: &Function, kind: &InstKind, slot_addr: u64) -> bool {
        match *kind {
            InstKind::MLoad(addr) | InstKind::MStore(addr, _) => {
                !Self::is_exact_external_slot_access(func, addr, slot_addr)
                    && Self::memory_range_may_overlap(func, addr, Some(32), slot_addr)
            }
            InstKind::MStore8(addr, _) => {
                Self::memory_range_may_overlap(func, addr, Some(1), slot_addr)
            }
            InstKind::Keccak256(addr, size)
            | InstKind::Log0(addr, size)
            | InstKind::ReturnDataCopy(addr, _, size)
            | InstKind::CodeCopy(addr, _, size)
            | InstKind::CalldataCopy(addr, _, size) => {
                Self::memory_range_may_overlap(func, addr, func.value_u64(size), slot_addr)
            }
            InstKind::MCopy(dest, src, size) => {
                let size = func.value_u64(size);
                Self::memory_range_may_overlap(func, dest, size, slot_addr)
                    || Self::memory_range_may_overlap(func, src, size, slot_addr)
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                Self::memory_range_may_overlap(func, dest, func.value_u64(size), slot_addr)
            }
            InstKind::Log1(addr, size, _)
            | InstKind::Log2(addr, size, _, _)
            | InstKind::Log3(addr, size, _, _, _)
            | InstKind::Log4(addr, size, _, _, _, _) => {
                Self::memory_range_may_overlap(func, addr, func.value_u64(size), slot_addr)
            }
            InstKind::Call { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::InternalCall { .. }
            | InstKind::Create(_, _, _)
            | InstKind::Create2(_, _, _, _)
            | InstKind::MSize => true,
            _ => false,
        }
    }

    fn terminator_may_observe_external_slot(
        func: &Function,
        term: &Terminator,
        slot_addr: u64,
    ) -> bool {
        match term {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                Self::memory_range_may_overlap(func, *offset, func.value_u64(*size), slot_addr)
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::SelfDestruct { .. } => false,
        }
    }

    fn is_exact_external_slot_access(func: &Function, addr: ValueId, slot_addr: u64) -> bool {
        Self::external_local_addr(func, addr) == Some(slot_addr)
    }

    fn is_exact_internal_slot_access(func: &Function, addr: ValueId, slot_offset: u64) -> bool {
        Self::internal_frame_offset(func, addr) == Some(slot_offset)
    }

    fn internal_frame_range_may_overlap(
        func: &Function,
        addr: ValueId,
        size: Option<u64>,
        slot_offset: u64,
    ) -> bool {
        let Some(offset) = Self::internal_frame_offset(func, addr) else { return false };
        let Some(size) = size else { return true };
        if size == 0 {
            return false;
        }
        mir_utils::ranges_overlap(offset, size, slot_offset, 32)
    }

    fn memory_range_may_overlap(
        func: &Function,
        addr: ValueId,
        size: Option<u64>,
        slot_addr: u64,
    ) -> bool {
        let Some(size) = size else { return true };
        if size == 0 {
            return false;
        }
        let Some(addr) = func.value_u64(addr) else { return true };
        mir_utils::ranges_overlap(addr, size, slot_addr, 32)
    }
}

impl<'a> SlotSsaBuilder<'a> {
    fn new(
        info: &'a SlotAccessInfo,
        cfg: &'a CfgInfo,
        inst_results: &'a FxHashMap<InstId, ValueId>,
    ) -> Self {
        Self {
            info,
            cfg,
            inst_results,
            replacements: FxHashMap::default(),
            dead: FxHashSet::default(),
            phis: FxHashMap::default(),
            live_in: FxHashSet::default(),
            phi_blocks: FxHashSet::default(),
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

    fn compute_live_in(&self, func: &Function) -> FxHashSet<BlockId> {
        let mut gen_set: FxHashSet<BlockId> = FxHashSet::default();
        let mut kill: FxHashSet<BlockId> = FxHashSet::default();

        for block in func.blocks.indices() {
            if !self.cfg.is_reachable(block) {
                continue;
            }

            let mut saw_store = false;
            for &inst_id in &func.blocks[block].instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MLoad(addr)
                        if !saw_store
                            && FrameSlotPromoter::promotable_slot(func, addr)
                                == Some(self.info.slot) =>
                    {
                        gen_set.insert(block);
                    }
                    InstKind::MStore(addr, _)
                        if FrameSlotPromoter::promotable_slot(func, addr)
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
                if !self.cfg.is_reachable(block)
                    || live_in.contains(&block)
                    || kill.contains(&block)
                {
                    continue;
                }
                let live_out = self.cfg.successors(block).iter().any(|succ| live_in.contains(succ));
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
        live_in: &FxHashSet<BlockId>,
    ) -> FxHashSet<BlockId> {
        let frontiers = self.compute_dominance_frontiers(func);
        let mut phi_blocks = FxHashSet::default();
        let mut worklist = sorted_blocks(&self.info.def_blocks);

        while let Some(block) = worklist.pop() {
            let Some(frontier) = frontiers.get(block.index()) else { continue };
            for &frontier_block in frontier {
                if !live_in.contains(&frontier_block) || !phi_blocks.insert(frontier_block) {
                    continue;
                }
                worklist.push(frontier_block);
            }
        }

        phi_blocks
    }

    fn compute_dominance_frontiers(&self, func: &Function) -> Vec<Vec<BlockId>> {
        let mut frontiers = vec![Vec::new(); func.blocks.len()];
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
                    if !frontiers[runner.index()].contains(&block) {
                        frontiers[runner.index()].push(block);
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
        self.rename_block(func, func.entry_block, None);
        !self.failed
    }

    fn apply(self, func: &mut Function) {
        for pending in self.phis.values() {
            let mut incoming = pending.incoming.clone();
            incoming.sort_by_key(|(block, _)| block.index());
            func.instructions[pending.inst].kind = InstKind::Phi(incoming);
            let insert_pos = func.blocks[pending.block]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
                .count();
            func.blocks[pending.block].instructions.insert(insert_pos, pending.inst);
        }

        func.replace_uses_canonicalized(&self.replacements);

        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !self.dead.contains(id));
        }
    }

    fn rewrite_single_block(&mut self, func: &Function) -> bool {
        if self.info.access_blocks.len() != 1 {
            return false;
        }
        let block = self.info.access_blocks.iter().copied().next().expect("checked len above");
        if !self.cfg.is_reachable(block) {
            self.failed = true;
            return true;
        }

        let mut current = None;
        let mut changed = false;
        for &inst_id in &func.blocks[block].instructions {
            match func.instructions[inst_id].kind {
                InstKind::MLoad(addr)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.info.slot) =>
                {
                    let Some(value) = current else { return false };
                    self.replace_load(inst_id, value);
                    changed = true;
                }
                InstKind::MStore(addr, value)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.info.slot) =>
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

        let insts = func.blocks[block].instructions.clone();
        for inst_id in insts {
            match func.instructions[inst_id].kind {
                InstKind::MLoad(addr)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.info.slot) =>
                {
                    let Some(value) = current else {
                        self.failed = true;
                        return;
                    };
                    self.replace_load(inst_id, value);
                }
                InstKind::MStore(addr, value)
                    if FrameSlotPromoter::promotable_slot(func, addr) == Some(self.info.slot) =>
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, FunctionId};
    use solar_interface::Ident;

    fn test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    fn count_active_frame_ops(func: &Function, offset: u64) -> (usize, usize) {
        let mut loads = 0;
        let mut stores = 0;

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MLoad(addr)
                        if FrameSlotPromoter::internal_frame_offset(func, addr) == Some(offset) =>
                    {
                        loads += 1;
                    }
                    InstKind::MStore(addr, _)
                        if FrameSlotPromoter::internal_frame_offset(func, addr) == Some(offset) =>
                    {
                        stores += 1;
                    }
                    _ => {}
                }
            }
        }

        (loads, stores)
    }

    #[test]
    fn promotes_loop_carried_frame_slot() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();

        let frame = builder.internal_frame_addr(128);
        let zero = builder.imm_u64(0);
        let limit = builder.imm_u64(4);
        builder.mstore(frame, zero);
        builder.jump(header);

        builder.switch_to_block(header);
        let header_frame = builder.internal_frame_addr(128);
        let i = builder.mload(header_frame);
        let cond = builder.lt(i, limit);
        builder.branch(cond, body, exit);

        builder.switch_to_block(body);
        let body_frame = builder.internal_frame_addr(128);
        let current = builder.mload(body_frame);
        let one = builder.imm_u64(1);
        let next = builder.add(current, one);
        let body_frame = builder.internal_frame_addr(128);
        builder.mstore(body_frame, next);
        builder.jump(header);

        builder.switch_to_block(exit);
        let exit_frame = builder.internal_frame_addr(128);
        let result = builder.mload(exit_frame);
        builder.ret([result]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(stats.loads_promoted, 3);
        assert_eq!(stats.stores_promoted, 2);
        assert_eq!(stats.phis_inserted, 1);
        assert_eq!(count_active_frame_ops(&func, 128), (0, 0));

        let Some(Terminator::Return { values }) = &func.blocks[exit].terminator else {
            panic!("expected return");
        };
        assert_ne!(values.as_slice(), &[result]);
    }

    #[test]
    fn skips_escaped_frame_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        let size = builder.imm_u64(32);
        builder.mstore(frame, value);
        let hash = builder.keccak256(frame, size);
        let loaded = builder.mload(frame);
        builder.ret([hash, loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.total(), 0);
        assert_eq!(count_active_frame_ops(&func, 128), (1, 1));
    }

    #[test]
    fn skips_gas_observed_functions() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        builder.mstore(frame, value);
        builder.gas();
        let loaded = builder.mload(frame);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.total(), 0);
        assert_eq!(count_active_frame_ops(&func, 128), (1, 1));
    }

    #[test]
    fn promotes_across_internal_calls() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(128);
        let value = builder.imm_u64(42);
        builder.mstore(frame, value);
        builder.internal_call(FunctionId::from_usize(0), Vec::new(), None, 0);
        let loaded = builder.mload(frame);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(count_active_frame_ops(&func, 128), (0, 0));
    }

    #[test]
    fn promotes_frame_address_with_constant_offset() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.internal_frame_addr(128);
        let offset = builder.imm_u64(32);
        let addr = builder.add(base, offset);
        let value = builder.imm_u64(99);
        builder.mstore(addr, value);
        let base = builder.internal_frame_addr(128);
        let offset = builder.imm_u64(32);
        let addr = builder.add(base, offset);
        let loaded = builder.mload(addr);
        builder.ret([loaded]);

        let mut pass = FrameSlotPromoter::new();
        let stats = pass.run(&mut func);

        assert_eq!(stats.slots_promoted, 1);
        assert_eq!(count_active_frame_ops(&func, 160), (0, 0));
    }
}
