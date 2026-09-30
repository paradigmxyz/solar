//! Remove reentrancy guards that nothing can observe.
//!
//! A `nonReentrant` lock stores a marker in a fixed slot before the guarded body and
//! restores the slot's previous value afterwards. The marker only matters to code that
//! runs while it is set: a reentrant call, or a later read of the slot inside the body.
//! When no instruction between the set and the restore can observe the slot, both stores
//! are unobservable: every path either restores the original value or reverts, which
//! discards the write. The pass then deletes the set and every restore, and replaces reads
//! of the slot inside the region with the stored marker value. The guard's check stays,
//! so the function still reverts when entered while another guarded region holds the lock.
//! On the EVM this saves the dirty write and the restore; the refund for restoring the
//! original value otherwise recovers most of their cost.
//!
//! The analysis runs in three steps:
//!
//! - A forward pass over the path-sensitive exact-slot domain of the dataflow framework computes
//!   each slot's word at every store, relative to function entry. Guard checks such as
//!   `require(unlocked == 1)` fix the slot's value; packed flags keep their other bits as copies of
//!   the entry value, so a read-modify-write restore still matches.
//! - For each store whose word differs from the slot's current word, a walk over the region after
//!   it collects restores, stores of exactly the original word, and reads of the slot. The walk
//!   rejects any other store or possible read of the slot, including through an internal call's
//!   summarized footprint; any external call whose target may run code that calls back, since a
//!   reentrant call could read the lock; static calls, whose reentrant views may read it; normal
//!   exits that do not restore; and loops back to the set. Region blocks must be dominated by the
//!   set and entered only from inside the region, so a read after a restore on another path is
//!   never rewritten.
//! - Functions that observe gas are skipped, like the other storage eliminations.
//!
//! Only guards whose calls run no code that can call back qualify: calls to precompiles,
//! internal helpers, or contracts created here without call instructions. Run in gas mode
//! after storage forwarding and dead-store elimination, when packed read-modify-write
//! sequences are explicit.

use crate::mir::{
    BlockId, Function, InstId, InstKind, Module, Terminator, ValueId,
    analysis::{
        CfgInfo, GasObservations,
        dataflow::{
            engine::{self, Analysis, Edge, EdgeCondition},
            facts::StorageFacts,
            lattice::Reachable,
            slot_state::{Clobber, SlotKey, SlotState, SymWord, Val},
            storage_path::PathId,
        },
    },
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};
use std::sync::Arc;

/// Function pass that removes unobservable set/restore pairs of exact storage slots.
pub(crate) struct GuardElim;

impl MirPass for GuardElim {
    fn name(&self) -> &'static str {
        "guard-elim"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            let stores = func
                .instructions()
                .filter(|&inst| {
                    matches!(func.inst(inst).kind, InstKind::SStore(..) | InstKind::TStore(..))
                })
                .count();
            if stores >= 2 {
                selected.insert(id);
            }
        }
        if selected.is_empty() {
            return false;
        }
        let facts = StorageFacts::compute(module, gcx.sess.opts.evm_version);
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let gas = GasObservations::new(func, analyses.cfg(), analyses.alias());
                if func.instructions().any(|inst| gas.observes(inst)) {
                    return false;
                }
                eliminate(func, analyses.cfg(), &facts)
            },
        )
    }
}

/// Returns the exact slot a load or store accesses.
fn exact_slot(facts: &StorageFacts, func: &Function, inst: InstId) -> Option<SlotKey> {
    let (paths, transient) = facts.slot_access(func, inst)?;
    match paths {
        &[path] => facts.exact_slot(path).map(|slot| SlotKey { transient, slot }),
        _ => None,
    }
}

/// Forward transfer over exact-slot words.
struct Transfer<'a> {
    facts: &'a StorageFacts,
}

impl Transfer<'_> {
    /// Applies possible writes to `paths`.
    fn clobber(&self, paths: Option<&[PathId]>, state: &mut SlotState) {
        let Some(paths) = paths else {
            state.clobber(Clobber::All);
            return;
        };
        for &path in paths {
            match self.facts.exact_slot(path) {
                Some(slot) => {
                    for transient in [false, true] {
                        state.write(SlotKey { transient, slot }, SymWord::UNKNOWN);
                    }
                }
                None if self.facts.is_hashed(path) => {}
                None => state.clobber(Clobber::All),
            }
        }
    }
}

