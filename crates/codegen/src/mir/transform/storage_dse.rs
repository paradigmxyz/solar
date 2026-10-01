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
//! stores without discarding preserved fields. A `gas` read clears both
//! forward and backward facts so measured storage writes remain explicit.
//!
//! With `-Zdataflow-optimizations`, calls use the experimental framework's storage facts.
//! Crossing a call backward kills
//! only the overwrite facts its footprint may read, so a store stays dead across a
//! helper that reads other storage; a call whose callee may end the transaction
//! successfully kills every fact, because the later overwrite may never run.
//! External calls that may re-enter can read arbitrary slots, including callbacks
//! reached through internal helpers. Forward equal-store removal keeps stored values
//! its footprint cannot write.

use crate::mir::{
    BlockId, Callee, Function, InstId, InstKind, Module, StorageAlias, Terminator, ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CfgInfo, Location, ModRef,
        dataflow::{
            facts::StorageFacts,
            reentrancy::call_operands,
            storage_path::{PathId, PathTable},
        },
    },
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
    utils as mir_utils,
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::{rc::Rc, sync::Arc};

/// Function pass for all-path dead storage-store elimination.
pub(crate) struct StorageDse;

impl MirPass for StorageDse {
    fn name(&self) -> &'static str {
        "storage-dse"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        let mut fact_candidates = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::SStore(..)))
            {
                selected.insert(id);
                if gcx.sess.opts.unstable.dataflow_optimizations
                    && func
                        .instructions()
                        .filter(|&inst| matches!(func.inst(inst).kind, InstKind::SStore(..)))
                        .take(2)
                        .count()
                        == 2
                    && func.instructions().any(|inst| is_call(&func.inst(inst).kind))
                {
                    fact_candidates.insert(id);
                }
            }
        }
        let facts = (!fact_candidates.is_empty())
            .then(|| StorageFacts::compute(module, gcx.sess.opts.evm_version, &fact_candidates));
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let mut eliminator = StorageStoreEliminator::new();
                eliminator.facts = facts.clone();
                eliminator.alias = Some(Rc::clone(analyses.alias()));
                let changed = eliminator.run_to_fixpoint(func) != 0;
                if eliminator.annotated_aliases {
                    analyses.note_unreported_edit();
                }
                changed
            },
        )
    }
}

/// Returns whether `kind` transfers control to other code, internal or external.
fn is_call(kind: &InstKind) -> bool {
    matches!(kind, InstKind::ICall { function: Callee::Function(_), .. })
        || call_operands(kind).is_some()
}

