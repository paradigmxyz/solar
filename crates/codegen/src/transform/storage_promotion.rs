//! Storage scalar promotion for simple loop-carried storage updates.
//!
//! This pass recognizes loops that repeatedly update one or more exact storage
//! slots and rewrites the loop to update memory-backed scalars instead. Final
//! values are stored back to storage once on each clean loop exit.
//!
//! Safety contract:
//! - promote only exact storage aliases that are loop-invariant
//! - promote multiple slots only when they are pairwise provably disjoint
//! - reject loops with calls, unknown storage writes, or non-isolated exits
//! - flush dirty promoted values before any clean observable exit
//! - skip the flush on revert exits: `revert`/`invalid` roll back every storage write of the frame,
//!   so the unflushed slot is unobservable there; reads of the promoted slot on those paths are
//!   rewritten to the memory temp so revert data still sees the current value
//! - leave loop-variant mapping/array slots in storage

use crate::{
    analysis::{AliasAnalysis, Loop, LoopAnalyzer},
    memory::EvmMemoryLayout,
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, StorageAlias,
        Terminator, Value, ValueId, utils as mir_utils,
    },
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

/// Function pass for loop-carried storage scalar promotion.
pub(crate) struct StorageScalarPromotion;

impl MirPass for StorageScalarPromotion {
    fn name(&self) -> &'static str {
        "storage-promotion"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let mut promoter = StorageScalarPromoter::new();
            let stats = promoter.run(func);
            stats.loops_promoted + stats.loads_promoted + stats.stores_promoted != 0
        })
    }
}

/// Statistics from storage scalar promotion.
#[derive(Clone, Debug, Default)]
struct StoragePromotionStats {
    /// Number of loops promoted.
    loops_promoted: usize,
    /// Number of storage loads rewritten to memory loads.
    loads_promoted: usize,
    /// Number of storage stores rewritten to memory stores.
    stores_promoted: usize,
}

/// Promotes loop-carried storage values to memory-backed scalars.
#[derive(Debug, Default)]
struct StorageScalarPromoter {
    stats: StoragePromotionStats,
}

#[derive(Clone, Debug)]
struct Candidate {
    slot_value: ValueId,
    slot: StorageAlias,
    preheader: BlockId,
    init_store: Option<InstId>,
    needs_initial_load: bool,
}

#[derive(Clone, Debug)]
struct PromotedCandidate {
    candidate: Candidate,
    temp_addr: ValueId,
    dirty_addr: Option<ValueId>,
    dirty_value: Option<ValueId>,
}

impl StorageScalarPromoter {
    /// Creates a new storage scalar promotion pass.
    fn new() -> Self {
        Self::default()
    }

    /// Runs the pass on one function.
    fn run(&mut self, func: &mut Function) -> &StoragePromotionStats {
        self.stats = StoragePromotionStats::default();

        // The pass currently introduces absolute low-memory temporaries, so it
        // only handles externally callable runtime entries.
        if func.selector.is_none() {
            return &self.stats;
        }

        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);

        // Promoting a loop can split its exit blocks and relocate the final
        // stores into new blocks, which invalidates the block sets of every
        // enclosing loop. Recompute loop info after each successful promotion
        // so later promotions reason about an accurate CFG. This terminates:
        // each promotion rewrites all of its loop's storage stores to memory
        // stores and only inserts new storage stores outside that loop.
        loop {
            let mut analyzer = LoopAnalyzer::new();
            let loop_info = analyzer.analyze(func);
            let mut loops: Vec<Loop> = loop_info.all_loops().cloned().collect();
            loops.sort_by_key(|loop_data| loop_data.header.index());

            let mut promoted = false;
            for loop_data in loops {
                if let Some(candidates) = self.find_initialized_candidates(func, &loop_data) {
                    self.promote_initialized_candidates(func, &loop_data, &candidates);
                } else if let Some(candidate) = self.find_candidate(func, &loop_data) {
                    self.promote_loop(func, &loop_data, &candidate);
                } else {
                    continue;
                }
                promoted = true;
                break;
            }
            if !promoted {
                break;
            }
        }

