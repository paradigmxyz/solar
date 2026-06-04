//! Storage scalar promotion for simple loop-carried storage updates.
//!
//! This pass recognizes loops that repeatedly update a single storage slot and
//! rewrites the loop to update a memory-backed scalar instead. The final value
//! is stored back to storage once on each clean loop exit.

use crate::{
    analysis::{Loop, LoopAnalyzer},
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Terminator, Value,
        ValueId,
    },
};
use alloy_primitives::U256;

const LOW_MEMORY_START: u64 = 0x80;

/// Statistics from storage scalar promotion.
#[derive(Clone, Debug, Default)]
pub struct StoragePromotionStats {
    /// Number of loops promoted.
    pub loops_promoted: usize,
    /// Number of storage loads rewritten to memory loads.
    pub loads_promoted: usize,
    /// Number of storage stores rewritten to memory stores.
    pub stores_promoted: usize,
}

/// Promotes loop-carried storage values to memory-backed scalars.
#[derive(Debug, Default)]
pub struct StorageScalarPromoter {
    stats: StoragePromotionStats,
}

#[derive(Clone, Debug)]
struct Candidate {
    slot_value: ValueId,
    slot: U256,
    preheader: BlockId,
    init_store: InstId,
}

impl StorageScalarPromoter {
    /// Creates a new storage scalar promotion pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns statistics for the most recent run.
    #[must_use]
    pub const fn stats(&self) -> &StoragePromotionStats {
        &self.stats
    }

    /// Runs the pass on one function.
    pub fn run(&mut self, func: &mut Function) -> &StoragePromotionStats {
        self.stats = StoragePromotionStats::default();

        // The pass currently introduces absolute low-memory temporaries, so it
        // only handles externally callable runtime entries.
        if func.selector.is_none() {
            return &self.stats;
        }

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze(func);
        let mut loops: Vec<Loop> = loop_info.all_loops().cloned().collect();
        loops.sort_by_key(|loop_data| loop_data.header.index());

        for loop_data in loops {
            if let Some(candidate) = self.find_candidate(func, &loop_data) {
                self.promote_loop(func, &loop_data, &candidate);
            }
        }

        &self.stats
    }

    fn find_candidate(&self, func: &Function, loop_data: &Loop) -> Option<Candidate> {
        let preheader = loop_data.preheader?;
        if loop_data.exit_blocks.is_empty() || !self.has_isolated_clean_exits(func, loop_data) {
            return None;
        }
        if !self.loop_has_only_promotable_storage_effects(func, loop_data) {
            return None;
        }

        let mut slot: Option<U256> = None;
        let mut slot_value: Option<ValueId> = None;
        let mut saw_loop_store = false;

        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if let InstKind::SStore(store_slot, _) = &func.instructions[inst_id].kind {
                    let store_key = self.immediate_slot(func, *store_slot)?;
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
        let init_store = self.find_preheader_init_store(func, preheader, slot)?;
        if !self.preheader_tail_is_safe(func, preheader, init_store, slot) {
            return None;
        }

        Some(Candidate { slot_value, slot, preheader, init_store })
    }

    fn has_isolated_clean_exits(&self, func: &Function, loop_data: &Loop) -> bool {
        loop_data.exit_blocks.iter().all(|&exit| {
            func.blocks[exit].predecessors.iter().all(|pred| loop_data.blocks.contains(pred))
                && !matches!(
                    func.blocks[exit].terminator,
                    Some(Terminator::Revert { .. })
                        | Some(Terminator::SelfDestruct { .. })
                        | Some(Terminator::Invalid)
                )
        })
    }

    fn loop_has_only_promotable_storage_effects(&self, func: &Function, loop_data: &Loop) -> bool {
        for &block_id in &loop_data.blocks {
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
                    InstKind::SStore(slot, _) if self.immediate_slot(func, *slot).is_none() => {
                        return false;
                    }
                    InstKind::SStore(_, _) => {}
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

    fn find_preheader_init_store(
        &self,
        func: &Function,
        preheader: BlockId,
        slot: U256,
    ) -> Option<InstId> {
        func.blocks[preheader].instructions.iter().rev().copied().find(|&inst_id| {
            matches!(
                &func.instructions[inst_id].kind,
                InstKind::SStore(store_slot, _) if self.immediate_slot(func, *store_slot) == Some(slot)
            )
        })
    }

    fn preheader_tail_is_safe(
        &self,
        func: &Function,
        preheader: BlockId,
        init_store: InstId,
        slot: U256,
    ) -> bool {
        let Some(init_pos) =
            func.blocks[preheader].instructions.iter().position(|&inst_id| inst_id == init_store)
        else {
            return false;
        };

        for &inst_id in &func.blocks[preheader].instructions[init_pos + 1..] {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(load_slot)
                    if self.immediate_slot(func, *load_slot) != Some(slot) =>
                {
                    continue;
                }
                InstKind::SLoad(_) => {}
                InstKind::MStore(_, _) | InstKind::MStore8(_, _) | InstKind::MCopy(_, _, _) => {}
                kind if kind.has_side_effects() => return false,
                InstKind::Gas => return false,
                _ => {}
            }
        }

        true
    }

    fn promote_loop(&mut self, func: &mut Function, loop_data: &Loop, candidate: &Candidate) {
        let temp_addr = self.allocate_temp_addr(func);

        self.rewrite_preheader(
            func,
            candidate.preheader,
            candidate.init_store,
            temp_addr,
            candidate.slot,
        );

        for &block_id in &loop_data.blocks {
            let inst_ids = func.blocks[block_id].instructions.clone();
            for inst_id in inst_ids {
                let replacement = match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot)
                        if self.immediate_slot(func, *slot) == Some(candidate.slot) =>
                    {
                        Some(InstKind::MLoad(temp_addr))
                    }
                    InstKind::SStore(slot, value)
                        if self.immediate_slot(func, *slot) == Some(candidate.slot) =>
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
                }
            }
        }

        for &exit in &loop_data.exit_blocks {
            self.insert_final_store(func, exit, candidate.slot_value, temp_addr);
        }

        self.stats.loops_promoted += 1;
    }

