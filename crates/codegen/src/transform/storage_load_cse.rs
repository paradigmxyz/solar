//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot.

use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, Terminator, Value, ValueId},
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Local storage load CSE pass.
#[derive(Debug, Default)]
pub struct StorageLoadCse {
    /// Number of storage loads eliminated.
    pub eliminated_count: usize,
}

impl StorageLoadCse {
    /// Creates a new storage-load CSE pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs storage-load CSE on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;
        self.annotate_storage_aliases(func);

        let liveness = Liveness::compute(func);
        let inst_results = Self::inst_results(func);
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();

        for block_id in block_ids {
            self.process_block(
                func,
                block_id,
                &liveness,
                &inst_results,
                &mut replacements,
                &mut dead,
            );
        }

        if !replacements.is_empty() {
            Self::replace_uses(func, &replacements);
        }
        if !dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|id| !dead.contains(id));
            }
        }

        self.eliminated_count
    }

    /// Runs storage-load CSE to a fixed point.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        loop {
            let eliminated = self.run(func);
            if eliminated == 0 {
                break;
            }
            total += eliminated;
        }
        total
    }

    fn process_block(
        &mut self,
        func: &Function,
        block_id: BlockId,
        liveness: &Liveness,
        inst_results: &FxHashMap<InstId, ValueId>,
        replacements: &mut FxHashMap<ValueId, ValueId>,
        dead: &mut FxHashSet<InstId>,
    ) {
        let mut cached_loads: FxHashMap<StorageAlias, ValueId> = FxHashMap::default();
        let inst_ids = func.blocks[block_id].instructions.clone();

        for (inst_idx, inst_id) in inst_ids.into_iter().enumerate() {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(slot) => {
                    let alias = self.storage_alias(func, inst_id, *slot, replacements);
                    let Some(&result) = inst_results.get(&inst_id) else {
                        continue;
                    };
                    if let Some(&cached) = cached_loads.get(&alias) {
                        if !liveness
                            .live_at_inst(func, block_id, inst_idx)
                            .live_before
                            .contains(cached)
                        {
                            cached_loads.insert(alias, result);
                            continue;
                        }
                        replacements.insert(result, cached);
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                    } else {
                        cached_loads.insert(alias, result);
                    }
                }
                InstKind::SStore(slot, _) => {
                    let alias = self.storage_alias(func, inst_id, *slot, replacements);
                    cached_loads.retain(|cached_alias, _| {
                        !Self::storage_aliases_may_alias(cached_alias, &alias)
                    });
                }
                kind if Self::may_mutate_storage(kind) => cached_loads.clear(),
                _ => {}
            }
        }
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
                slot.map(|slot| Self::storage_alias_for_value(func, slot));
        }
    }

    fn storage_alias(
        &self,
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        let original_slot = slot;
        let slot = Self::canonical_value(slot, replacements);
        if slot == original_slot {
            func.instructions[inst_id]
                .metadata
                .storage_alias
                .unwrap_or_else(|| Self::storage_alias_for_value(func, slot))
        } else {
            Self::storage_alias_for_value(func, slot)
        }
    }

    fn storage_alias_for_value(func: &Function, value: ValueId) -> StorageAlias {
        match func.value(value) {
            Value::Immediate(imm) => {
                imm.as_u256().map_or(StorageAlias::Symbolic(value), StorageAlias::Slot)
            }
            _ => StorageAlias::Symbolic(value),
        }
    }

    fn storage_aliases_may_alias(a: &StorageAlias, b: &StorageAlias) -> bool {
        match (a, b) {
            (StorageAlias::Slot(a), StorageAlias::Slot(b)) => a == b,
            _ => true,
        }
    }

    fn may_mutate_storage(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Call { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
        )
    }

    fn canonical_value(value: ValueId, replacements: &FxHashMap<ValueId, ValueId>) -> ValueId {
        let mut value = value;
        while let Some(&replacement) = replacements.get(&value) {
            if replacement == value {
                break;
            }
            value = replacement;
        }
        value
    }

    fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
        let mut results = FxHashMap::default();
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(inst_id) = value {
                results.insert(*inst_id, value_id);
            }
        }
        results
    }

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        for inst in func.instructions.iter_mut() {
            Self::replace_inst_operands(&mut inst.kind, replacements);
            if matches!(inst.kind, InstKind::SLoad(_) | InstKind::SStore(_, _)) {
                inst.metadata.storage_alias = None;
            }
        }

        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                Self::replace_terminator_operands(term, replacements);
            }
        }
    }

    fn replace_inst_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
        let replace = |value: &mut ValueId| {
            *value = Self::canonical_value(*value, replacements);
        };

        match kind {
            InstKind::Add(a, b)
            | InstKind::Sub(a, b)
            | InstKind::Mul(a, b)
            | InstKind::Div(a, b)
            | InstKind::SDiv(a, b)
            | InstKind::Mod(a, b)
            | InstKind::SMod(a, b)
            | InstKind::Exp(a, b)
            | InstKind::And(a, b)
            | InstKind::Or(a, b)
            | InstKind::Xor(a, b)
            | InstKind::Shl(a, b)
            | InstKind::Shr(a, b)
            | InstKind::Sar(a, b)
            | InstKind::Byte(a, b)
            | InstKind::Lt(a, b)
            | InstKind::Gt(a, b)
            | InstKind::SLt(a, b)
            | InstKind::SGt(a, b)
            | InstKind::Eq(a, b)
            | InstKind::MStore(a, b)
            | InstKind::MStore8(a, b)
            | InstKind::SStore(a, b)
            | InstKind::TStore(a, b)
            | InstKind::Keccak256(a, b)
            | InstKind::Log0(a, b)
            | InstKind::SignExtend(a, b) => {
                replace(a);
                replace(b);
            }

            InstKind::Not(a)
            | InstKind::IsZero(a)
            | InstKind::MLoad(a)
            | InstKind::SLoad(a)
            | InstKind::TLoad(a)
            | InstKind::CalldataLoad(a)
            | InstKind::ExtCodeSize(a)
            | InstKind::ExtCodeHash(a)
            | InstKind::Balance(a)
            | InstKind::BlockHash(a)
            | InstKind::BlobHash(a) => replace(a),

            InstKind::AddMod(a, b, c)
            | InstKind::MulMod(a, b, c)
            | InstKind::MCopy(a, b, c)
            | InstKind::CalldataCopy(a, b, c)
            | InstKind::CodeCopy(a, b, c)
            | InstKind::ReturnDataCopy(a, b, c)
            | InstKind::Create(a, b, c)
            | InstKind::Log1(a, b, c)
            | InstKind::Select(a, b, c) => {
                replace(a);
                replace(b);
                replace(c);
            }

            InstKind::ExtCodeCopy(a, b, c, d)
            | InstKind::Create2(a, b, c, d)
            | InstKind::Log2(a, b, c, d) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
            }

            InstKind::Log3(a, b, c, d, e) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
            }

            InstKind::Log4(a, b, c, d, e, f) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
                replace(f);
            }

            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(value);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }

            InstKind::InternalCall { args, .. } => {
                for arg in args {
                    replace(arg);
                }
            }

            InstKind::Phi(incoming) => {
                for (_, value) in incoming {
                    replace(value);
                }
            }

            InstKind::MSize
            | InstKind::CalldataSize
            | InstKind::InternalFrameAddr(_)
            | InstKind::CodeSize
            | InstKind::ReturnDataSize
            | InstKind::Caller
            | InstKind::CallValue
            | InstKind::Origin
            | InstKind::GasPrice
            | InstKind::Coinbase
            | InstKind::Timestamp
            | InstKind::BlockNumber
            | InstKind::PrevRandao
            | InstKind::GasLimit
            | InstKind::ChainId
            | InstKind::Address
            | InstKind::SelfBalance
            | InstKind::Gas
            | InstKind::BaseFee
            | InstKind::BlobBaseFee => {}
        }
    }

    fn replace_terminator_operands(
        term: &mut Terminator,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let replace = |value: &mut ValueId| {
            *value = Self::canonical_value(*value, replacements);
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => replace(condition),
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case, _) in cases {
                    replace(case);
                }
            }
            Terminator::Return { values } => {
                for value in values {
                    replace(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => replace(recipient),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Immediate, Instruction, MirType};
    use alloy_primitives::U256;
    use solar_interface::Ident;

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

    #[test]
    fn reuses_load_across_disjoint_constant_store() {
        let mut func = Function::new(Ident::DUMMY);
        let entry = func.entry_block;
        let slot0 = imm(&mut func, 0);
        let slot1 = imm(&mut func, 1);
        let value = imm(&mut func, 9);

        let (first_load, first_value) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        inst_value(&mut func, entry, InstKind::SStore(slot1, value), None);
        let (second_load, second_value) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        let (add, _) = inst_value(
            &mut func,
            entry,
            InstKind::Add(first_value, second_value),
            Some(MirType::uint256()),
        );
        func.blocks[entry].terminator = Some(Terminator::Stop);

        let mut pass = StorageLoadCse::new();
        assert_eq!(pass.run(&mut func), 1);

        assert!(func.blocks[entry].instructions.contains(&first_load));
        assert!(!func.blocks[entry].instructions.contains(&second_load));
        assert!(matches!(func.instructions[add].kind, InstKind::Add(lhs, _) if lhs == first_value));
    }

    #[test]
    fn keeps_load_when_reuse_would_extend_live_range() {
        let mut func = Function::new(Ident::DUMMY);
        let entry = func.entry_block;
        let slot0 = imm(&mut func, 0);
        let slot1 = imm(&mut func, 1);
        let value = imm(&mut func, 9);

        let (first_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        inst_value(&mut func, entry, InstKind::SStore(slot1, value), None);
        let (second_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        func.blocks[entry].terminator = Some(Terminator::Stop);

        let mut pass = StorageLoadCse::new();
        assert_eq!(pass.run(&mut func), 0);

        assert!(func.blocks[entry].instructions.contains(&first_load));
        assert!(func.blocks[entry].instructions.contains(&second_load));
    }

    #[test]
    fn keeps_load_after_same_slot_store() {
        let mut func = Function::new(Ident::DUMMY);
        let entry = func.entry_block;
        let slot = imm(&mut func, 0);
        let value = imm(&mut func, 9);

        let (first_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot), Some(MirType::uint256()));
        inst_value(&mut func, entry, InstKind::SStore(slot, value), None);
        let (second_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot), Some(MirType::uint256()));
        func.blocks[entry].terminator = Some(Terminator::Stop);

        let mut pass = StorageLoadCse::new();
        assert_eq!(pass.run(&mut func), 0);

        assert!(func.blocks[entry].instructions.contains(&first_load));
        assert!(func.blocks[entry].instructions.contains(&second_load));
    }

    #[test]
    fn treats_symbolic_store_as_possible_alias() {
        let mut func = Function::new(Ident::DUMMY);
        let entry = func.entry_block;
        func.params.push(MirType::uint256());
        let slot0 = imm(&mut func, 0);
        let symbolic_slot = func.alloc_value(Value::Arg { index: 0, ty: MirType::uint256() });
        let value = imm(&mut func, 9);

        let (first_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        inst_value(&mut func, entry, InstKind::SStore(symbolic_slot, value), None);
        let (second_load, _) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        func.blocks[entry].terminator = Some(Terminator::Stop);

        let mut pass = StorageLoadCse::new();
        assert_eq!(pass.run(&mut func), 0);

        assert!(func.blocks[entry].instructions.contains(&first_load));
        assert!(func.blocks[entry].instructions.contains(&second_load));
    }

    #[test]
    fn replaces_successor_phi_uses() {
        let mut func = Function::new(Ident::DUMMY);
        let entry = func.entry_block;
        let exit = func.alloc_block();
        let slot0 = imm(&mut func, 0);

        let (_, first_value) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        let (second_load, second_value) =
            inst_value(&mut func, entry, InstKind::SLoad(slot0), Some(MirType::uint256()));
        inst_value(
            &mut func,
            entry,
            InstKind::Add(first_value, second_value),
            Some(MirType::uint256()),
        );
        func.blocks[entry].terminator = Some(Terminator::Jump(exit));
        func.blocks[entry].successors.push(exit);
        func.blocks[exit].predecessors.push(entry);
        let (phi, _) = inst_value(
            &mut func,
            exit,
            InstKind::Phi(vec![(entry, second_value)]),
            Some(MirType::uint256()),
        );
        func.blocks[exit].terminator = Some(Terminator::Stop);

        let mut pass = StorageLoadCse::new();
        assert_eq!(pass.run(&mut func), 1);

        assert!(!func.blocks[entry].instructions.contains(&second_load));
        assert!(matches!(
            &func.instructions[phi].kind,
            InstKind::Phi(incoming) if incoming[0].1 == first_value
        ));
    }
}
