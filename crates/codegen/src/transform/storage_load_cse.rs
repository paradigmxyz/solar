//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot.

use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, ValueId, utils as mir_utils},
    pass::{AnalysisManager, FunctionPass, LivenessAnalysis},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

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
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);

        let mut analyses = AnalysisManager::new();
        let liveness = analyses.get_or_compute(&LivenessAnalysis, func);
        let inst_results = func.inst_results();
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let mut replacements = FxHashMap::default();
        let mut dead = DenseBitSet::new_empty(func.instructions.len());

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
                block.instructions.retain(|&id| !dead.contains(id));
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
        dead: &mut DenseBitSet<InstId>,
    ) {
        let mut cached_loads: FxHashMap<StorageAlias, ValueId> = FxHashMap::default();
        let inst_ids = func.blocks[block_id].instructions.clone();

        for (inst_idx, inst_id) in inst_ids.into_iter().enumerate() {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(slot) => {
                    let alias = func.storage_alias_after_replacements(inst_id, *slot, replacements);
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
                    let alias = func.storage_alias_after_replacements(inst_id, *slot, replacements);
                    cached_loads.retain(|cached_alias, _| !cached_alias.may_alias(alias));
                }
                kind if kind.may_mutate_storage() => cached_loads.clear(),
                _ => {}
            }
        }
    }

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        for inst in func.instructions.iter_mut() {
            mir_utils::replace_inst_uses_canonicalized(&mut inst.kind, replacements);
            if matches!(inst.kind, InstKind::SLoad(_) | InstKind::SStore(_, _)) {
                inst.metadata.set_storage_alias(None);
            }
        }

        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                mir_utils::replace_terminator_uses_canonicalized(term, replacements);
            }
        }
    }
}
