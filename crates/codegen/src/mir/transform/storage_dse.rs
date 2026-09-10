//! All-path dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block or across CFG edges when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.
//!
//! A backward must analysis intersects the definitely-overwritten slots of all
//! successors. Empty exit facts and a least fixed point prevent a loop from
//! justifying its own dead stores. Reads, possible aliases and unknown call
//! effects kill facts. Only after convergence are stores erased; local equal
//! store removal then runs with the same alias barriers. Run after storage
//! forwarding so packed read-modify-write chains expose their overwritten
//! stores without discarding preserved fields.

use crate::mir::{
    BlockId, Function, InstId, InstKind, Module, StorageAlias, Terminator, ValueId,
    analysis::{Access, AddressSpace, AliasAnalysis, CfgInfo, Location, ModRef},
    pass::{MirPass, run_function_pass_with_alias},
    utils as mir_utils,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::rc::Rc;

/// Function pass for all-path dead storage-store elimination.
pub(crate) struct StorageDse;

impl MirPass for StorageDse {
    fn name(&self) -> &'static str {
        "storage-dse"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_with_alias(module, analyses, |func, analyses| {
            let mut eliminator = StorageStoreEliminator::new();
            eliminator.alias = Some(Rc::clone(analyses.alias()));
            eliminator.run_to_fixpoint(func) != 0
        })
    }
}

/// All-path dead storage-store elimination pass.
#[derive(Debug, Default)]
struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    eliminated_count: usize,
    alias: Option<Rc<AliasAnalysis>>,
}

struct RunState {
    stored_values: FxHashMap<StorageAlias, ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self { stored_values: FxHashMap::default(), dead: DenseBitSet::new_empty(func.num_insts()) }
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

        let cfg = CfgInfo::new(func);
        let mut incoming = index_vec![FxHashSet::default(); func.blocks.len()];
        loop {
            let mut changed = false;
            for &block in cfg.rpo().iter().rev() {
                let mut facts = Self::successor_facts(func, &cfg, &incoming, block);
                self.transfer_reverse(func, block, &mut facts, None);
                if incoming[block] != facts {
                    incoming[block] = facts;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        state.dead.clear();
        for &block in cfg.rpo() {
            let mut facts = Self::successor_facts(func, &cfg, &incoming, block);
            self.transfer_reverse(func, block, &mut facts, Some(&mut state.dead));
        }
        self.eliminated_count += state.dead.count();
        // sstore slot, old; ...; sstore slot, new => ...; sstore slot, new
        for block in &mut func.blocks {
            block.instructions.retain(|id| !state.dead.contains(*id));
        }
        for block in func.blocks.indices() {
            self.remove_equal_stores(func, block, &mut state.stored_values, &mut state.dead);
        }

        self.eliminated_count
    }

    /// Runs storage DSE to a fixed point.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
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

    fn successor_facts(
        func: &Function,
        cfg: &CfgInfo,
        incoming: &IndexVec<BlockId, FxHashSet<StorageAlias>>,
        block: BlockId,
    ) -> FxHashSet<StorageAlias> {
        if !matches!(
            func.blocks[block].terminator,
            Some(Terminator::Jump(_) | Terminator::Branch { .. } | Terminator::Switch { .. })
        ) {
            return FxHashSet::default();
        }
        let mut successors = cfg.successors(block).iter();
        let Some(&first) = successors.next() else { return FxHashSet::default() };
        let mut facts = incoming[first].clone();
        for &successor in successors {
            facts.retain(|alias| incoming[successor].contains(alias));
        }
        facts
    }

    fn transfer_reverse(
        &self,
        func: &Function,
        block_id: BlockId,
        later_writes: &mut FxHashSet<StorageAlias>,
        mut dead: Option<&mut DenseBitSet<InstId>>,
    ) {
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            match &func.inst(inst_id).kind {
                InstKind::SStore(slot, _) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if later_writes.contains(&alias) {
                        if let Some(dead) = &mut dead {
                            dead.insert(inst_id);
                        }
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
            // A symbolic slot defined here denotes a different dynamic value
            // on the next loop iteration. Do not carry its overwrite fact
            // backward past its definition (including a loop-header phi).
            if let Some(value) = func.inst_result_value(inst_id) {
                later_writes.retain(|alias| match *alias {
                    StorageAlias::Slot(_) => true,
                    StorageAlias::Symbolic(base) | StorageAlias::Offset { base, .. } => {
                        base != value
                    }
                });
            }
        }
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
            match &func.inst(inst_id).kind {
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

        // Reverse transfer applies writes before reads. A packed aggregate
        // copy reports both for a read-modify-write slot: the read must remove
        // the write again so an earlier store feeding preserved bytes stays live.
        for &access in effects.writes() {
            if let Access::Location(Location::Storage(alias)) = access {
                Self::remove_aliasing_set(aa, later_writes, alias);
                later_writes.insert(alias);
            }
        }
        for &access in effects.reads() {
            if let Access::Location(Location::Storage(alias)) = access {
                Self::remove_aliasing_set(aa, later_writes, alias);
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