impl Analysis for Transfer<'_> {
    type Domain = Reachable<SlotState>;

    fn bottom(&self, _func: &Function) -> Self::Domain {
        Reachable::Unreachable
    }

    fn initialize_boundary(&mut self, _func: &Function, _block: BlockId, state: &mut Self::Domain) {
        *state = Reachable::State(SlotState::default());
    }

    fn apply_instruction(
        &mut self,
        func: &Function,
        _block: BlockId,
        inst: InstId,
        state: &mut Self::Domain,
    ) {
        let Reachable::State(current) = state else { return };
        let kind = &func.inst(inst).kind;
        let loaded = match kind {
            InstKind::SLoad(_) | InstKind::TLoad(_) => exact_slot(self.facts, func, inst),
            _ => None,
        };
        if let Some(fact) = current.evaluate(func, inst, loaded)
            && let Some(result) = func.inst_result_value(inst)
        {
            current.values.insert(result, fact);
        }
        match *kind {
            InstKind::SStore(_, stored) | InstKind::TStore(_, stored) => {
                match exact_slot(self.facts, func, inst) {
                    Some(slot) => {
                        let word = current
                            .value(func, stored)
                            .and_then(Val::word)
                            .unwrap_or(SymWord::UNKNOWN);
                        current.write(slot, word);
                    }
                    None => {
                        let paths = self.facts.slot_access(func, inst).map(|(paths, _)| paths);
                        self.clobber(paths, current);
                    }
                }
            }
            InstKind::SLoad(_) | InstKind::TLoad(_) => {}
            _ => {
                if let Some(footprint) = self.facts.footprint(func, inst) {
                    self.clobber(footprint.writes, current);
                }
                if let Some(pred) = current.check_assumption(func, kind)
                    && !current.assume(pred)
                {
                    *state = Reachable::Unreachable;
                }
            }
        }
    }

    fn apply_edge(&mut self, func: &Function, edge: &Edge, state: &mut Self::Domain) {
        let Reachable::State(current) = state else { return };
        if let EdgeCondition::Branch { condition, taken } = edge.condition
            && let Some(pred) = current.condition(func, condition)
            && !current.assume(if taken { pred } else { pred.negate() })
        {
            *state = Reachable::Unreachable;
        }
    }

    fn apply_phi(
        &mut self,
        func: &Function,
        phi: InstId,
        incoming: ValueId,
        _edge: &Edge,
        state: &mut Self::Domain,
    ) {
        let Reachable::State(current) = state else { return };
        if let (Some(result), Some(value)) =
            (func.inst_result_value(phi), current.value(func, incoming))
        {
            current.values.insert(result, value);
        }
    }
}

/// A store's slot, the slot's word just before it, and the word it stores.
#[derive(Clone, Copy)]
struct StoreFact {
    slot: SlotKey,
    before: SymWord,
    stored: SymWord,
}

/// The accesses of the slot between a set and its restores.
struct Region {
    restores: Vec<InstId>,
    reads: Vec<InstId>,
}

fn eliminate(func: &mut Function, cfg: &CfgInfo, facts: &Arc<StorageFacts>) -> bool {
    let mut transfer = Transfer { facts };
    let results = engine::solve(func, cfg, &mut transfer);
    let mut stores = FxHashMap::default();
    engine::replay(func, cfg, &mut transfer, &results, |transfer, point, state| {
        let engine::ProgramPoint::Instruction(_, inst) = point else { return };
        let Reachable::State(state) = state else { return };
        if let InstKind::SStore(_, stored) | InstKind::TStore(_, stored) = func.inst(inst).kind
            && let Some(slot) = exact_slot(transfer.facts, func, inst)
        {
            let stored = state.value(func, stored).and_then(Val::word).unwrap_or(SymWord::UNKNOWN);
            stores.insert(inst, StoreFact { slot, before: state.current(slot), stored });
        }
    });

    let blocks = func.inst_block_table();
    let mut removed = FxHashSet::default();
    let mut replacements = FxHashMap::default();
    let candidates =
        func.instructions().filter(|inst| stores.contains_key(inst)).collect::<Vec<_>>();
    for set in candidates {
        let fact = stores[&set];
        if removed.contains(&set) || !fact.before.is_determined() || fact.stored == fact.before {
            continue;
        }
        let Some(block) = blocks[set] else { continue };
        let Some(region) = region_after(func, cfg, facts, &stores, &removed, set, block, fact)
        else {
            continue;
        };
        // sstore slot, marker; ...; v = sload slot; ...; sstore slot, original
        // => ...; v = marker; ...
        let (InstKind::SStore(_, marker) | InstKind::TStore(_, marker)) = func.inst(set).kind
        else {
            continue;
        };
        removed.insert(set);
        removed.extend(region.restores);
        for read in region.reads {
            if let Some(result) = func.inst_result_value(read) {
                replacements.insert(result, marker);
                removed.insert(read);
            }
        }
    }
    if removed.is_empty() {
        return false;
    }
    for block in func.blocks.iter_mut() {
        block.instructions.retain(|inst| !removed.contains(inst));
    }
    func.replace_uses(&replacements);
    true
}

