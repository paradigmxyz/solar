//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot.

use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, Terminator, Value, ValueId},
    pass::{AnalysisManager, FunctionPass, LivenessAnalysis},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Local storage load CSE pass.
#[derive(Debug, Default)]
pub struct StorageLoadCse {
    /// Number of storage loads eliminated.
    pub eliminated_count: usize,
}

/// Function pass for straight-line storage-load CSE.
pub struct StorageLoadCsePass;

impl FunctionPass for StorageLoadCsePass {
    fn name(&self) -> &str {
        "storage-load-cse"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        StorageLoadCse::new().run_to_fixpoint(func) != 0
    }
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

        let mut analyses = AnalysisManager::new();
        let liveness = analyses.get_or_compute(&LivenessAnalysis, func);
        let inst_results = Self::inst_results(func);
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();

        for block_id in block_ids {
            self.process_block(
                func,
                block_id,
                liveness,
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
                kind if kind.may_mutate_storage() => cached_loads.clear(),
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
                slot.map(|slot| StorageAlias::for_value(func, slot));
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
                .unwrap_or_else(|| StorageAlias::for_value(func, slot))
        } else {
            StorageAlias::for_value(func, slot)
        }
    }

    fn storage_aliases_may_alias(a: &StorageAlias, b: &StorageAlias) -> bool {
        a.may_alias(*b)
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
        kind.visit_operands_mut(|value| {
            *value = Self::canonical_value(*value, replacements);
        });
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
