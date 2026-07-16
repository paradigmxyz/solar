//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot.

use crate::{
    analysis::{AddressSpace, AliasAnalysis, Liveness, Location},
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, ValueId, utils as mir_utils},
    pass::{AnalysisManager, FunctionPass, LivenessAnalysis},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Local storage load CSE pass.
#[derive(Debug, Default)]
pub(crate) struct StorageLoadCse {
    /// Number of storage loads eliminated.
    pub eliminated_count: usize,
}

struct RunState {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
    cached_loads: FxHashMap<StorageAlias, ValueId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self {
            replacements: FxHashMap::default(),
            dead: DenseBitSet::new_empty(func.instructions.len()),
            cached_loads: FxHashMap::default(),
        }
    }
}

/// Function pass for straight-line storage-load CSE.
pub(crate) struct StorageLoadCsePass;

impl FunctionPass for StorageLoadCsePass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        StorageLoadCse::new().run_to_fixpoint(func) != 0
    }
}

impl StorageLoadCse {
    /// Creates a new storage-load CSE pass.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);

        let mut analyses = AnalysisManager::new();
        let liveness = analyses.get_or_compute(&LivenessAnalysis, func);
        let inst_results = func.inst_results();
        state.replacements.clear();
        state.dead.clear();

        for block_id in func.blocks.indices() {
            state.cached_loads.clear();
            self.process_block(func, block_id, liveness, &inst_results, state);
        }

        if !state.replacements.is_empty() {
            Self::replace_uses(func, &state.replacements);
        }
        if !state.dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !state.dead.contains(id));
            }
        }

        self.eliminated_count
    }

    /// Runs storage-load CSE to a fixed point.
    pub(crate) fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        let mut state = RunState::new(func);
        loop {
            let eliminated = self.run_with_state(func, &mut state);
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
        state: &mut RunState,
    ) {
        let aa = AliasAnalysis;
        for (inst_idx, &inst_id) in func.blocks[block_id].instructions.iter().enumerate() {
            match &func.instructions[inst_id].kind {
                InstKind::SLoad(slot) => {
                    let alias = aa.storage_alias_after_replacements(
                        func,
                        inst_id,
                        *slot,
                        &state.replacements,
                    );
                    let Some(&result) = inst_results.get(&inst_id) else {
                        continue;
                    };
                    if let Some(&cached) = state.cached_loads.get(&alias) {
                        if !liveness.is_used_at_or_after(cached, block_id, inst_idx) {
                            state.cached_loads.insert(alias, result);
                            continue;
                        }
                        state.replacements.insert(result, cached);
                        state.dead.insert(inst_id);
                        self.eliminated_count += 1;
                    } else {
                        state.cached_loads.insert(alias, result);
                    }
                }
                InstKind::SStore(slot, _) => {
                    let alias =
                        aa.storage_alias_after_replacements(
                            func,
                            inst_id,
                            *slot,
                            &state.replacements,
                        );
                    state.cached_loads.retain(|cached_alias, _| {
                        !aa.alias(Location::Storage(*cached_alias), Location::Storage(alias))
                            .may_alias()
                    });
                }
                _ if aa
                    .instruction_mod_ref_with_replacements(func, inst_id, &state.replacements)
                    .writes_anywhere(AddressSpace::Storage) =>
                {
                    state.cached_loads.clear();
                }
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