/// Walks the region after `set`, returning its restores and reads if nothing else can
/// observe the slot there.
#[allow(clippy::too_many_arguments)]
fn region_after(
    func: &Function,
    cfg: &CfgInfo,
    facts: &StorageFacts,
    stores: &FxHashMap<InstId, StoreFact>,
    removed: &FxHashSet<InstId>,
    set: InstId,
    set_block: BlockId,
    fact: StoreFact,
) -> Option<Region> {
    let slot_path = facts.slot_access(func, set)?.0.first().copied()?;
    let touches =
        |paths: Option<&[PathId]>, reentrant: bool| facts.may_touch(&[slot_path], paths, reentrant);
    let dominators = cfg.dominators();
    let start = func.blocks[set_block].instructions.iter().position(|&inst| inst == set)? + 1;
    let mut region = Region { restores: Vec::new(), reads: Vec::new() };
    let mut worklist = vec![(set_block, start)];
    let mut visited = FxHashSet::default();
    let mut exits_in_region = FxHashSet::default();
    'blocks: while let Some((block, start)) = worklist.pop() {
        for &inst in &func.blocks[block].instructions[start..] {
            if removed.contains(&inst) {
                return None;
            }
            match func.inst(inst).kind {
                InstKind::SStore(..) | InstKind::TStore(..) => {
                    let (paths, _) = facts.slot_access(func, inst)?;
                    if !touches(Some(paths), false) {
                        continue;
                    }
                    let restore = stores.get(&inst).is_some_and(|store| {
                        store.slot == fact.slot
                            && store.stored.is_determined()
                            && store.stored == fact.before
                    });
                    if !restore {
                        return None;
                    }
                    region.restores.push(inst);
                    continue 'blocks;
                }
                InstKind::SLoad(_) | InstKind::TLoad(_) => {
                    let (paths, _) = facts.slot_access(func, inst)?;
                    if !touches(Some(paths), false) {
                        continue;
                    }
                    if exact_slot(facts, func, inst) != Some(fact.slot) {
                        return None;
                    }
                    region.reads.push(inst);
                }
                _ => {
                    let footprint = facts.footprint(func, inst)?;
                    if touches(footprint.reads, footprint.reentrant)
                        || touches(footprint.writes, footprint.reentrant)
                        || footprint.terminates
                    {
                        return None;
                    }
                }
            }
        }
        exits_in_region.insert(block);
        match &func.blocks[block].terminator {
            Some(
                Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
            ) => {}
            Some(Terminator::Jump(_) | Terminator::Branch { .. } | Terminator::Switch { .. }) => {
                for &successor in cfg.successors(block) {
                    if successor == set_block || !dominators.dominates(set_block, successor) {
                        return None;
                    }
                    if visited.insert(successor) {
                        worklist.push((successor, 0));
                    }
                }
            }
            // A normal exit would leave the marker in place.
            _ => return None,
        }
    }
    // Every entry into the region must come from inside it, before any restore.
    for &block in &visited {
        if func.blocks[block].predecessors.iter().any(|pred| !exits_in_region.contains(pred)) {
            return None;
        }
    }
    (!region.restores.is_empty()).then_some(region)
}
