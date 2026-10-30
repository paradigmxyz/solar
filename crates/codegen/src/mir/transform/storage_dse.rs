//! All-path dead storage-store elimination.
//!
//! This pass removes persistent `sstore` and transient `tstore` instructions inside a single basic
//! block or across CFG edges when a later store to the same definitely-known slot overwrites them
//! before any storage observer can see the intermediate value. It also removes
//! stores of the word their slot already holds: a repeated equal store, or a store of the word an
//! `sload` of the slot read, when no intervening instruction can clobber the slot.
//!
//! A backward must analysis intersects the definitely-overwritten slots of all
//! successors. An exit that returns or stops starts with no slots, and a least
//! fixed point prevents a loop from justifying its own dead stores. An exit that
//! reverts, directly or through a tail call to a function that never returns,
//! starts with every slot: the revert discards every storage and transient-storage write, so a
//! store that only reaches it is dead unless something reads the slot first. A return or stop
//! commits persistent writes and leaves transient writes readable by a re-entrant `tload`, so
//! neither kind is dead on those exits. Each space keeps its own facts, so an access to one
//! never kills a fact about the other. Reads, possible aliases and unknown call effects kill
//! facts. Only after convergence are stores erased; the equal-store walk then runs with the
//! same alias barriers. Run after storage forwarding so packed read-modify-write chains expose
//! their overwritten stores without discarding preserved fields. A `gas` read clears both
//! forward and backward facts so measured storage writes remain explicit.
//! Calls that may terminate the current EVM call clear backward facts because
//! they can commit earlier writes without reaching the later overwrite.
//!
//! The equal-store walk is forward. A store, or an `sload`, tells the word its slot holds until a
//! write that may alias the slot, a call that may write its space, or a possible `gas`
//! observation, and a store of that word is removed. A read-modify-write that changes no bit,
//! such as `x = sload s; sstore s, x ^ ((x ^ v) & 0)` once the value simplifies to `x`, thus
//! loses its store, and DCE then removes the unused load. Facts flow into a block entered from
//! one predecessor, whose terminator touches no storage, so a check that lowering splits off
//! between the load and the store keeps the pair visible; every other block starts with none.
//! After a possible `gas` observation, which `GasObservations` tracks through every path and
//! transitive callee, accesses stay explicit, as in `storage-cse`.
//!
//! Removing a store of the word its slot holds changes behavior only where the store itself
//! would fail: in a static context, where it reverts, and, for an `SSTORE`, with at most 2,300
//! gas left. solc's optimizer removes the same `SSTORE`s, in its legacy and its IR pipeline, and
//! accepts the same difference.

use crate::mir::{
    BlockId, Function, FunctionId, InstId, InstKind, Module, PtrLocation, StorageAlias, Terminator,
    ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CfgInfo, GasObservations, Location, ModRef,
        returning_blocks,
    },
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
        for (id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst| {
                matches!(
                    func.inst(inst).kind,
                    InstKind::Store(.., PtrLocation::Storage | PtrLocation::Transient)
                )
            }) {
                selected.insert(id);
            }
        }
        if selected.is_empty() {
            return false;
        }
        let mut reverting = DenseBitSet::new_empty(module.functions.len());
        let summaries = analyses.call_summaries(module);
        for (id, func) in module.functions.iter_enumerated() {
            // A callee that can halt successfully, itself or through its own calls, commits
            // storage even when it never returns.
            if !func.blocks.is_empty()
                && summaries.get(id).is_some_and(|summary| !summary.may_terminate())
                && !returning_blocks(func).contains(BlockId::ENTRY)
            {
                reverting.insert(id);
            }
        }
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let eliminator = StorageStoreEliminator {
                    alias: Rc::clone(analyses.alias()),
                    cfg: Rc::clone(analyses.cfg()),
                    reverting: &reverting,
                };
                // Annotating changes metadata only, which is not reported as a change.
                if func.annotate_storage_aliases(mir_utils::StorageAliasScope::StorageAndTransient)
                {
                    analyses.note_unreported_edit();
                }
                eliminator.run(func)
            },
        )
    }
}