    fn rewrite_preheader(
        &mut self,
        func: &mut Function,
        preheader: BlockId,
        init_store: InstId,
        temp_addr: ValueId,
        slot: U256,
    ) {
        if let InstKind::SStore(_, init) = &func.instructions[init_store].kind {
            func.instructions[init_store].kind = InstKind::MStore(temp_addr, *init);
            self.stats.stores_promoted += 1;
        }

        let inst_ids = func.blocks[preheader].instructions.clone();
        let mut rewrite = false;
        for inst_id in inst_ids {
            if inst_id == init_store {
                rewrite = true;
                continue;
            }
            if !rewrite {
                continue;
            }
            if let InstKind::SLoad(load_slot) = &func.instructions[inst_id].kind
                && self.immediate_slot(func, *load_slot) == Some(slot)
            {
                func.instructions[inst_id].kind = InstKind::MLoad(temp_addr);
                self.stats.loads_promoted += 1;
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
        let _store_value = func.alloc_value(Value::Inst(store_inst));

        let insert_pos = func.blocks[exit]
            .instructions
            .iter()
            .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .count();
        func.blocks[exit].instructions.insert(insert_pos, store_inst);
        func.blocks[exit].instructions.insert(insert_pos, load_inst);
    }

    fn allocate_temp_addr(&self, func: &mut Function) -> ValueId {
        let frame_offset = func.internal_frame_size.max(func.external_static_return_size);
        let temp_addr = LOW_MEMORY_START + frame_offset;
        func.internal_frame_size = func.internal_frame_size.max(frame_offset + 32);
        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(temp_addr))))
    }

    fn immediate_slot(&self, func: &Function, value: ValueId) -> Option<U256> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Immediate, Terminator};
    use solar_interface::Ident;

    struct TestLoop {
        func: Function,
        entry_store: InstId,
        body_load: InstId,
        body_store: InstId,
        exit: BlockId,
    }

    fn imm(func: &mut Function, value: u64) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(value))))
    }

    fn inst(func: &mut Function, block: BlockId, kind: InstKind, ty: Option<MirType>) -> InstId {
        let inst = func.alloc_inst(Instruction::new(kind, ty));
        func.blocks[block].instructions.push(inst);
        func.alloc_value(Value::Inst(inst));
        inst
    }

    fn make_storage_loop(external: bool) -> TestLoop {
        let mut func = Function::new(Ident::DUMMY);
        if external {
            func.selector = Some([0, 0, 0, 1]);
        }

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let update = func.alloc_block();
        let exit = func.alloc_block();

        let slot = imm(&mut func, 0);
        let one = imm(&mut func, 1);
        let two = imm(&mut func, 2);
        let cond = imm(&mut func, 1);

        let entry_store = inst(&mut func, entry, InstKind::SStore(slot, one), None);
        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[entry].successors.push(header);
        func.blocks[header].predecessors.push(entry);

        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[header].successors.push(body);
        func.blocks[header].successors.push(exit);
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        let body_load = inst(&mut func, body, InstKind::SLoad(slot), Some(MirType::uint256()));
        let loaded = match func.values.iter_enumerated().find_map(|(value_id, value)| match value {
            Value::Inst(inst_id) if *inst_id == body_load => Some(value_id),
            _ => None,
        }) {
            Some(value) => value,
            None => panic!("missing load result"),
        };
        let mul = inst(&mut func, body, InstKind::Mul(loaded, two), Some(MirType::uint256()));
        let product =
            match func.values.iter_enumerated().find_map(|(value_id, value)| match value {
                Value::Inst(inst_id) if *inst_id == mul => Some(value_id),
                _ => None,
            }) {
                Some(value) => value,
                None => panic!("missing product result"),
            };
        let body_store = inst(&mut func, body, InstKind::SStore(slot, product), None);
        func.blocks[body].terminator = Some(Terminator::Jump(update));
        func.blocks[body].successors.push(update);
        func.blocks[update].predecessors.push(body);

        func.blocks[update].terminator = Some(Terminator::Jump(header));
        func.blocks[update].successors.push(header);
        func.blocks[header].predecessors.push(update);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        TestLoop { func, entry_store, body_load, body_store, exit }
    }

    #[test]
    fn promotes_external_storage_update_loop() {
        let mut test = make_storage_loop(true);
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 1);
        assert_eq!(stats.loads_promoted, 1);
        assert_eq!(stats.stores_promoted, 2);
        assert!(matches!(test.func.instructions[test.entry_store].kind, InstKind::MStore(_, _)));
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::MLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::MStore(_, _)));
        assert!(matches!(
            test.func.instructions[test.func.blocks[test.exit].instructions[0]].kind,
            InstKind::MLoad(_)
        ));
        assert!(matches!(
            test.func.instructions[test.func.blocks[test.exit].instructions[1]].kind,
            InstKind::SStore(_, _)
        ));
    }

    #[test]
    fn skips_non_external_functions() {
        let mut test = make_storage_loop(false);
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 0);
        assert!(matches!(test.func.instructions[test.entry_store].kind, InstKind::SStore(_, _)));
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::SLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::SStore(_, _)));
    }
}
