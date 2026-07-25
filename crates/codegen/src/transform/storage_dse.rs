//! Local dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.

use crate::{
    analysis::{Access, AddressSpace, AliasAnalysis, Location, ModRef},
    mir::{BlockId, Function, InstId, InstKind, Module, StorageAlias, ValueId, utils as mir_utils},
    pass::{MirPass, run_function_pass_with_alias_filtered},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::rc::Rc;

/// Function pass for local dead storage-store elimination.
pub(crate) struct StorageDse;

impl MirPass for StorageDse {
    fn name(&self) -> &'static str {
        "storage-dse"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_with_alias_filtered(
            module,
            analyses,
            |_, func| {
                func.instructions().any(|inst_id| {
                    matches!(func.inst(inst_id).kind, InstKind::SLoad(_) | InstKind::SStore(_, _))
                })
            },
            |func, alias| {
                let mut eliminator = StorageStoreEliminator::new();
                eliminator.alias = Some(Rc::clone(alias));
                eliminator.run_to_fixpoint(func) != 0
            },
        )
    }
}

/// Local dead storage-store elimination pass.
#[derive(Debug, Default)]
struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    eliminated_count: usize,
    alias: Option<Rc<AliasAnalysis>>,
}

struct RunState {
    later_writes: StorageAliases<()>,
    stored_values: StorageAliases<ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self {
            later_writes: StorageAliases::default(),
            stored_values: StorageAliases::default(),
            dead: DenseBitSet::new_empty(func.num_insts()),
        }
    }
}

/// Separates exact slots, which only alias on equality, from symbolic aliases.
#[derive(Debug)]
struct StorageAliases<T> {
    slots: FxHashMap<U256, T>,
    symbolic: FxHashMap<StorageAlias, T>,
    symbolic_base: Option<ValueId>,
}

impl<T> Default for StorageAliases<T> {
    fn default() -> Self {
        Self { slots: FxHashMap::default(), symbolic: FxHashMap::default(), symbolic_base: None }
    }
}

impl<T> StorageAliases<T> {
    fn clear(&mut self) {
        self.slots.clear();
        self.symbolic.clear();
        self.symbolic_base = None;
    }

    fn get(&self, alias: StorageAlias) -> Option<&T> {
        match alias {
            StorageAlias::Slot(slot) => self.slots.get(&slot),
            alias => self.symbolic.get(&alias),
        }
    }

    fn insert(&mut self, alias: StorageAlias, value: T) {
        match alias {
            StorageAlias::Slot(slot) => {
                self.slots.insert(slot, value);
            }
            alias => {
                let base = alias.symbolic_base().expect("symbolic alias has a base");
                if self.symbolic_base.is_some_and(|current| current != base) {
                    self.symbolic.clear();
                }
                self.symbolic_base = Some(base);
                self.symbolic.insert(alias, value);
            }
        }
    }

    fn remove_aliasing(&mut self, alias: StorageAlias) {
        match alias {
            StorageAlias::Slot(slot) => {
                self.slots.remove(&slot);
                self.symbolic.clear();
                self.symbolic_base = None;
            }
            StorageAlias::Symbolic(base) => {
                self.slots.clear();
                if self.symbolic_base != Some(base) {
                    self.symbolic.clear();
                    self.symbolic_base = None;
                    return;
                }
                self.symbolic.remove(&StorageAlias::Symbolic(base));
                self.symbolic.remove(&StorageAlias::Offset { base, offset: U256::ZERO });
            }
            StorageAlias::Offset { base, offset } => {
                self.slots.clear();
                if self.symbolic_base != Some(base) {
                    self.symbolic.clear();
                    self.symbolic_base = None;
                    return;
                }
                self.symbolic.remove(&StorageAlias::Offset { base, offset });
                if offset.is_zero() {
                    self.symbolic.remove(&StorageAlias::Symbolic(base));
                }
            }
        }
    }
}

impl StorageStoreEliminator {
    /// Creates a new storage-store eliminator.
    fn new() -> Self {
        Self::default()
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.remove_overwritten_stores(
                func,
                block_id,
                &mut state.later_writes,
                &mut state.dead,
            );
            self.remove_equal_stores(func, block_id, &mut state.stored_values, &mut state.dead);
        }

        self.eliminated_count
    }

    /// Runs local storage DSE to a fixed point.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut state = RunState::new(func);
        self.run_with_state(func, &mut state)
    }

    fn remove_overwritten_stores(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        later_writes: &mut StorageAliases<()>,
        dead: &mut DenseBitSet<InstId>,
    ) {
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        later_writes.clear();
        dead.clear();

        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            match &func.inst(inst_id).kind {
                InstKind::SStore(slot, _) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if later_writes.get(alias).is_some() {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    later_writes.remove_aliasing(alias);
                    later_writes.insert(alias, ());
                }
                InstKind::SLoad(slot) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    later_writes.remove_aliasing(alias);
                }
                _ => {
                    let effects = aa.instruction_mod_ref(func, inst_id);
                    Self::apply_reverse_effects(&effects, later_writes);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !dead.contains(id));
    }

    fn remove_equal_stores(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        stored_values: &mut StorageAliases<ValueId>,
        dead: &mut DenseBitSet<InstId>,
    ) {
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        stored_values.clear();
        dead.clear();

        for &inst_id in &func.blocks[block_id].instructions {
            match &func.inst(inst_id).kind {
                InstKind::SStore(slot, value) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if stored_values.get(alias).is_some_and(|&stored| stored == *value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    stored_values.remove_aliasing(alias);
                    stored_values.insert(alias, *value);
                }
                _ => {
                    let effects = aa.instruction_mod_ref(func, inst_id);
                    Self::apply_forward_writes(&effects, stored_values);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !dead.contains(id));
    }

    fn apply_reverse_effects(effects: &ModRef, later_writes: &mut StorageAliases<()>) {
        if effects.reads_anywhere(AddressSpace::Storage)
            || effects.writes_anywhere(AddressSpace::Storage)
        {
            later_writes.clear();
            return;
        }

        for &access in effects.reads() {
            if let Access::Location(Location::Storage(alias)) = access {
                later_writes.remove_aliasing(alias);
            }
        }
        for &access in effects.writes() {
            if let Access::Location(Location::Storage(alias)) = access {
                later_writes.remove_aliasing(alias);
                later_writes.insert(alias, ());
            }
        }
    }

    fn apply_forward_writes(effects: &ModRef, stored_values: &mut StorageAliases<ValueId>) {
        if effects.writes_anywhere(AddressSpace::Storage) {
            stored_values.clear();
            return;
        }
        for &access in effects.writes() {
            if let Access::Location(Location::Storage(alias)) = access {
                stored_values.remove_aliasing(alias);
            }
        }
    }
}
