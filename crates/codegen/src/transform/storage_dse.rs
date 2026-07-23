//! Local dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.

use crate::{
    analysis::{Access, AddressSpace, AliasAnalysis, Location, ModRef},
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, ValueId, utils as mir_utils},
    pass::FunctionPass,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Local dead storage-store elimination pass.
#[derive(Debug, Default)]
pub(crate) struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    pub eliminated_count: usize,
    alias: Option<AliasAnalysis>,
}

struct RunState {
    later_writes: FxHashSet<StorageAlias>,
    stored_values: FxHashMap<StorageAlias, ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self {
            later_writes: FxHashSet::default(),
            stored_values: FxHashMap::default(),
            dead: DenseBitSet::new_empty(func.instructions.len()),
        }
    }
}

/// Function pass for local dead storage-store elimination.
pub(crate) struct StorageDsePass;

impl FunctionPass for StorageDsePass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        StorageStoreEliminator::new().run_to_fixpoint(func) != 0
    }
}

impl StorageStoreEliminator {
    /// Creates a new storage-store eliminator.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        self.alias = Some(AliasAnalysis::new(func));

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

    fn remove_overwritten_stores(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        later_writes: &mut FxHashSet<StorageAlias>,
        dead: &mut DenseBitSet<InstId>,
    ) {
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        later_writes.clear();
        dead.clear();

        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            match &func.instructions[inst_id].kind {
                InstKind::SStore(slot, _) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if later_writes.contains(&alias) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_aliasing_set(aa, later_writes, alias);
                    later_writes.insert(alias);
                }
                InstKind::SLoad(slot) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    Self::remove_aliasing_set(aa, later_writes, alias);
                }
                _ => {
                    let effects = aa.instruction_mod_ref(func, inst_id);
                    Self::apply_reverse_effects(aa, &effects, later_writes);
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
        stored_values: &mut FxHashMap<StorageAlias, ValueId>,
        dead: &mut DenseBitSet<InstId>,
    ) {
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        stored_values.clear();
        dead.clear();

        for &inst_id in &func.blocks[block_id].instructions {
            match &func.instructions[inst_id].kind {
                InstKind::SStore(slot, value) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if stored_values.get(&alias).is_some_and(|&stored| stored == *value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_aliasing_map(aa, stored_values, alias);
                    stored_values.insert(alias, *value);
                }
                _ => {
                    let effects = aa.instruction_mod_ref(func, inst_id);
                    Self::apply_forward_writes(aa, &effects, stored_values);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !dead.contains(id));
    }

    fn remove_aliasing_set(
        aa: &AliasAnalysis,
        aliases: &mut FxHashSet<StorageAlias>,
        alias: StorageAlias,
    ) {
        aliases.retain(|cached| {
            !aa.alias(Location::Storage(*cached), Location::Storage(alias)).may_alias()
        });
    }

    fn remove_aliasing_map(
        aa: &AliasAnalysis,
        values: &mut FxHashMap<StorageAlias, ValueId>,
        alias: StorageAlias,
    ) {
        values.retain(|cached, _| {
            !aa.alias(Location::Storage(*cached), Location::Storage(alias)).may_alias()
        });
    }

    fn apply_reverse_effects(
        aa: &AliasAnalysis,
        effects: &ModRef,
        later_writes: &mut FxHashSet<StorageAlias>,
    ) {
        if effects.reads_anywhere(AddressSpace::Storage)
            || effects.writes_anywhere(AddressSpace::Storage)
        {
            later_writes.clear();
            return;
        }

        for &access in effects.reads() {
            if let Access::Location(Location::Storage(alias)) = access {
                Self::remove_aliasing_set(aa, later_writes, alias);
            }
        }
        for &access in effects.writes() {
            if let Access::Location(Location::Storage(alias)) = access {
                Self::remove_aliasing_set(aa, later_writes, alias);
                later_writes.insert(alias);
            }
        }
    }

    fn apply_forward_writes(
        aa: &AliasAnalysis,
        effects: &ModRef,
        stored_values: &mut FxHashMap<StorageAlias, ValueId>,
    ) {
        if effects.writes_anywhere(AddressSpace::Storage) {
            stored_values.clear();
            return;
        }
        for &access in effects.writes() {
            if let Access::Location(Location::Storage(alias)) = access {
                Self::remove_aliasing_map(aa, stored_values, alias);
            }
        }
    }
}
