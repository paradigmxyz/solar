//! Local dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.

use crate::{
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, ValueId, utils as mir_utils},
    pass::FunctionPass,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Local dead storage-store elimination pass.
#[derive(Debug, Default)]
pub struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    pub eliminated_count: usize,
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
pub struct StorageDsePass;

impl FunctionPass for StorageDsePass {
    fn name(&self) -> &str {
        "storage-dse"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        StorageStoreEliminator::new().run_to_fixpoint(func) != 0
    }
}

impl StorageStoreEliminator {
    /// Creates a new storage-store eliminator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs local storage DSE on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        let mut state = RunState::new(func);
        self.run_with_state(func, &mut state)
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);

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
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
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
        later_writes.clear();
        dead.clear();

        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            match &func.instructions[inst_id].kind {
                InstKind::SStore(slot, _) => {
                    let alias = func.storage_alias(inst_id, *slot);
                    if !later_writes.insert(alias) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    later_writes.retain(|cached| *cached == alias || !cached.may_alias(alias));
                }
                InstKind::SLoad(slot) => {
                    let alias = func.storage_alias(inst_id, *slot);
                    Self::remove_aliasing_set(later_writes, alias);
                }
                kind if Self::may_observe_or_mutate_storage(kind) => {
                    later_writes.clear();
                }
                _ => {}
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
        stored_values.clear();
        dead.clear();

        for &inst_id in &func.blocks[block_id].instructions {
            match &func.instructions[inst_id].kind {
                InstKind::SStore(slot, value) => {
                    let alias = func.storage_alias(inst_id, *slot);
                    if stored_values.get(&alias).is_some_and(|&stored| stored == *value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_aliasing_map(stored_values, alias);
                    stored_values.insert(alias, *value);
                }
                kind if Self::may_observe_or_mutate_storage(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !dead.contains(id));
    }

    fn remove_aliasing_set(aliases: &mut FxHashSet<StorageAlias>, alias: StorageAlias) {
        aliases.retain(|cached| !cached.may_alias(alias));
    }

    fn remove_aliasing_map(values: &mut FxHashMap<StorageAlias, ValueId>, alias: StorageAlias) {
        values.retain(|cached, _| !cached.may_alias(alias));
    }

    fn may_observe_or_mutate_storage(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
        ) || kind.may_mutate_storage()
    }
}
