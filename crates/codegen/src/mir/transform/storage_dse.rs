//! All-path dead storage-store elimination.
//!
//! This pass removes persistent `sstore` instructions inside a single basic
//! block or across CFG edges when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! repeated equal stores when no intervening instruction can clobber storage.
//!
//! A backward must analysis intersects the definitely-overwritten slots of all
//! successors. An exit that returns or stops starts with no slots, and a least
//! fixed point prevents a loop from justifying its own dead stores. An exit that
//! reverts, directly or through a tail call to a function that never returns,
//! starts with every slot: the revert discards every storage write, so a store
//! that only reaches it is dead unless something reads the slot first. Reads,
//! possible aliases and unknown call effects kill facts. Only after convergence are stores erased;
//! local equal store removal then runs with the same alias barriers. Run after storage
//! forwarding so packed read-modify-write chains expose their overwritten
//! stores without discarding preserved fields. A `gas` read clears both
//! forward and backward facts so measured storage writes remain explicit.
//! Calls that may terminate the current EVM call clear backward facts because
//! they can commit earlier writes without reaching the later overwrite.

use crate::mir::{
    BlockId, Function, FunctionId, InstId, InstKind, Module, PtrLocation, StorageAlias, Terminator,
    ValueId,
    analysis::{Access, AddressSpace, AliasAnalysis, CfgInfo, Location, ModRef, returning_blocks},
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
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
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        let mut reverting = DenseBitSet::new_empty(module.functions.len());
        let summaries = analyses.call_summaries(module);
        for (id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst| {
                matches!(func.inst(inst).kind, InstKind::Store(.., PtrLocation::Storage))
            }) {
                selected.insert(id);
            }
            // A callee that can halt successfully, itself or through its own calls, commits
            // storage even when it never returns.
            if !func.blocks.is_empty()
                && !returning_blocks(func).contains(BlockId::ENTRY)
                && summaries.get(id).is_some_and(|summary| !summary.may_terminate())
            {
                reverting.insert(id);
            }
        }
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let mut eliminator = StorageStoreEliminator::new(
                    Rc::clone(analyses.alias()),
                    Rc::clone(analyses.cfg()),
                    &reverting,
                );
                let changed = eliminator.run_to_fixpoint(func) != 0;
                if eliminator.annotated_aliases {
                    analyses.note_unreported_edit();
                }
                changed
            },
        )
    }
}

/// All-path dead storage-store elimination pass.
struct StorageStoreEliminator<'a> {
    /// Number of storage stores eliminated.
    eliminated_count: usize,
    alias: Rc<AliasAnalysis>,
    /// Storage DSE removes instructions only, so one CFG snapshot serves every round.
    cfg: Rc<CfgInfo>,
    /// Functions whose every path reverts, so a tail call to one discards storage writes.
    reverting: &'a DenseBitSet<FunctionId>,
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

impl<'a> StorageStoreEliminator<'a> {
    /// Creates a new storage-store eliminator.
    fn new(
        alias: Rc<AliasAnalysis>,
        cfg: Rc<CfgInfo>,
        reverting: &'a DenseBitSet<FunctionId>,
    ) -> Self {
        Self { eliminated_count: 0, alias, cfg, reverting, annotated_aliases: false }
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.eliminated_count = 0;
        self.annotated_aliases |=
            func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);