        &self.stats
    }

    fn find_candidate(&self, func: &Function, loop_data: &Loop) -> Option<Candidate> {
        let preheader = loop_data.preheader?;
        if loop_data.exit_blocks.is_empty() || !self.has_isolated_promotable_exits(func, loop_data)
        {
            return None;
        }
        if !self.loop_has_no_unpromotable_side_effects(func, loop_data) {
            return None;
        }

        let mut slot: Option<StorageAlias> = None;
        let mut slot_value: Option<ValueId> = None;
        let mut saw_loop_store = false;

        for block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::SStore(store_slot, _) = &func.instructions[inst_id].kind {
                    let store_key =
                        self.storage_alias_for_loop_value(func, *store_slot, loop_data)?;
                    match slot {
                        Some(existing) if existing != store_key => return None,
                        Some(_) => {}
                        None => {
                            slot = Some(store_key);
                            slot_value = Some(*store_slot);
                        }
                    }
                    saw_loop_store = true;
                }
            }
        }

        if !saw_loop_store {
            return None;
        }

        let (slot, slot_value) = (slot?, slot_value?);
        let rewrite_blocks = self.promotion_block_ids(func, loop_data);
        if !self.loop_storage_accesses_are_safe(func, &rewrite_blocks, &slot) {
            return None;
        }
        let saw_loop_load = rewrite_blocks.iter().any(|&block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                matches!(
                    &func.instructions[inst_id].kind,
                    InstKind::SLoad(load_slot)
                        if self.storage_alias(func, inst_id, *load_slot) == slot
                )
            })
        });
        let init_store = self.find_preheader_init_store(func, preheader, &slot);
        if let Some(init_store) = init_store
            && !self.preheader_tail_is_safe(func, preheader, init_store, &slot)
        {
            return None;
        }
        let slot_value = if let Some(init_store) = init_store {
            self.store_slot(func, init_store)?
        } else if self.value_defined_in_loop(func, slot_value, loop_data) {
            return None;
        } else {
            slot_value
        };
        let needs_initial_load = init_store.is_none() && saw_loop_load;

        Some(Candidate { slot_value, slot, preheader, init_store, needs_initial_load })
    }

    fn find_initialized_candidates(
        &self,
        func: &Function,
        loop_data: &Loop,
    ) -> Option<Vec<Candidate>> {
        let preheader = loop_data.preheader?;
        if loop_data.exit_blocks.is_empty() || !self.has_isolated_promotable_exits(func, loop_data)
        {
            return None;
        }
        if !self.loop_has_no_unpromotable_side_effects(func, loop_data) {
            return None;
        }

        let mut stores: FxHashMap<StorageAlias, ValueId> = FxHashMap::default();
        for block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::SStore(store_slot, _) = &func.instructions[inst_id].kind {
                    let store_key =
                        self.storage_alias_for_loop_value(func, *store_slot, loop_data)?;
                    stores.entry(store_key).or_insert(*store_slot);
                }
            }
        }

        if stores.len() <= 1 {
            return None;
        }

        // Distinct alias keys can still name the same runtime slot (for
        // example two symbolic mapping slots whose keys happen to be equal).
        // Promoting them to separate memory temps would desynchronize loads
        // and stores, so require every pair to be provably disjoint.
        let keys: Vec<StorageAlias> = stores.keys().copied().collect();
        if keys
            .iter()
            .enumerate()
            .any(|(i, a)| keys[i + 1..].iter().any(|b| self.storage_may_alias(*a, *b)))
        {
            return None;
        }

        let mut candidates = Vec::with_capacity(stores.len());
        for (slot, _) in stores {
            let init_store = self.find_preheader_init_store(func, preheader, &slot)?;
            let slot_value = self.store_slot(func, init_store)?;
            candidates.push(Candidate {
                slot_value,
                slot,
                preheader,
                init_store: Some(init_store),
                needs_initial_load: false,
            });
        }
        candidates.sort_by_key(|candidate| candidate.init_store.map(|inst| inst.index()));

        let rewrite_blocks = self.promotion_block_ids(func, loop_data);
        if !self.loop_storage_accesses_are_safe_for_candidates(func, &rewrite_blocks, &candidates) {
            return None;
        }
        if !self.preheader_tail_is_safe_for_candidates(func, preheader, &candidates) {
            return None;
        }

        Some(candidates)
    }

    fn has_isolated_promotable_exits(&self, func: &Function, loop_data: &Loop) -> bool {
        loop_data.exit_blocks.iter().all(|&exit| {
            func.blocks[exit].predecessors.iter().all(|&pred| loop_data.blocks.contains(pred))
                && if self.exit_rolls_back(func, exit) {
                    self.rollback_exit_has_no_observable_effects(func, exit)
                } else {
                    !matches!(func.blocks[exit].terminator, Some(Terminator::SelfDestruct { .. }))
                }
        })
    }

    /// Returns true if the exit block ends by rolling back all storage writes
    /// of the frame, which makes skipping the promoted-value flush sound.
    fn exit_rolls_back(&self, func: &Function, exit: BlockId) -> bool {
        matches!(
            func.blocks[exit].terminator,
            Some(Terminator::Revert { .. } | Terminator::Invalid)
        )
    }

    /// A rollback exit must not contain calls (a callee could observe the
    /// unflushed slot and leak that observation into the revert data) or
    /// other instructions whose results escape the rolled-back frame.
    fn rollback_exit_has_no_observable_effects(&self, func: &Function, exit: BlockId) -> bool {
        func.blocks[exit].instructions.iter().all(|&inst_id| {
            !matches!(
                &func.instructions[inst_id].kind,
                InstKind::Call { .. }
                    | InstKind::StaticCall { .. }
                    | InstKind::DelegateCall { .. }
                    | InstKind::InternalCall { .. }
                    | InstKind::Create(_, _, _)
                    | InstKind::Create2(_, _, _, _)
                    | InstKind::Gas
            )
        })
    }

    /// Blocks whose storage accesses are checked and rewritten by promotion:
    /// the loop body plus rollback exits, whose reads of the promoted slot
    /// must observe the memory temp instead of the stale storage value.
    fn promotion_block_ids(&self, func: &Function, loop_data: &Loop) -> Vec<BlockId> {
        let mut blocks: Vec<BlockId> = loop_data.blocks.iter().collect();
        blocks.extend(
            loop_data.exit_blocks.iter().copied().filter(|&exit| self.exit_rolls_back(func, exit)),
        );
        blocks
    }

    fn loop_has_no_unpromotable_side_effects(&self, func: &Function, loop_data: &Loop) -> bool {
        for block_id in &loop_data.blocks {
            if matches!(
                func.blocks[block_id].terminator,
                Some(
                    Terminator::Return { .. }
                        | Terminator::Revert { .. }
                        | Terminator::ReturnData { .. }
                        | Terminator::Stop
                        | Terminator::SelfDestruct { .. }
                        | Terminator::Invalid
                )
            ) {
                return false;
            }

            for &inst_id in &func.blocks[block_id].instructions {
                let inst = &func.instructions[inst_id];
                match &inst.kind {
                    InstKind::SLoad(_) | InstKind::SStore(_, _) => {}
                    InstKind::TStore(_, _)
                    | InstKind::Call { .. }
                    | InstKind::StaticCall { .. }
                    | InstKind::DelegateCall { .. }
                    | InstKind::InternalCall { .. }
                    | InstKind::Create(_, _, _)
                    | InstKind::Create2(_, _, _, _)
                    | InstKind::Gas => return false,
                    _ => {}
                }
            }
        }
        true
    }

    fn loop_storage_accesses_are_safe(
        &self,
        func: &Function,
        blocks: &[BlockId],
        candidate: &StorageAlias,
    ) -> bool {
        for &block_id in blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        if alias != *candidate && self.storage_may_alias(*candidate, alias) {
                            return false;
                        }
                    }
                    InstKind::SStore(slot, _) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        if alias != *candidate {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }
        true
    }

    fn loop_storage_accesses_are_safe_for_candidates(
        &self,
        func: &Function,
        blocks: &[BlockId],
        candidates: &[Candidate],
    ) -> bool {
        for &block_id in blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        if self.candidate_index(candidates, &alias).is_none()
                            && candidates
                                .iter()
                                .any(|candidate| self.storage_may_alias(candidate.slot, alias))
                        {
                            return false;
                        }
                    }
                    InstKind::SStore(slot, _) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        if self.candidate_index(candidates, &alias).is_none() {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }
        true
    }

    fn find_preheader_init_store(
        &self,
        func: &Function,
        preheader: BlockId,
        slot: &StorageAlias,
    ) -> Option<InstId> {
        func.blocks[preheader].instructions.iter().rev().copied().find(|&inst_id| {
            matches!(
                &func.instructions[inst_id].kind,
                InstKind::SStore(store_slot, _)
                    if self.storage_alias(func, inst_id, *store_slot) == *slot
            )
        })
    }

    fn store_slot(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        match func.instructions[inst_id].kind {
            InstKind::SStore(slot, _) => Some(slot),
            _ => None,
        }
    }

    fn preheader_tail_is_safe(
        &self,
        func: &Function,
        preheader: BlockId,
        init_store: InstId,
        slot: &StorageAlias,
    ) -> bool {
        let Some(init_pos) =
            func.blocks[preheader].instructions.iter().position(|&inst_id| inst_id == init_store)
        else {
            return false;
        };

        for &inst_id in &func.blocks[preheader].instructions[init_pos + 1..] {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(load_slot) => {
                    let alias = self.storage_alias(func, inst_id, *load_slot);
                    if alias != *slot && self.storage_may_alias(*slot, alias) {
                        return false;
                    }
                }
                InstKind::MStore(_, _) | InstKind::MStore8(_, _) | InstKind::MCopy(_, _, _) => {}
                kind if kind.has_side_effects() => return false,
                InstKind::Gas => return false,
                _ => {}
            }
        }

        true
    }

    fn preheader_tail_is_safe_for_candidates(
        &self,
        func: &Function,
        preheader: BlockId,
        candidates: &[Candidate],
    ) -> bool {
        let Some(first_init) = candidates
            .iter()
            .filter_map(|candidate| candidate.init_store)
            .filter_map(|inst_id| {
                func.blocks[preheader]
                    .instructions
                    .iter()
                    .position(|&candidate| candidate == inst_id)
            })
            .min()
        else {
            return false;
        };

        for &inst_id in &func.blocks[preheader].instructions[first_init + 1..] {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(load_slot) | InstKind::SStore(load_slot, _) => {
                    let alias = self.storage_alias(func, inst_id, *load_slot);
                    if self.candidate_index(candidates, &alias).is_none()
                        && candidates
                            .iter()
                            .any(|candidate| self.storage_may_alias(candidate.slot, alias))
                    {
                        return false;
                    }
                }
                InstKind::MStore(_, _) | InstKind::MStore8(_, _) | InstKind::MCopy(_, _, _) => {}
                kind if kind.has_side_effects() => return false,
                InstKind::Gas => return false,
                _ => {}
            }
        }

        true
    }

    fn promote_initialized_candidates(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        candidates: &[Candidate],
    ) {
        let promoted: Vec<_> = candidates
            .iter()
            .cloned()
            .map(|candidate| PromotedCandidate {
                candidate,
                temp_addr: self.allocate_temp_addr(func),
                dirty_addr: None,
                dirty_value: None,
            })
            .collect();

        let rewrite_blocks = self.promotion_block_ids(func, loop_data);
        self.rewrite_preheader_multi(func, &promoted);
        self.rewrite_loop_body_multi(func, &rewrite_blocks, &promoted);

        for &exit in &loop_data.exit_blocks {
            if self.exit_rolls_back(func, exit) {
                continue;
            }
            for promoted in &promoted {
                self.insert_final_store(
                    func,
                    exit,
                    promoted.candidate.slot_value,
                    promoted.temp_addr,
                );
            }
        }

        self.stats.loops_promoted += 1;
    }

    fn rewrite_preheader_multi(&mut self, func: &mut Function, promoted: &[PromotedCandidate]) {
        let Some(preheader) = promoted.first().map(|candidate| candidate.candidate.preheader)
        else {
            return;
        };

        let mut temps: FxHashMap<StorageAlias, (ValueId, usize)> = FxHashMap::default();
        for candidate in promoted {
            if let Some(init_store) = candidate.candidate.init_store
                && let InstKind::SStore(_, init) = &func.instructions[init_store].kind
            {
                let init_pos = func.blocks[preheader]
                    .instructions
                    .iter()
                    .position(|&inst_id| inst_id == init_store)
                    .expect("candidate init store should be in the preheader");
                temps.insert(candidate.candidate.slot, (candidate.temp_addr, init_pos));
                func.instructions[init_store].kind = InstKind::MStore(candidate.temp_addr, *init);
                func.instructions[init_store].metadata.set_storage_alias(None);
                self.stats.stores_promoted += 1;
            }
        }

        for (pos, inst_id) in func.blocks[preheader].instructions.iter().copied().enumerate() {
            if let InstKind::SLoad(load_slot) = &func.instructions[inst_id].kind {
                let alias = self.storage_alias(func, inst_id, *load_slot);
                if let Some(&(temp_addr, init_pos)) = temps.get(&alias)
                    && pos > init_pos
                {
                    func.instructions[inst_id].kind = InstKind::MLoad(temp_addr);
                    func.instructions[inst_id].metadata.set_storage_alias(None);
                    self.stats.loads_promoted += 1;
                }
            }
        }
    }

    fn rewrite_loop_body_multi(
        &mut self,
        func: &mut Function,
        blocks: &[BlockId],
        promoted: &[PromotedCandidate],
    ) {
        let temps: FxHashMap<StorageAlias, ValueId> =
            promoted.iter().map(|promoted| (promoted.candidate.slot, promoted.temp_addr)).collect();

        for &block_id in blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                let replacement = match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        temps.get(&alias).copied().map(InstKind::MLoad)
                    }
                    InstKind::SStore(slot, value) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        temps.get(&alias).copied().map(|temp| InstKind::MStore(temp, *value))
                    }
                    _ => None,
                };

                if let Some(new_kind) = replacement {
                    match new_kind {
                        InstKind::MLoad(_) => self.stats.loads_promoted += 1,
                        InstKind::MStore(_, _) => self.stats.stores_promoted += 1,
                        _ => {}
                    }
                    func.instructions[inst_id].kind = new_kind;
                    func.instructions[inst_id].metadata.set_storage_alias(None);
                }
            }
        }
    }

    fn promote_loop(&mut self, func: &mut Function, loop_data: &Loop, candidate: &Candidate) {
        let temp_addr = self.allocate_temp_addr(func);
        let dirty_addr = candidate.init_store.is_none().then(|| self.allocate_temp_addr(func));
        let dirty_value = dirty_addr.map(|_| self.bool_word(func, true));
        let promoted =
            PromotedCandidate { candidate: candidate.clone(), temp_addr, dirty_addr, dirty_value };

        self.rewrite_preheader(func, &promoted);

        for block_id in self.promotion_block_ids(func, loop_data) {
            // Rollback exits never flush, so their rewritten stores do not
            // need to update the dirty flag.
            let track_dirty = loop_data.blocks.contains(block_id);
            let mut index = 0;
            while index < func.blocks[block_id].instructions.len() {
                let inst_id = func.blocks[block_id].instructions[index];
                let replacement = match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot)
                        if self.storage_alias(func, inst_id, *slot) == candidate.slot =>
                    {
                        Some(InstKind::MLoad(temp_addr))
                    }
                    InstKind::SStore(slot, value)
                        if self.storage_alias(func, inst_id, *slot) == candidate.slot =>
                    {
                        Some(InstKind::MStore(temp_addr, *value))
                    }
                    _ => None,
                };

                if let Some(new_kind) = replacement {
                    match new_kind {
                        InstKind::MLoad(_) => self.stats.loads_promoted += 1,
                        InstKind::MStore(_, _) => self.stats.stores_promoted += 1,
                        _ => {}
                    }
                    func.instructions[inst_id].kind = new_kind;
                    func.instructions[inst_id].metadata.set_storage_alias(None);
                    if track_dirty
                        && let (Some(dirty_addr), Some(dirty_value)) =
                            (promoted.dirty_addr, promoted.dirty_value)
                        && matches!(func.instructions[inst_id].kind, InstKind::MStore(_, _))
                    {
                        let dirty_store =
                            self.alloc_void_inst(func, InstKind::MStore(dirty_addr, dirty_value));
                        func.blocks[block_id].instructions.insert(index + 1, dirty_store);
                        index += 1;
                    }
                }
                index += 1;
            }
        }

        for &exit in &loop_data.exit_blocks {
            if self.exit_rolls_back(func, exit) {
                continue;
            }
            if let Some(dirty_addr) = promoted.dirty_addr {
                self.insert_conditional_final_store(
                    func,
                    exit,
                    candidate.slot_value,
                    temp_addr,
                    dirty_addr,
                );
            } else {
                self.insert_final_store(func, exit, candidate.slot_value, temp_addr);
            }
        }

        self.stats.loops_promoted += 1;
    }

    fn rewrite_preheader(&mut self, func: &mut Function, promoted: &PromotedCandidate) {
        let candidate = &promoted.candidate;
        match candidate.init_store {
            Some(init_store) => {
                if let InstKind::SStore(_, init) = &func.instructions[init_store].kind {
                    func.instructions[init_store].kind =
                        InstKind::MStore(promoted.temp_addr, *init);
                    func.instructions[init_store].metadata.set_storage_alias(None);
                    self.stats.stores_promoted += 1;
                }

                let init_pos = func.blocks[candidate.preheader]
                    .instructions
                    .iter()
                    .position(|&inst_id| inst_id == init_store)
                    .expect("candidate init store should be in the preheader");
                for &inst_id in &func.blocks[candidate.preheader].instructions[init_pos + 1..] {
                    if let InstKind::SLoad(load_slot) = &func.instructions[inst_id].kind
                        && self.storage_alias(func, inst_id, *load_slot) == candidate.slot
                    {
                        func.instructions[inst_id].kind = InstKind::MLoad(promoted.temp_addr);
                        func.instructions[inst_id].metadata.set_storage_alias(None);
                        self.stats.loads_promoted += 1;
                    }
                }
            }
            None => {
                let insert_pos = func.blocks[candidate.preheader].instructions.len();
                let mut inserted = 0;

                if candidate.needs_initial_load {
                    let (load_inst, load_value) = self.alloc_inst_value(
                        func,
                        InstKind::SLoad(candidate.slot_value),
                        MirType::uint256(),
                    );
                    let store_inst = self
                        .alloc_void_inst(func, InstKind::MStore(promoted.temp_addr, load_value));
                    func.blocks[candidate.preheader]
                        .instructions
                        .insert(insert_pos + inserted, load_inst);
                    inserted += 1;
                    func.blocks[candidate.preheader]
                        .instructions
                        .insert(insert_pos + inserted, store_inst);
                    inserted += 1;
                }

                if let Some(dirty_addr) = promoted.dirty_addr {
                    let false_word = self.bool_word(func, false);
                    let dirty_store =
                        self.alloc_void_inst(func, InstKind::MStore(dirty_addr, false_word));
                    func.blocks[candidate.preheader]
                        .instructions
                        .insert(insert_pos + inserted, dirty_store);
                }
            }
        }
    }

    fn insert_final_store(
        &mut self,
        func: &mut Function,
        exit: BlockId,
        slot_value: ValueId,
        temp_addr: ValueId,
    ) {
        let load_inst =
            func.alloc_inst(Instruction::new(InstKind::MLoad(temp_addr), Some(MirType::uint256())));
        let load_value = func.alloc_value(Value::Inst(load_inst));
        let store_inst =
            func.alloc_inst(Instruction::new(InstKind::SStore(slot_value, load_value), None));

        let insert_pos = func.blocks[exit]
            .instructions
            .iter()
            .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .count();
        func.blocks[exit].instructions.insert(insert_pos, store_inst);
        func.blocks[exit].instructions.insert(insert_pos, load_inst);
    }

    fn insert_conditional_final_store(
        &mut self,
        func: &mut Function,
        exit: BlockId,
        slot_value: ValueId,
        temp_addr: ValueId,
        dirty_addr: ValueId,
    ) {
        let continuation = func.alloc_block();
        let store_block = func.alloc_block();

        let old_instructions = std::mem::take(&mut func.blocks[exit].instructions);
        let old_terminator = func.blocks[exit].terminator.take();
        let old_successors =
            old_terminator.as_ref().map(Terminator::successors).unwrap_or_default();

        // Keep existing exit phis in place; only the non-phi tail moves behind the dirty check.
        let split_pos = old_instructions
            .iter()
            .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .count();
        let mut exit_instructions = old_instructions[..split_pos].to_vec();
        let continuation_instructions = old_instructions[split_pos..].to_vec();

        let dirty_load_inst =
            func.alloc_inst(Instruction::new(InstKind::MLoad(dirty_addr), Some(MirType::Bool)));
        let dirty_value = func.alloc_value(Value::Inst(dirty_load_inst));
        exit_instructions.push(dirty_load_inst);

        func.blocks[exit].instructions = exit_instructions;
        func.blocks[exit].terminator = Some(Terminator::Branch {
            condition: dirty_value,
            then_block: store_block,
            else_block: continuation,
        });

        let load_inst =
            func.alloc_inst(Instruction::new(InstKind::MLoad(temp_addr), Some(MirType::uint256())));
        let load_value = func.alloc_value(Value::Inst(load_inst));
        let store_inst =
            func.alloc_inst(Instruction::new(InstKind::SStore(slot_value, load_value), None));

        func.blocks[store_block].predecessors.push(exit);
        func.blocks[store_block].instructions.push(load_inst);
        func.blocks[store_block].instructions.push(store_inst);
        func.blocks[store_block].terminator = Some(Terminator::Jump(continuation));

        func.blocks[continuation].predecessors.push(exit);
        func.blocks[continuation].predecessors.push(store_block);
        func.blocks[continuation].instructions = continuation_instructions;
        func.blocks[continuation].terminator = old_terminator;

        self.redirect_successor_phi_incoming(func, exit, continuation, &old_successors);
        for successor in old_successors {
            for pred in &mut func.blocks[successor].predecessors {
                if *pred == exit {
                    *pred = continuation;
                }
            }
        }
    }

    fn allocate_temp_addr(&self, func: &mut Function) -> ValueId {
        let frame_offset = func.internal_frame_size.max(func.external_static_return_size);
        let temp_addr = EvmMemoryLayout::HEAP_START + frame_offset;
        func.internal_frame_size = func.internal_frame_size.max(frame_offset + 32);
        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(temp_addr))))
    }

    fn redirect_successor_phi_incoming(
        &self,
        func: &mut Function,
        old_pred: BlockId,
        new_pred: BlockId,
        successors: &[BlockId],
    ) {
        for &successor in successors {
            for idx in 0..func.blocks[successor].instructions.len() {
                let inst_id = func.blocks[successor].instructions[idx];
                if !matches!(func.instructions[inst_id].kind, InstKind::Phi(_)) {
                    break;
                }
                let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind else {
                    continue;
                };
                for (pred, _) in incoming {
                    if *pred == old_pred {
                        *pred = new_pred;
                    }
                }
            }
        }
    }

    fn bool_word(&self, func: &mut Function, value: bool) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    fn alloc_inst_value(
        &self,
        func: &mut Function,
        kind: InstKind,
        ty: MirType,
    ) -> (InstId, ValueId) {
        let inst = func.alloc_inst(Instruction::new(kind, Some(ty)));
        let value = func.alloc_value(Value::Inst(inst));
        (inst, value)
    }

    /// Allocates an instruction that produces no value, so no result [`Value`]
    /// entry is created for it.
    fn alloc_void_inst(&self, func: &mut Function, kind: InstKind) -> InstId {
        func.alloc_inst(Instruction::new(kind, None))
    }

    fn storage_alias_for_loop_value(
        &self,
        func: &Function,
        value: ValueId,
        loop_data: &Loop,
    ) -> Option<StorageAlias> {
        let alias = AliasAnalysis::storage_alias_for_value_at(func, value);
        if let Some(base) = alias.symbolic_base()
            && self.value_defined_in_loop(func, base, loop_data)
        {
            return None;
        }
        Some(alias)
    }

    fn value_defined_in_loop(&self, func: &Function, value: ValueId, loop_data: &Loop) -> bool {
        match func.value(value) {
            Value::Inst(inst_id) => loop_data
                .blocks
                .iter()
                .any(|block_id| func.blocks[block_id].instructions.contains(inst_id)),
            Value::Undef(_) | Value::Error(_) => true,
            Value::Arg { .. } | Value::Immediate(_) => false,
        }
    }

    fn candidate_index(&self, candidates: &[Candidate], alias: &StorageAlias) -> Option<usize> {
        candidates.iter().position(|candidate| candidate.slot == *alias)
    }

    fn storage_alias(&self, func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
        AliasAnalysis::storage_alias_at(func, inst_id, slot)
    }

    fn storage_may_alias(&self, first: StorageAlias, second: StorageAlias) -> bool {
        // Storage-only aliasing does not depend on pointer provenance, but the
        // shared analysis still owns the comparison API.
        first == second || first.may_alias(second)
    }
}
