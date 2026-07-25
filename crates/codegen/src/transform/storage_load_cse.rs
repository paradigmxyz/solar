//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot.

use crate::{
    analysis::{Access, AddressSpace, AliasAnalysis, Liveness, Location},
    mir::{BlockId, Function, InstId, InstKind, Module, StorageAlias, ValueId, utils as mir_utils},
    pass::{AnalysisManager, LivenessAnalysis, MirPass, run_function_pass_with_alias_filtered},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use std::rc::Rc;

/// Function pass for straight-line storage-load CSE.
pub(crate) struct StorageLoadCse;

impl MirPass for StorageLoadCse {
    fn name(&self) -> &'static str {
        "storage-load-cse"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut cse = StorageLoadCseCx::new();
        let mut state = RunState::new();
        run_function_pass_with_alias_filtered(
            module,
            analyses,
            |_, func| {
                func.instructions().any(|inst_id| {
                    matches!(func.inst(inst_id).kind, InstKind::SLoad(_) | InstKind::SStore(_, _))
                })
            },
            |func, alias| {
                cse.alias = Some(Rc::clone(alias));
                cse.run_with_state(func, &mut state) != 0
            },
        )
    }
}

/// Local storage load CSE pass.
#[derive(Debug, Default)]
struct StorageLoadCseCx {
    /// Number of storage loads eliminated.
    eliminated_count: usize,
    alias: Option<Rc<AliasAnalysis>>,
}

struct RunState {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: GrowableBitSet<InstId>,
    cached_loads: CachedStorageLoads,
}

impl RunState {
    fn new() -> Self {
        Self {
            replacements: FxHashMap::default(),
            dead: GrowableBitSet::new_empty(),
            cached_loads: CachedStorageLoads::default(),
        }
    }
}

/// Groups symbolic load aliases by base so writes only inspect aliases they can preserve.
#[derive(Debug, Default)]
struct CachedStorageLoads {
    slots: FxHashMap<U256, ValueId>,
    symbolic: FxHashMap<ValueId, FxHashMap<StorageAlias, ValueId>>,
}

impl CachedStorageLoads {
    fn clear(&mut self) {
        self.slots.clear();
        self.symbolic.clear();
    }

    fn get(&self, alias: StorageAlias) -> Option<&ValueId> {
        match alias {
            StorageAlias::Slot(slot) => self.slots.get(&slot),
            alias => self.symbolic.get(&alias.symbolic_base()?)?.get(&alias),
        }
    }

    fn insert(&mut self, alias: StorageAlias, value: ValueId) {
        match alias {
            StorageAlias::Slot(slot) => {
                self.slots.insert(slot, value);
            }
            alias => {
                let base = alias.symbolic_base().expect("symbolic alias has a base");
                self.symbolic.entry(base).or_default().insert(alias, value);
            }
        }
    }

    fn remove_aliasing(&mut self, alias: StorageAlias) {
        match alias {
            StorageAlias::Slot(slot) => {
                self.slots.remove(&slot);
                self.symbolic.clear();
            }
            StorageAlias::Symbolic(base) => {
                self.slots.clear();
                let Some(mut aliases) = self.symbolic.remove(&base) else {
                    self.symbolic.clear();
                    return;
                };
                self.symbolic.clear();
                aliases.remove(&StorageAlias::Symbolic(base));
                aliases.remove(&StorageAlias::Offset { base, offset: U256::ZERO });
                if !aliases.is_empty() {
                    self.symbolic.insert(base, aliases);
                }
            }
            StorageAlias::Offset { base, offset } => {
                self.slots.clear();
                let Some(mut aliases) = self.symbolic.remove(&base) else {
                    self.symbolic.clear();
                    return;
                };
                self.symbolic.clear();
                aliases.remove(&StorageAlias::Offset { base, offset });
                if offset.is_zero() {
                    aliases.remove(&StorageAlias::Symbolic(base));
                }
                if !aliases.is_empty() {
                    self.symbolic.insert(base, aliases);
                }
            }
        }
    }
}

impl StorageLoadCseCx {
    /// Creates a new storage-load CSE pass.
    fn new() -> Self {
        Self::default()
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }

        let mut analyses = AnalysisManager::new();
        let liveness = analyses.get_or_compute(&LivenessAnalysis, func);
        state.replacements.clear();
        state.dead.clear();

        for block_id in func.blocks.indices() {
            state.cached_loads.clear();
            self.process_block(func, block_id, liveness, state);
        }

        if !state.replacements.is_empty() {
            Self::replace_uses(func, &state.replacements);
        }
        if !state.dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !state.dead.contains(id));
            }
        }
        if !state.replacements.is_empty() {
            func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        }

        self.eliminated_count
    }

    fn process_block(
        &mut self,
        func: &Function,
        block_id: BlockId,
        liveness: &Liveness,
        state: &mut RunState,
    ) {
        let aa = self.alias.as_ref().expect("storage-load CSE alias snapshot is initialized");
        for (inst_idx, &inst_id) in func.blocks[block_id].instructions.iter().enumerate() {
            match &func.inst(inst_id).kind {
                InstKind::SLoad(slot) => {
                    let alias = aa.storage_alias_after_replacements(
                        func,
                        inst_id,
                        *slot,
                        &state.replacements,
                    );
                    let Some(result) = func.inst_result_value(inst_id) else {
                        continue;
                    };
                    if let Some(&cached) = state.cached_loads.get(alias) {
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
                    let alias = aa.storage_alias_after_replacements(
                        func,
                        inst_id,
                        *slot,
                        &state.replacements,
                    );
                    state.cached_loads.remove_aliasing(alias);
                }
                _ => {
                    let effects = aa.instruction_mod_ref_with_replacements(
                        func,
                        inst_id,
                        &state.replacements,
                    );
                    for &access in effects.writes() {
                        match access {
                            Access::Any(AddressSpace::Storage) => {
                                state.cached_loads.clear();
                                break;
                            }
                            Access::Location(Location::Storage(alias)) => {
                                state.cached_loads.remove_aliasing(alias);
                            }
                            Access::Any(AddressSpace::Memory | AddressSpace::Transient)
                            | Access::Location(Location::Memory(_) | Location::Transient(_)) => {}
                        }
                    }
                }
            }
        }
    }

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        func.for_each_instruction_mut(|_, inst| {
            mir_utils::replace_inst_uses_canonicalized(&mut inst.kind, replacements);
            if matches!(inst.kind, InstKind::SLoad(_) | InstKind::SStore(_, _)) {
                inst.metadata.set_storage_alias(None);
            }
        });

        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                mir_utils::replace_terminator_uses_canonicalized(term, replacements);
            }
        }
    }
}