/// All-path dead storage-store elimination pass.
struct StorageStoreEliminator<'a> {
    alias: Rc<AliasAnalysis>,
    /// Storage DSE removes instructions only, so one CFG snapshot serves both phases.
    cfg: Rc<CfgInfo>,
    /// Functions whose every path reverts, so a tail call to one discards storage writes.
    reverting: &'a DenseBitSet<FunctionId>,
}

/// The word each slot is known to hold, from a store or a load of it.
type StoredValues = FxHashMap<Location, ValueId>;

impl StorageStoreEliminator<'_> {
    /// Removes dead stores, then equal stores. Returns whether it removed any.
    fn run(&self, func: &mut Function) -> bool {
        let cfg = &*self.cfg;
        let mut incoming = index_vec![Overwritten::none(); func.blocks.len()];
        loop {
            let mut changed = false;
            for &block in cfg.rpo().iter().rev() {
                let mut facts = self.successor_facts(func, &incoming, block);
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
        let mut dead = DenseBitSet::new_empty(func.num_insts());
        for &block in cfg.rpo() {
            let mut facts = self.successor_facts(func, &incoming, block);
            self.transfer_reverse(func, block, &mut facts, Some(&mut dead));
        }
        // sstore slot, old; ...; sstore slot, new => ...; sstore slot, new
        let removed_dead = Self::remove_dead(func, &dead);
        self.remove_equal_stores(func, &mut dead) | removed_dead
    }

    /// Removes the `dead` instructions. Returns whether there were any.
    fn remove_dead(func: &mut Function, dead: &DenseBitSet<InstId>) -> bool {
        if dead.is_empty() {
            return false;
        }
        for block in &mut func.blocks {
            block.instructions.retain(|id| !dead.contains(*id));
        }
        true
    }

    fn successor_facts(
        &self,
        func: &Function,
        incoming: &IndexVec<BlockId, Overwritten>,
        block: BlockId,
    ) -> Overwritten {
        let Some(terminator) = &func.blocks[block].terminator else { return Overwritten::none() };
        let Some((&first, rest)) = self.cfg.successors(block).split_first() else {
            return self.exit_facts(func, terminator);
        };
        let mut facts = incoming[first].clone();
        for &successor in rest {
            facts.intersect(&incoming[successor]);
        }
        facts
    }

    /// The slots an exit discards before its terminator: every slot when it reverts, none
    /// when it can commit storage. A return or stop keeps transient writes too, because a
    /// re-entrant call can still read them.
    fn exit_facts(&self, func: &Function, terminator: &Terminator) -> Overwritten {
        let reverts = terminator.reverts()
            || matches!(*terminator, Terminator::TailCall { function, .. }
                if self.reverting.contains(function));
        if !reverts {
            return Overwritten::none();
        }
        // A reverting callee can still read a slot into its revert data.
        let mut facts = Overwritten::all();
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
                &InstKind::Store(
                    slot,
                    _,
                    location @ (PtrLocation::Storage | PtrLocation::Transient),
                ) => {
                    let written = Self::slot_location(func, inst_id, slot, location);
                    let facts = later_writes.facts_mut(written.address_space());
                    if facts.contains(&written) {
                        if let Some(dead) = &mut dead {
                            dead.insert(inst_id);
                        }
                        continue;
                    }

                    facts.remove_aliasing(aa, written);
                    facts.insert(written);
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

    /// Removes the stores of a word their slot already holds, as an equal store or a load of
    /// the slot left it. A block entered from one predecessor starts with the words that
    /// predecessor ends with, since no terminator that continues to a successor touches
    /// storage, so a check that lowering splits off between a load and its store keeps the pair
    /// visible; every other block starts with none.
    fn remove_equal_stores(&self, func: &mut Function, dead: &mut DenseBitSet<InstId>) -> bool {
        let cfg = &*self.cfg;
        let gas = GasObservations::new(func, cfg, &self.alias);
        dead.clear();
        // Each exit is kept with the number of successors still waiting for it, and the last one
        // takes it.
        let mut exits = FxHashMap::<BlockId, (usize, StoredValues)>::default();
        for &block in cfg.rpo() {
            let mut stored_values = match cfg.predecessors(block) {
                &[pred] => match exits.get_mut(&pred) {
                    Some((1, _)) => exits.remove(&pred).unwrap().1,
                    Some((waiting, stored_values)) => {
                        *waiting -= 1;
                        stored_values.clone()
                    }
                    None => StoredValues::default(),
                },
                _ => StoredValues::default(),
            };
            self.remove_block_equal_stores(func, block, &gas, &mut stored_values, dead);
            let waiting = cfg
                .successors(block)
                .iter()
                .filter(|&&successor| cfg.predecessors(successor).len() == 1)
                .count();
            if waiting != 0 {
                exits.insert(block, (waiting, stored_values));
            }
        }

        Self::remove_dead(func, dead)
    }

    fn remove_block_equal_stores(
        &self,
        func: &Function,
        block_id: BlockId,
        gas: &GasObservations,
        stored_values: &mut StoredValues,
        dead: &mut DenseBitSet<InstId>,
    ) {
        let aa = &*self.alias;
        // After a gas read, storage accesses stay, so a later read observes their cost.
        if gas.at_entry(block_id) {
            stored_values.clear();
            return;
        }
        for &inst_id in &func.blocks[block_id].instructions {
            if gas.observes(inst_id) {
                stored_values.clear();
                return;
            }
            match func.inst(inst_id).kind {
                InstKind::Store(
                    slot,
                    value,
                    location @ (PtrLocation::Storage | PtrLocation::Transient),
                ) => {
                    let location = Self::slot_location(func, inst_id, slot, location);
                    // sstore slot, v; ...; sstore slot, v -> sstore slot, v; ...
                    // v = sload slot; ...; sstore slot, v -> v = sload slot; ...
                    if stored_values.get(&location).is_some_and(|&stored| stored == value) {
                        dead.insert(inst_id);
                        continue;
                    }

                    Self::remove_aliasing_map(aa, stored_values, location);
                    stored_values.insert(location, value);
                }
                // A load tells the word its slot holds until a write that may alias the slot.
                InstKind::Load(slot, PtrLocation::Storage) => {
                    if let Some(loaded) = func.inst_result_value(inst_id) {
                        let location = Location::Storage(func.storage_alias(inst_id, slot));
                        stored_values.entry(location).or_insert(loaded);
                    }
                }
                _ => {
                    let effects = aa.instruction_mod_ref(func, inst_id);
                    Self::apply_forward_writes(aa, &effects, stored_values);
                }
            }
        }
    }

    /// The slot a storage or transient store at `slot` writes.
    fn slot_location(
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        location: PtrLocation,
    ) -> Location {
        Location::slot(location, func.storage_alias(inst_id, slot))
            .expect("a storage or transient store")
    }

    fn remove_aliasing_map(aa: &AliasAnalysis, values: &mut StoredValues, location: Location) {
        values.retain(|cached, _| !aa.alias(*cached, location).may_alias());
    }

    fn apply_reverse_effects(aa: &AliasAnalysis, effects: &ModRef, later_writes: &mut Overwritten) {
        if effects.observes_gas() {
            later_writes.clear();
            return;
        }

        for space in AddressSpace::SLOTS {
            if effects.reads_anywhere(space) || effects.writes_anywhere(space) {
                later_writes.facts_mut(space).clear();
                continue;
            }

            // ModRef describes possible writes, not definite overwrites. Only
            // an explicit store can establish a later write on every path.
            for &access in effects.reads() {
                if let Access::Location(location) = access
                    && location.address_space() == space
                {
                    later_writes.facts_mut(space).remove_aliasing(aa, location);
                }
            }
        }
    }

    fn apply_forward_writes(
        aa: &AliasAnalysis,
        effects: &ModRef,
        stored_values: &mut StoredValues,
    ) {
        if effects.observes_gas() {
            stored_values.clear();
            return;
        }
        for &access in effects.writes() {
            match access {
                Access::Any(space) => {
                    stored_values.retain(|location, _| location.address_space() != space);
                }
                Access::Location(location) => {
                    Self::remove_aliasing_map(aa, stored_values, location)
                }
            }
        }
    }
}

/// Slots that every path from a program point overwrites or discards before anything observes
/// them, per slot space: persistent storage, which a revert discards and a return commits, and
/// transient storage, which a revert discards and a return keeps readable to later calls of the
/// transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Overwritten {
    slots: [Slots; AddressSpace::SLOTS.len()],
}

impl Overwritten {
    fn none() -> Self {
        Self { slots: AddressSpace::SLOTS.map(|_| Slots::none()) }
    }

    /// Every slot of every space: each path reverts.
    fn all() -> Self {
        Self { slots: AddressSpace::SLOTS.map(|_| Slots::All) }
    }

    fn facts_mut(&mut self, space: AddressSpace) -> &mut Slots {
        &mut self.slots[space.slot_index().expect("a slot space")]
    }

    fn clear(&mut self) {
        self.slots.iter_mut().for_each(Slots::clear);
    }

    fn intersect(&mut self, other: &Self) {
        for (slots, other) in self.slots.iter_mut().zip(&other.slots) {
            slots.intersect(other);
        }
    }

    /// Drops the symbolic slots computed from `value`.
    fn remove_based_on(&mut self, value: ValueId) {
        for slots in &mut self.slots {
            slots.remove_based_on(value);
        }
    }
}

/// The overwritten slots of one space.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Slots {
    /// Every slot: each path reverts, which discards every write.
    All,
    /// The slots a later store on every path overwrites.
    Set(FxHashSet<Location>),
}

impl Slots {
    fn none() -> Self {
        Self::Set(FxHashSet::default())
    }

    fn contains(&self, slot: &Location) -> bool {
        match self {
            Self::All => true,
            Self::Set(slots) => slots.contains(slot),
        }
    }

    fn insert(&mut self, slot: Location) {
        if let Self::Set(slots) = self {
            slots.insert(slot);
        }
    }

    fn clear(&mut self) {
        match self {
            Self::All => *self = Self::none(),
            Self::Set(slots) => slots.clear(),
        }
    }

    fn intersect(&mut self, other: &Self) {
        match (&mut *self, other) {
            (_, Self::All) => {}
            (Self::All, Self::Set(_)) => *self = other.clone(),
            (Self::Set(slots), Self::Set(other)) => slots.retain(|slot| other.contains(slot)),
        }
    }

    /// Drops every slot that may alias `written`. Every slot but those cannot be named, so a read
    /// on a reverting path keeps none.
    fn remove_aliasing(&mut self, aa: &AliasAnalysis, written: Location) {
        match self {
            Self::All => self.clear(),
            Self::Set(slots) => slots.retain(|&cached| !aa.alias(cached, written).may_alias()),
        }
    }

    /// Drops the symbolic slots computed from `value`, which name a different slot on the next
    /// loop iteration. A revert discards that slot too, so every slot stays.
    fn remove_based_on(&mut self, value: ValueId) {
        if let Self::Set(slots) = self {
            slots.retain(|slot| match slot.slot_alias() {
                Some(StorageAlias::Symbolic(base) | StorageAlias::Offset { base, .. }) => {
                    base != value
                }
                _ => true,
            });
        }
    }
}