/// All-path dead storage-store elimination pass.
#[derive(Debug, Default)]
struct StorageStoreEliminator {
    /// Number of storage stores eliminated.
    eliminated_count: usize,
    alias: Option<Rc<AliasAnalysis>>,
    /// Storage footprints of calls, computed for the module at pass entry.
    facts: Option<Arc<StorageFacts>>,
    /// Storage paths of each alias accessed by a load or store of the function.
    alias_paths: FxHashMap<StorageAlias, SmallVec<[PathId; 2]>>,
    /// Whether storage-alias annotation changed metadata, which is not reported as a change.
    annotated_aliases: bool,
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
        self.annotated_aliases |=
            func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }
        self.collect_alias_paths(func);

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
                    if later_writes.contains(&alias)
                        || self.overwritten_elsewhere(later_writes, alias)
                    {
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
                    if !effects.observes_gas()
                        && self.apply_reverse_call(func, inst_id, later_writes)
                    {
                        // The call's storage footprint decided which facts survive.
                    } else {
                        Self::apply_reverse_effects(aa, &effects, later_writes);
                    }
                }
            }
            // A symbolic slot defined here denotes a different dynamic value
            // on the next loop iteration. Do not carry its overwrite fact
            // backward past its definition (including a loop-header phi),
            // unless its storage path is built only from keys that stay fixed
            // for the whole activation.
            if let Some(value) = func.inst_result_value(inst_id) {
                later_writes.retain(|alias| match *alias {
                    StorageAlias::Slot(_) => true,
                    StorageAlias::Symbolic(base) | StorageAlias::Offset { base, .. } => {
                        base != value || self.is_stable_alias(*alias)
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
                    if !effects.observes_gas()
                        && is_call(&func.inst(inst_id).kind)
                        && let Some(facts) = self.facts.as_deref()
                        && let Some(footprint) = facts.footprint(func, inst_id)
                    {
                        let alias_paths = &self.alias_paths;
                        stored_values.retain(|alias, _| {
                            alias_paths.get(alias).is_some_and(|paths| {
                                !facts.may_touch(paths, footprint.writes, footprint.reentrant)
                            })
                        });
                        continue;
                    }
                    Self::apply_forward_writes(aa, &effects, stored_values);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !dead.contains(id));
    }

    /// Records the storage paths of every alias the function's loads and stores access.
    fn collect_alias_paths(&mut self, func: &Function) {
        self.alias_paths.clear();
        let Some(facts) = self.facts.as_deref() else { return };
        let aa = self.alias.as_ref().expect("storage DSE alias snapshot is initialized");
        for inst in func.instructions() {
            let (InstKind::SLoad(slot) | InstKind::SStore(slot, _)) = func.inst(inst).kind else {
                continue;
            };
            let alias = aa.storage_alias(func, inst, slot);
            let paths = facts.slot_paths(func, inst).map(|paths| paths.iter().copied().collect());
            match (self.alias_paths.get(&alias), paths) {
                (Some(known), Some(paths)) if *known == paths => {}
                (None, Some(paths)) => {
                    self.alias_paths.insert(alias, paths);
                }
                // Missing or disagreeing paths leave the alias unknown.
                _ => {
                    self.alias_paths.insert(alias, SmallVec::from_elem(PathTable::UNKNOWN, 1));
                }
            }
        }
    }

    /// Returns whether `alias` denotes one slot for the whole activation.
    fn is_stable_alias(&self, alias: StorageAlias) -> bool {
        let (Some(facts), Some(paths)) = (self.facts.as_deref(), self.alias_paths.get(&alias))
        else {
            return false;
        };
        facts.same_location(paths, paths)
    }

    /// Returns whether a later store to the same slot through another SSA value overwrites
    /// `alias`, according to the storage facts' paths.
    fn overwritten_elsewhere(
        &self,
        later_writes: &FxHashSet<StorageAlias>,
        alias: StorageAlias,
    ) -> bool {
        let (Some(facts), Some(paths)) = (self.facts.as_deref(), self.alias_paths.get(&alias))
        else {
            return false;
        };
        later_writes.iter().any(|later| {
            self.alias_paths.get(later).is_some_and(|other| facts.same_location(paths, other))
        })
    }

    /// Crosses a call backward using its storage footprint. Returns whether it applied.
    fn apply_reverse_call(
        &self,
        func: &Function,
        inst: InstId,
        later_writes: &mut FxHashSet<StorageAlias>,
    ) -> bool {
        if !is_call(&func.inst(inst).kind) {
            return false;
        }
        let Some(facts) = self.facts.as_deref() else { return false };
        let Some(footprint) = facts.footprint(func, inst) else { return false };
        if footprint.terminates {
            later_writes.clear();
            return true;
        }
        later_writes.retain(|alias| {
            self.alias_paths
                .get(alias)
                .is_some_and(|paths| !facts.may_touch(paths, footprint.reads, footprint.reentrant))
        });
        true
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
        if effects.observes_gas()
            || effects.reads_anywhere(AddressSpace::Storage)
            || effects.writes_anywhere(AddressSpace::Storage)
        {
            later_writes.clear();
            return;
        }

        // ModRef describes possible writes, not definite overwrites. Only
        // an explicit sstore can establish a later write on every path.
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
        if effects.observes_gas() || effects.writes_anywhere(AddressSpace::Storage) {
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
