//! Storage scalar promotion for simple loop-carried storage updates.
//!
//! This pass recognizes loops that repeatedly update a single storage slot and
//! rewrites the loop to update a memory-backed scalar instead. The final value
//! is stored back to storage once on each clean loop exit.

use crate::{
    analysis::{Loop, LoopAnalyzer},
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, StorageAlias,
        Terminator, Value, ValueId,
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
    slot: StorageAlias,
    preheader: BlockId,
    init_store: Option<InstId>,
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

        self.annotate_storage_aliases(func);

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
        if !self.loop_has_no_unpromotable_side_effects(func, loop_data) {
            return None;
        }

        let mut slot: Option<StorageAlias> = None;
        let mut slot_value: Option<ValueId> = None;
        let mut saw_loop_store = false;

        for &block_id in &loop_data.blocks {
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
        if !self.loop_storage_accesses_are_safe(func, loop_data, &slot) {
            return None;
        }
        let saw_loop_load = loop_data.blocks.iter().any(|&block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                matches!(
                    &func.instructions[inst_id].kind,
                    InstKind::SLoad(load_slot) if self.storage_alias(func, inst_id, *load_slot) == slot
                )
            })
        });
        let init_store = self.find_preheader_init_store(func, preheader, &slot);
        if let Some(init_store) = init_store
            && !self.preheader_tail_is_safe(func, preheader, init_store, &slot)
        {
            return None;
        }
        if init_store.is_none() && !saw_loop_load {
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

    fn loop_has_no_unpromotable_side_effects(&self, func: &Function, loop_data: &Loop) -> bool {
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
        loop_data: &Loop,
        candidate: &StorageAlias,
    ) -> bool {
        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::SLoad(slot) => {
                        let alias = self.storage_alias(func, inst_id, *slot);
                        if alias != *candidate && self.storage_aliases_may_alias(candidate, &alias)
                        {
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

    fn find_preheader_init_store(
        &self,
        func: &Function,
        preheader: BlockId,
        slot: &StorageAlias,
    ) -> Option<InstId> {
        func.blocks[preheader].instructions.iter().rev().copied().find(|&inst_id| {
            matches!(
                &func.instructions[inst_id].kind,
                InstKind::SStore(store_slot, _) if self.storage_alias(func, inst_id, *store_slot) == *slot
            )
        })
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
                    if alias != *slot && self.storage_aliases_may_alias(slot, &alias) {
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

    fn promote_loop(&mut self, func: &mut Function, loop_data: &Loop, candidate: &Candidate) {
        let temp_addr = self.allocate_temp_addr(func);
        let dirty_addr = candidate.init_store.is_none().then(|| self.allocate_temp_addr(func));
        let dirty_value = dirty_addr.map(|_| self.bool_word(func, true));

        self.rewrite_preheader(func, candidate, temp_addr, dirty_addr);

        for &block_id in &loop_data.blocks {
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
                    func.instructions[inst_id].metadata.storage_alias = None;
                    if let (Some(dirty_addr), Some(dirty_value)) = (dirty_addr, dirty_value)
                        && matches!(func.instructions[inst_id].kind, InstKind::MStore(_, _))
                    {
                        let (dirty_store, _) = self.alloc_inst_value(
                            func,
                            InstKind::MStore(dirty_addr, dirty_value),
                            None,
                        );
                        func.blocks[block_id].instructions.insert(index + 1, dirty_store);
                        index += 1;
                    }
                }
                index += 1;
            }
        }

        for &exit in &loop_data.exit_blocks {
            if let Some(dirty_addr) = dirty_addr {
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

    fn rewrite_preheader(
        &mut self,
        func: &mut Function,
        candidate: &Candidate,
        temp_addr: ValueId,
        dirty_addr: Option<ValueId>,
    ) {
        match candidate.init_store {
            Some(init_store) => {
                if let InstKind::SStore(_, init) = &func.instructions[init_store].kind {
                    func.instructions[init_store].kind = InstKind::MStore(temp_addr, *init);
                    self.stats.stores_promoted += 1;
                }

                let inst_ids = func.blocks[candidate.preheader].instructions.clone();
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
                        && self.storage_alias(func, inst_id, *load_slot) == candidate.slot
                    {
                        func.instructions[inst_id].kind = InstKind::MLoad(temp_addr);
                        func.instructions[inst_id].metadata.storage_alias = None;
                        self.stats.loads_promoted += 1;
                    }
                }
            }
            None => {
                let (load_inst, load_value) = self.alloc_inst_value(
                    func,
                    InstKind::SLoad(candidate.slot_value),
                    Some(MirType::uint256()),
                );
                let (store_inst, _) =
                    self.alloc_inst_value(func, InstKind::MStore(temp_addr, load_value), None);
                let insert_pos = func.blocks[candidate.preheader].instructions.len();
                func.blocks[candidate.preheader].instructions.insert(insert_pos, store_inst);
                func.blocks[candidate.preheader].instructions.insert(insert_pos, load_inst);

                if let Some(dirty_addr) = dirty_addr {
                    let false_word = self.bool_word(func, false);
                    let (dirty_store, _) =
                        self.alloc_inst_value(func, InstKind::MStore(dirty_addr, false_word), None);
                    func.blocks[candidate.preheader]
                        .instructions
                        .insert(insert_pos + 2, dirty_store);
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
        let _store_value = func.alloc_value(Value::Inst(store_inst));

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
        let old_successors = std::mem::take(&mut func.blocks[exit].successors);

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
        func.blocks[exit].successors.push(store_block);
        func.blocks[exit].successors.push(continuation);

        let load_inst =
            func.alloc_inst(Instruction::new(InstKind::MLoad(temp_addr), Some(MirType::uint256())));
        let load_value = func.alloc_value(Value::Inst(load_inst));
        let store_inst =
            func.alloc_inst(Instruction::new(InstKind::SStore(slot_value, load_value), None));
        let _store_value = func.alloc_value(Value::Inst(store_inst));

        func.blocks[store_block].predecessors.push(exit);
        func.blocks[store_block].instructions.push(load_inst);
        func.blocks[store_block].instructions.push(store_inst);
        func.blocks[store_block].terminator = Some(Terminator::Jump(continuation));
        func.blocks[store_block].successors.push(continuation);

        func.blocks[continuation].predecessors.push(exit);
        func.blocks[continuation].predecessors.push(store_block);
        func.blocks[continuation].instructions = continuation_instructions;
        func.blocks[continuation].terminator = old_terminator;
        func.blocks[continuation].successors = old_successors.clone();

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
        let temp_addr = LOW_MEMORY_START + frame_offset;
        func.internal_frame_size = func.internal_frame_size.max(frame_offset + 32);
        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(temp_addr))))
    }

    fn bool_word(&self, func: &mut Function, value: bool) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    fn alloc_inst_value(
        &self,
        func: &mut Function,
        kind: InstKind,
        ty: Option<MirType>,
    ) -> (InstId, ValueId) {
        let inst = func.alloc_inst(Instruction::new(kind, ty));
        let value = func.alloc_value(Value::Inst(inst));
        (inst, value)
    }

    fn annotate_storage_aliases(&self, func: &mut Function) {
        let inst_ids: Vec<_> =
            func.instructions.iter_enumerated().map(|(inst_id, _)| inst_id).collect();
        for inst_id in inst_ids {
            let slot = match &func.instructions[inst_id].kind {
                InstKind::SLoad(slot) | InstKind::SStore(slot, _) => Some(*slot),
                _ => None,
            };
            func.instructions[inst_id].metadata.storage_alias =
                slot.map(|slot| self.storage_alias_for_value(func, slot));
        }
    }

    fn storage_alias(&self, func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
        func.instructions[inst_id]
            .metadata
            .storage_alias
            .unwrap_or_else(|| self.storage_alias_for_value(func, slot))
    }

    fn storage_alias_for_loop_value(
        &self,
        func: &Function,
        value: ValueId,
        loop_data: &Loop,
    ) -> Option<StorageAlias> {
        let alias = self.storage_alias_for_value(func, value);
        if let StorageAlias::Symbolic(value) = alias
            && self.value_defined_in_loop(func, value, loop_data)
        {
            return None;
        }
        Some(alias)
    }

    fn storage_alias_for_value(&self, func: &Function, value: ValueId) -> StorageAlias {
        match func.value(value) {
            Value::Immediate(imm) => {
                imm.as_u256().map_or(StorageAlias::Symbolic(value), StorageAlias::Slot)
            }
            _ => StorageAlias::Symbolic(value),
        }
    }

    fn value_defined_in_loop(&self, func: &Function, value: ValueId, loop_data: &Loop) -> bool {
        match func.value(value) {
            Value::Inst(inst_id) => loop_data
                .blocks
                .iter()
                .any(|&block_id| func.blocks[block_id].instructions.contains(inst_id)),
            Value::Phi { .. } | Value::Undef(_) => true,
            Value::Arg { .. } | Value::Immediate(_) => false,
        }
    }

    fn storage_aliases_may_alias(&self, a: &StorageAlias, b: &StorageAlias) -> bool {
        match (a, b) {
            (StorageAlias::Slot(a), StorageAlias::Slot(b)) => a == b,
            _ => true,
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

    struct NoInitLoop {
        func: Function,
        body_load: InstId,
        body_store: InstId,
        exit: BlockId,
    }

    fn imm(func: &mut Function, value: u64) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(value))))
    }

    fn inst_value(
        func: &mut Function,
        block: BlockId,
        kind: InstKind,
        ty: Option<MirType>,
    ) -> (InstId, ValueId) {
        let inst = func.alloc_inst(Instruction::new(kind, ty));
        func.blocks[block].instructions.push(inst);
        let value = func.alloc_value(Value::Inst(inst));
        (inst, value)
    }

    fn inst(func: &mut Function, block: BlockId, kind: InstKind, ty: Option<MirType>) -> InstId {
        inst_value(func, block, kind, ty).0
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

    fn make_storage_loop_without_init() -> NoInitLoop {
        let mut func = Function::new(Ident::DUMMY);
        func.selector = Some([0, 0, 0, 1]);

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let update = func.alloc_block();
        let exit = func.alloc_block();

        let slot = imm(&mut func, 0);
        let two = imm(&mut func, 2);
        let cond = imm(&mut func, 1);

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
        let add = inst(&mut func, body, InstKind::Add(loaded, two), Some(MirType::uint256()));
        let sum = match func.values.iter_enumerated().find_map(|(value_id, value)| match value {
            Value::Inst(inst_id) if *inst_id == add => Some(value_id),
            _ => None,
        }) {
            Some(value) => value,
            None => panic!("missing sum result"),
        };
        let body_store = inst(&mut func, body, InstKind::SStore(slot, sum), None);
        func.blocks[body].terminator = Some(Terminator::Jump(update));
        func.blocks[body].successors.push(update);
        func.blocks[update].predecessors.push(body);

        func.blocks[update].terminator = Some(Terminator::Jump(header));
        func.blocks[update].successors.push(header);
        func.blocks[header].predecessors.push(update);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        NoInitLoop { func, body_load, body_store, exit }
    }

    fn make_store_only_loop_without_init() -> NoInitLoop {
        let mut func = Function::new(Ident::DUMMY);
        func.selector = Some([0, 0, 0, 1]);

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let update = func.alloc_block();
        let exit = func.alloc_block();

        let slot = imm(&mut func, 0);
        let value = imm(&mut func, 2);
        let cond = imm(&mut func, 1);

        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[entry].successors.push(header);
        func.blocks[header].predecessors.push(entry);

        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[header].successors.push(body);
        func.blocks[header].successors.push(exit);
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        let body_store = inst(&mut func, body, InstKind::SStore(slot, value), None);
        func.blocks[body].terminator = Some(Terminator::Jump(update));
        func.blocks[body].successors.push(update);
        func.blocks[update].predecessors.push(body);

        func.blocks[update].terminator = Some(Terminator::Jump(header));
        func.blocks[update].successors.push(header);
        func.blocks[header].predecessors.push(update);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        NoInitLoop { func, body_load: body_store, body_store, exit }
    }

    fn make_symbolic_storage_loop() -> TestLoop {
        let mut func = Function::new(Ident::DUMMY);
        func.selector = Some([0, 0, 0, 1]);
        func.params.push(MirType::uint256());

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let update = func.alloc_block();
        let exit = func.alloc_block();

        let slot = func.alloc_value(Value::Arg { index: 0, ty: MirType::uint256() });
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

        let (body_load, loaded) =
            inst_value(&mut func, body, InstKind::SLoad(slot), Some(MirType::uint256()));
        let (_, product) =
            inst_value(&mut func, body, InstKind::Mul(loaded, two), Some(MirType::uint256()));
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

    fn make_loop_variant_symbolic_storage_loop() -> NoInitLoop {
        let mut func = Function::new(Ident::DUMMY);
        func.selector = Some([0, 0, 0, 1]);
        func.params.push(MirType::uint256());

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let update = func.alloc_block();
        let exit = func.alloc_block();

        let seed = func.alloc_value(Value::Arg { index: 0, ty: MirType::uint256() });
        let zero = imm(&mut func, 0);
        let two = imm(&mut func, 2);
        let cond = imm(&mut func, 1);

        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[entry].successors.push(header);
        func.blocks[header].predecessors.push(entry);

        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[header].successors.push(body);
        func.blocks[header].successors.push(exit);
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        let (_, slot) =
            inst_value(&mut func, body, InstKind::Add(seed, zero), Some(MirType::uint256()));
        let (body_load, loaded) =
            inst_value(&mut func, body, InstKind::SLoad(slot), Some(MirType::uint256()));
        let (_, sum) =
            inst_value(&mut func, body, InstKind::Add(loaded, two), Some(MirType::uint256()));
        let body_store = inst(&mut func, body, InstKind::SStore(slot, sum), None);
        func.blocks[body].terminator = Some(Terminator::Jump(update));
        func.blocks[body].successors.push(update);
        func.blocks[update].predecessors.push(body);

        func.blocks[update].terminator = Some(Terminator::Jump(header));
        func.blocks[update].successors.push(header);
        func.blocks[header].predecessors.push(update);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        NoInitLoop { func, body_load, body_store, exit }
    }

    fn make_symbolic_loop_with_possibly_aliasing_load() -> TestLoop {
        let mut test = make_symbolic_storage_loop();
        let const_slot = imm(&mut test.func, 0);
        let body = test
            .func
            .blocks
            .iter_enumerated()
            .find_map(|(block_id, block)| {
                block.instructions.contains(&test.body_load).then_some(block_id)
            })
            .expect("missing body block");
        inst(&mut test.func, body, InstKind::SLoad(const_slot), Some(MirType::uint256()));
        test
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

    #[test]
    fn promotes_storage_update_loop_without_preheader_store() {
        let mut test = make_storage_loop_without_init();
        let entry = test.func.entry_block;
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 1);
        assert_eq!(stats.loads_promoted, 1);
        assert_eq!(stats.stores_promoted, 1);
        assert!(matches!(
            test.func.instructions[test.func.blocks[entry].instructions[0]].kind,
            InstKind::SLoad(_)
        ));
        assert!(matches!(
            test.func.instructions[test.func.blocks[entry].instructions[1]].kind,
            InstKind::MStore(_, _)
        ));
        assert!(matches!(
            test.func.instructions[test.func.blocks[entry].instructions[2]].kind,
            InstKind::MStore(_, _)
        ));
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::MLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::MStore(_, _)));

        let dirty_store_pos = test.func.blocks.iter().find_map(|block| {
            block
                .instructions
                .iter()
                .position(|&inst_id| inst_id == test.body_store)
                .map(|pos| (block, pos + 1))
        });
        let Some((body_block, dirty_store_pos)) = dirty_store_pos else {
            panic!("missing promoted body store");
        };
        assert!(matches!(
            test.func.instructions[body_block.instructions[dirty_store_pos]].kind,
            InstKind::MStore(_, _)
        ));

        assert!(matches!(
            test.func.instructions[test.func.blocks[test.exit].instructions[0]].kind,
            InstKind::MLoad(_)
        ));
        let Some(Terminator::Branch { then_block, else_block, .. }) =
            test.func.blocks[test.exit].terminator.as_ref()
        else {
            panic!("dirty exit should branch");
        };
        assert!(matches!(
            test.func.instructions[test.func.blocks[*then_block].instructions[0]].kind,
            InstKind::MLoad(_)
        ));
        assert!(matches!(
            test.func.instructions[test.func.blocks[*then_block].instructions[1]].kind,
            InstKind::SStore(_, _)
        ));
        assert_eq!(test.func.blocks[*else_block].terminator, Some(Terminator::Stop));
    }

    #[test]
    fn skips_store_only_loop_without_preheader_store() {
        let mut test = make_store_only_loop_without_init();
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 0);
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::SStore(_, _)));
    }

    #[test]
    fn promotes_invariant_symbolic_storage_slot() {
        let mut test = make_symbolic_storage_loop();
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 1);
        assert_eq!(stats.loads_promoted, 1);
        assert_eq!(stats.stores_promoted, 2);
        assert!(matches!(test.func.instructions[test.entry_store].kind, InstKind::MStore(_, _)));
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::MLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::MStore(_, _)));
    }

    #[test]
    fn skips_loop_variant_symbolic_storage_slot() {
        let mut test = make_loop_variant_symbolic_storage_loop();
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 0);
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::SLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::SStore(_, _)));
    }

    #[test]
    fn skips_symbolic_slot_with_possibly_aliasing_storage_load() {
        let mut test = make_symbolic_loop_with_possibly_aliasing_load();
        let mut pass = StorageScalarPromoter::new();
        let stats = pass.run(&mut test.func);

        assert_eq!(stats.loops_promoted, 0);
        assert!(matches!(test.func.instructions[test.entry_store].kind, InstKind::SStore(_, _)));
        assert!(matches!(test.func.instructions[test.body_load].kind, InstKind::SLoad(_)));
        assert!(matches!(test.func.instructions[test.body_store].kind, InstKind::SStore(_, _)));
    }
}
