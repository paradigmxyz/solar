//! Local storage-load forwarding.
//!
//! This pass removes redundant `sload` instructions on straight-line paths when
//! no intervening storage write may alias the loaded slot. Exact stores also
//! forward their full word to subsequent loads. This exposes packed field
//! updates as word expressions, allowing storage DSE to remove intermediate
//! writes while retaining every preserved bit. Possible aliases invalidate both
//! load and store facts. After a `gas` read, storage accesses remain explicit so
//! a later `gas` read observes their cost and warmness effects, including
//! observations in predecessor blocks and transitive callees.
//!
//! With `-Zdataflow-optimizations`, calls invalidate only what the experimental
//! dataflow framework's storage facts say they may
//! write. An internal call writes its callee's summarized footprint instantiated
//! for its storage-pointer arguments, which keeps loads of other fields, mapping
//! entries, and slots available across helpers that previously forgot every
//! symbolic slot. A static call cannot write state. A call whose target runs no
//! code that can call back, such as an active precompile, leaves this contract's
//! storage unchanged. Any other external call may re-enter and write arbitrary
//! slots, including through internal helpers and execution via delegatecall.
//! Created-code provenance is not used as an optimization proof. Paths compare
//! structurally under the storage layout assumptions of those facts.

use crate::mir::{
    BlockId, Callee, Function, InstId, InstKind, Module, StorageAlias, ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CfgInfo, GasObservations, Liveness, Location,
        dataflow::{facts::StorageFacts, reentrancy::call_operands, storage_path::PathId},
    },
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
    utils as mir_utils,
};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::{cell::OnceCell, rc::Rc, sync::Arc};

/// Function pass for straight-line storage-load CSE.
pub(crate) struct StorageLoadCse;