        let cfg = Rc::clone(&self.cfg);
        let mut incoming = index_vec![Overwritten::none(); func.blocks.len()];
        loop {
            let mut changed = false;
            for &block in cfg.rpo().iter().rev() {
                let mut facts = self.successor_facts(func, &cfg, &incoming, block);
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
            let mut facts = self.successor_facts(func, &cfg, &incoming, block);
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
        &self,
        func: &Function,
        cfg: &CfgInfo,
        incoming: &IndexVec<BlockId, Overwritten>,
        block: BlockId,
    ) -> Overwritten {
        let Some(terminator) = &func.blocks[block].terminator else { return Overwritten::none() };
        if !terminator.has_successors() {
            return self.exit_facts(func, terminator);
        }
        let mut successors = cfg.successors(block).iter();
        let Some(&first) = successors.next() else { return Overwritten::none() };
        let mut facts = incoming[first].clone();
        for &successor in successors {
            facts.intersect(&incoming[successor]);
        }
        facts
    }

    /// The slots an exit discards before its terminator: every slot when it reverts, none
    /// when it can commit storage.
    fn exit_facts(&self, func: &Function, terminator: &Terminator) -> Overwritten {
        let reverts = match *terminator {
            Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid => true,
            Terminator::TailCall { function, .. } => self.reverting.contains(function),
            _ => false,
        };
        if !reverts {
            return Overwritten::none();
        }
        // A reverting callee can still read a slot into its revert data.
        let mut facts = Overwritten::All;
        let effects = self.alias.terminator_mod_ref(func, terminator);
        Self::apply_reverse_effects(&self.alias, &effects, &mut facts);
        facts
    }

    fn transfer_reverse(
        &self,
        func: &Function,
        block_id: BlockId,
        later_writes: &mut Overwritten,
        mut dead: Option<&mut DenseBitSet<InstId>>,
    ) {
        let aa = &*self.alias;
        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            if aa.instruction_may_terminate(func, inst_id) {
                later_writes.clear();
            }
            match &func.inst(inst_id).kind {
                InstKind::Store(slot, _, PtrLocation::Storage) => {
                    let alias = aa.storage_alias(func, inst_id, *slot);
                    if later_writes.contains(&alias) {
                        if let Some(dead) = &mut dead {
                            dead.insert(inst_id);
                        }
                        continue;
                    }

                    later_writes.remove_aliasing(aa, alias);
                    later_writes.insert(alias);
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
                later_writes.remove_based_on(value);
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
        let aa = &*self.alias;
        stored_values.clear();
        dead.clear();

        for &inst_id in &func.blocks[block_id].instructions {
            match &func.inst(inst_id).kind {
                InstKind::Store(slot, value, PtrLocation::Storage) => {
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

    fn remove_aliasing_map(
        aa: &AliasAnalysis,
        values: &mut FxHashMap<StorageAlias, ValueId>,
        alias: StorageAlias,
    ) {
        values.retain(|cached, _| {
            !aa.alias(Location::Storage(*cached), Location::Storage(alias)).may_alias()
        });
    }

    fn apply_reverse_effects(aa: &AliasAnalysis, effects: &ModRef, later_writes: &mut Overwritten) {
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
                later_writes.remove_aliasing(aa, alias);
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

/// Slots that every path from a program point overwrites or discards before anything observes
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Overwritten {
    /// Every slot: each path reverts, which discards every storage write.
    All,
    /// The slots a later store on every path overwrites.
    Slots(FxHashSet<StorageAlias>),
}

impl Overwritten {
    fn none() -> Self {
        Self::Slots(FxHashSet::default())
    }

    fn contains(&self, alias: &StorageAlias) -> bool {
        match self {
            Self::All => true,
            Self::Slots(slots) => slots.contains(alias),
        }
    }

    fn insert(&mut self, alias: StorageAlias) {
        if let Self::Slots(slots) = self {
            slots.insert(alias);
        }
    }

    fn clear(&mut self) {
        *self = Self::none();
    }

    fn intersect(&mut self, other: &Self) {
        match (&mut *self, other) {
            (_, Self::All) => {}
            (Self::All, Self::Slots(_)) => *self = other.clone(),
            (Self::Slots(slots), Self::Slots(other)) => slots.retain(|alias| other.contains(alias)),
        }
    }

    /// Drops every slot that may alias `alias`. Every slot but those cannot be named, so a read
    /// on a reverting path keeps none.
    fn remove_aliasing(&mut self, aa: &AliasAnalysis, alias: StorageAlias) {
        match self {
            Self::All => self.clear(),
            Self::Slots(slots) => slots.retain(|cached| {
                !aa.alias(Location::Storage(*cached), Location::Storage(alias)).may_alias()
            }),
        }
    }

    /// Drops the symbolic slots computed from `value`, which name a different slot on the next
    /// loop iteration. A revert discards that slot too, so every slot stays.
    fn remove_based_on(&mut self, value: ValueId) {
        if let Self::Slots(slots) = self {
            slots.retain(|alias| match *alias {
                StorageAlias::Slot(_) => true,
                StorageAlias::Symbolic(base) | StorageAlias::Offset { base, .. } => base != value,
            });
        }
    }
}
