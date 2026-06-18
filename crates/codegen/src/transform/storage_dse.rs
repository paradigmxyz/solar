//! Local dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.

use crate::{
    mir::{BlockId, Function, InstId, InstKind, StorageAlias, ValueId},
    pass::FunctionPass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Local dead storage-store elimination pass.
#[derive(Debug, Default)]
pub struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    pub eliminated_count: usize,
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
        self.eliminated_count = 0;
        self.annotate_storage_aliases(func);

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.remove_overwritten_stores(func, block_id);
            self.remove_equal_stores(func, block_id);
        }

        self.eliminated_count
    }

    /// Runs local storage DSE to a fixed point.
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

    fn remove_overwritten_stores(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut later_writes: FxHashSet<StorageAlias> = FxHashSet::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in inst_ids.iter().rev() {
            match func.instructions[inst_id].kind() {
                InstKind::SStore(slot, _) => {
                    let alias = self.storage_alias(func, inst_id, slot);
                    if later_writes.contains(&alias) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_aliasing_set(&mut later_writes, alias);
                    later_writes.insert(alias);
                }
                InstKind::SLoad(slot) => {
                    let alias = self.storage_alias(func, inst_id, slot);
                    Self::remove_aliasing_set(&mut later_writes, alias);
                }
                kind if Self::may_observe_or_mutate_storage(&kind) => {
                    later_writes.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn remove_equal_stores(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut stored_values: FxHashMap<StorageAlias, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            match func.instructions[inst_id].kind() {
                InstKind::SStore(slot, value) => {
                    let alias = self.storage_alias(func, inst_id, slot);
                    if stored_values.get(&alias).is_some_and(|&stored| stored == value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_aliasing_map(&mut stored_values, alias);
                    stored_values.insert(alias, value);
                }
                kind if Self::may_observe_or_mutate_storage(&kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn annotate_storage_aliases(&self, func: &mut Function) {
        let inst_ids: Vec<_> =
            func.instructions.iter_enumerated().map(|(inst_id, _)| inst_id).collect();
        for inst_id in inst_ids {
            let slot = match func.instructions[inst_id].kind() {
                InstKind::SLoad(slot) | InstKind::SStore(slot, _) => Some(slot),
                _ => None,
            };
            let alias = slot.map(|slot| StorageAlias::for_value(func, slot));
            func.instructions[inst_id].metadata.set_storage_alias(alias);
        }
    }

    fn storage_alias(&self, func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
        func.instructions[inst_id]
            .metadata
            .storage_alias()
            .unwrap_or_else(|| StorageAlias::for_value(func, slot))
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