impl MirPass for StorageLoadCse {
    fn name(&self) -> &'static str {
        "storage-load-cse"
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
            if func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::SLoad(..))) {
                selected.insert(id);
                // Facts can only help a local cache when a load follows a storage access
                // and an intervening call in the same block.
                if gcx.sess.opts.unstable.dataflow_optimizations
                    && func.blocks.iter().any(|block| {
                        let mut cached = false;
                        let mut crossed_call = false;
                        for &inst in &block.instructions {
                            match &func.inst(inst).kind {
                                InstKind::SLoad(_) if crossed_call => return true,
                                InstKind::SLoad(_) | InstKind::SStore(..) => cached = true,
                                kind if cached && is_call(kind) => crossed_call = true,
                                _ => {}
                            }
                        }
                        false
                    })
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
                let mut cse = StorageLoadCseCx::new();
                cse.facts = facts.clone();
                cse.alias = Some(Rc::clone(analyses.alias()));
                let changed = cse.run_to_fixpoint(func) != 0;
                if cse.annotated_aliases {
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

/// Local storage load CSE pass.
#[derive(Debug, Default)]
struct StorageLoadCseCx {
    /// Number of storage loads eliminated.
    eliminated_count: usize,
    alias: Option<Rc<AliasAnalysis>>,
    /// Storage footprints of calls, computed for the module at pass entry.
    facts: Option<Arc<StorageFacts>>,
    /// Whether storage-alias annotation changed metadata, which is not reported as a change.
    annotated_aliases: bool,
}

struct RunState {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
    cached_loads: FxHashMap<StorageAlias, (ValueId, bool)>,
    /// Storage paths of each cached slot, when the facts describe its access.
    cached_paths: FxHashMap<StorageAlias, SmallVec<[PathId; 2]>>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self {
            replacements: FxHashMap::default(),
            dead: DenseBitSet::new_empty(func.num_insts()),
            cached_loads: FxHashMap::default(),
            cached_paths: FxHashMap::default(),
        }
    }

    fn remember_paths(
        &mut self,
        facts: Option<&StorageFacts>,
        func: &Function,
        alias: StorageAlias,
        inst: InstId,
    ) {
        match facts.and_then(|facts| facts.slot_paths(func, inst)) {
            Some(paths) => {
                self.cached_paths.insert(alias, paths.iter().copied().collect());
            }
            None => {
                self.cached_paths.remove(&alias);
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
        self.annotated_aliases |=
            func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }

        // Liveness is only needed when a load meets an earlier load of the same slot.
        let liveness = OnceCell::new();
        state.replacements.clear();
        state.dead.clear();

        let gas = GasObservations::new(func, &CfgInfo::new(func), self.alias.as_ref().unwrap());
        for block_id in func.blocks.indices() {
            state.cached_loads.clear();
            state.cached_paths.clear();
            self.process_block(func, block_id, &liveness, &gas, state);
        }

        if !state.replacements.is_empty() {
            Self::replace_uses(func, &state.replacements);
        }
        if !state.dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !state.dead.contains(id));
            }
        }

        self.eliminated_count
    }

    /// Runs storage-load CSE to a fixed point.
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

    fn process_block(
        &mut self,
        func: &Function,
        block_id: BlockId,
        liveness: &OnceCell<Liveness>,
        gas: &GasObservations,
        state: &mut RunState,
    ) {
        let aa = self.alias.as_ref().expect("storage-load CSE alias snapshot is initialized");
        let mut gas_observed = gas.at_entry(block_id);
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
                    if gas_observed {
                        continue;
                    }
                    if let Some(&(cached, from_store)) = state.cached_loads.get(&alias) {
                        if !from_store
                            && !liveness
                                .get_or_init(|| Liveness::compute(func))
                                .is_used_at_or_after(cached, block_id, inst_idx)
                        {
                            state.cached_loads.insert(alias, (result, false));
                            state.remember_paths(self.facts.as_deref(), func, alias, inst_id);
                            continue;
                        }
                        state.replacements.insert(result, cached);
                        state.dead.insert(inst_id);
                        self.eliminated_count += 1;
                    } else {
                        state.cached_loads.insert(alias, (result, false));
                        state.remember_paths(self.facts.as_deref(), func, alias, inst_id);
                    }
                }
                InstKind::SStore(slot, value) => {
                    let alias = aa.storage_alias_after_replacements(
                        func,
                        inst_id,
                        *slot,
                        &state.replacements,
                    );
                    state.cached_loads.retain(|cached_alias, _| {
                        !aa.alias(Location::Storage(*cached_alias), Location::Storage(alias))
                            .may_alias()
                    });
                    if gas_observed {
                        continue;
                    }
                    // sstore slot, value; result = sload slot => result = value
                    let value = mir_utils::resolve_replacement(*value, &state.replacements);
                    state.cached_loads.insert(alias, (value, true));
                    state.remember_paths(self.facts.as_deref(), func, alias, inst_id);
                }
                _ => {
                    let effects = aa.instruction_mod_ref_with_replacements(
                        func,
                        inst_id,
                        &state.replacements,
                    );
                    if gas.observes(inst_id) {
                        state.cached_loads.clear();
                        gas_observed = true;
                        continue;
                    }
                    // A call keeps the loads its storage footprint cannot write.
                    if is_call(&func.inst(inst_id).kind)
                        && let Some(facts) = self.facts.as_deref()
                        && let Some(footprint) = facts.footprint(func, inst_id)
                    {
                        let cached_paths = &state.cached_paths;
                        state.cached_loads.retain(|alias, _| {
                            cached_paths.get(alias).is_some_and(|paths| {
                                !facts.may_touch(paths, footprint.writes, footprint.reentrant)
                            })
                        });
                        continue;
                    }
                    for &access in effects.writes() {
                        match access {
                            Access::Any(AddressSpace::Storage) => {
                                state.cached_loads.clear();
                                break;
                            }
                            Access::Location(Location::Storage(alias)) => {
                                state.cached_loads.retain(|cached_alias, _| {
                                    !aa.alias(
                                        Location::Storage(*cached_alias),
                                        Location::Storage(alias),
                                    )
                                    .may_alias()
                                });
                            }
                            Access::Any(
                                AddressSpace::Memory
                                | AddressSpace::Transient
                                | AddressSpace::Immutable,
                            )
                            | Access::Location(
                                Location::Memory(_)
                                | Location::Transient(_)
                                | Location::Immutable(_),
                            ) => {}
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
            mir_utils::replace_inst_uses_canonicalized(inst, replacements);
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
