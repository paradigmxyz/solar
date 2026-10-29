//! Free-memory-pointer reads that return the entry's initial pointer.
//!
//! A runtime external entry starts with the free memory pointer at its initial value, the heap
//! floor codegen computes once it has placed the frames. A read is fresh when no instruction on
//! any path from the start of the entry to the read may write the pointer's slot, so it returns
//! that initial value. The pass that lowers abstract allocation reads the floor for such a read
//! instead of the slot, and an entry whose slot reads are all fresh needs no initialization.
//!
//! A function is entered fresh when it is a runtime external entry that no function but the
//! dispatch entry calls, or a non-recursive internal function whose every call site, tail calls
//! included, is a fresh point of a function entered fresh. A call site is fresh when no
//! instruction before it may write the slot, the same test as for a read. The constructor, the
//! dispatch entry, and recursive functions are never entered fresh: the constructor lays its
//! arguments out below the pointer, the dispatch entry reaches every entry, and a recursive
//! function runs again after its own allocations.
//!
//! An instruction may write the slot when it allocates, moves the pointer, writes memory through
//! an address that may lie below the heap or covers the slot, or calls a function whose memory
//! summary may do any of these. Stores and copies through a pointer derived from the free memory
//! pointer or an allocation lie at or above the heap floor and cannot reach the slot, also when a
//! constant offset moves them: no write has changed the pointer yet, so it is the entry's initial
//! one, and a constant below `2^64` added to it cannot wrap. Everything else is judged by its
//! `ModRef` effects.
//!
//! The analysis does not see raw loads of the slot: the compiler reads the pointer only through
//! `fmp` and `alloc`, and memory-safe assembly that loads the slot keeps its read.

use super::{AliasAnalysis, CallGraphInfo, CfgInfo, Location, MemoryCallSummaries};
use crate::mir::{
    AllocationFailure, BlockId, Callee, EffectKind, Function, FunctionId, InstId, InstKind, Module,
    PtrLocation, Terminator, Value, ValueId,
};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashSet,
};
use std::sync::Arc;

/// The `fmp` and `alloc` instructions of a module that read the entry's initial pointer.
pub(crate) struct FreshFmpReads {
    reads: IndexVec<FunctionId, FxHashSet<InstId>>,
}

impl FreshFmpReads {
    /// Finds the fresh reads of every optimized function of `module`, those of checked allocations
    /// too in a function optimized for gas.
    #[must_use]
    pub(crate) fn new(
        module: &Module,
        summaries: impl FnOnce() -> Arc<MemoryCallSummaries>,
    ) -> Self {
        let wanted = |func: &Function| {
            let objective = module.objective_of(func);
            !objective.is_none() && has_read(func, objective.is_gas())
        };
        if !module.functions.iter().any(wanted) {
            return Self::none();
        }
        let summaries = summaries();
        let mut analysis = Analysis::new(module, &summaries);
        let reads = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| {
                if wanted(func) && analysis.entered_fresh(id) {
                    let freshness = analysis.freshness(id);
                    let checked = module.objective_of(func).is_gas();
                    func.instructions()
                        .filter(|&inst| {
                            is_read(func, inst, checked) && freshness.insts.contains(&inst)
                        })
                        .collect()
                } else {
                    FxHashSet::default()
                }
            })
            .collect();
        Self { reads }
    }

    /// No read is fresh.
    #[must_use]
    pub(crate) fn none() -> Self {
        Self { reads: IndexVec::new() }
    }

    /// Whether `inst` of `func` reads the entry's initial pointer.
    #[must_use]
    pub(crate) fn contains(&self, func: FunctionId, inst: InstId) -> bool {
        self.reads.get(func).is_some_and(|reads| reads.contains(&inst))
    }
}

/// A call to a function: an `icall` or the tail call ending a block.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CallSite {
    Inst(FunctionId, InstId),
    Tail(FunctionId, BlockId),
}

impl CallSite {
    const fn caller(self) -> FunctionId {
        match self {
            Self::Inst(caller, _) | Self::Tail(caller, _) => caller,
        }
    }
}

/// The reads and call sites of a function that no write precedes, when the function is entered
/// fresh.
#[derive(Clone, Default)]
struct Freshness {
    insts: FxHashSet<InstId>,
    tail_calls: FxHashSet<BlockId>,
}

impl Freshness {
    fn contains(&self, site: CallSite) -> bool {
        match site {
            CallSite::Inst(_, inst) => self.insts.contains(&inst),
            CallSite::Tail(_, block) => self.tail_calls.contains(&block),
        }
    }
}

struct Analysis<'a> {
    module: &'a Module,
    summaries: &'a Arc<MemoryCallSummaries>,
    call_graph: CallGraphInfo,
    callers: IndexVec<FunctionId, Vec<CallSite>>,
    entered: IndexVec<FunctionId, Option<bool>>,
    freshness: IndexVec<FunctionId, Option<Freshness>>,
}

impl<'a> Analysis<'a> {
    fn new(module: &'a Module, summaries: &'a Arc<MemoryCallSummaries>) -> Self {
        let mut callers = index_vec![Vec::new(); module.functions.len()];
        for (caller, func) in module.functions.iter_enumerated() {
            for inst in func.instructions() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                {
                    callers[callee].push(CallSite::Inst(caller, inst));
                }
            }
            for (block, data) in func.blocks.iter_enumerated() {
                if let Some(Terminator::TailCall { function: callee, .. }) = data.terminator {
                    callers[callee].push(CallSite::Tail(caller, block));
                }
            }
        }
        Self {
            module,
            summaries,
            call_graph: CallGraphInfo::new(module),
            callers,
            entered: index_vec![None; module.functions.len()],
            freshness: index_vec![None; module.functions.len()],
        }
    }

    /// Whether every execution of `id` starts with the free memory pointer at its initial value.
    fn entered_fresh(&mut self, id: FunctionId) -> bool {
        if let Some(entered) = self.entered[id] {
            return entered;
        }
        let module = self.module;
        let func = &module.functions[id];
        let entered = if func.attributes.is_constructor
            || module.dispatch_entry() == Some(id)
            || self.call_graph.is_recursive(id)
        {
            false
        } else if func.is_runtime_entry() {
            let dispatch = module.dispatch_entry();
            self.callers[id].iter().all(|site| Some(site.caller()) == dispatch)
        } else {
            // A caller that is itself recursive ends the climb: recursive functions are never
            // fresh.
            let sites = self.callers[id].len();
            sites != 0
                && (0..sites).all(|index| {
                    let site = self.callers[id][index];
                    self.entered_fresh(site.caller())
                })
                && (0..sites).all(|index| {
                    let site = self.callers[id][index];
                    self.freshness(site.caller()).contains(site)
                })
        };
        self.entered[id] = Some(entered);
        entered
    }

    /// The reads and call sites of `id` that no write precedes, if `id` is entered fresh.
    fn freshness(&mut self, id: FunctionId) -> &Freshness {
        self.freshness[id].get_or_insert_with(|| compute_freshness(self.module, self.summaries, id))
    }
}

/// Forward must-analysis of the points before the first write to the slot.
fn compute_freshness(
    module: &Module,
    summaries: &Arc<MemoryCallSummaries>,
    id: FunctionId,
) -> Freshness {
    let func = &module.functions[id];
    let mut freshness = Freshness::default();
    if func.blocks.is_empty() {
        return freshness;
    }
    let alias = AliasAnalysis::with_call_summaries(func, Arc::clone(summaries));
    let cfg = CfgInfo::new(func);
    let first_writes = func
        .blocks
        .iter()
        .map(|block| block.instructions.iter().position(|&inst| may_write_slot(func, &alias, inst)))
        .collect::<IndexVec<BlockId, _>>();

    // A block is fresh on entry when every predecessor is fresh on exit; loops start optimistic.
    let mut fresh_in = index_vec![true; func.blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for &block in cfg.rpo() {
            let fresh = cfg.predecessors(block).iter().all(|&pred| {
                !cfg.is_reachable(pred) || (fresh_in[pred] && first_writes[pred].is_none())
            });
            if fresh_in[block] != fresh {
                fresh_in[block] = fresh;
                changed = true;
            }
        }
    }

    for &block in cfg.rpo() {
        if !fresh_in[block] {
            continue;
        }
        // The first writer is itself fresh: it reads before it writes.
        let end = first_writes[block].map_or(func.blocks[block].instructions.len(), |at| at + 1);
        for &inst in &func.blocks[block].instructions[..end] {
            if matches!(
                func.inst(inst).kind,
                InstKind::Fmp | InstKind::Alloc { .. } | InstKind::ICall { .. }
            ) {
                freshness.insts.insert(inst);
            }
        }
        if first_writes[block].is_none()
            && matches!(func.blocks[block].terminator, Some(Terminator::TailCall { .. }))
        {
            freshness.tail_calls.insert(block);
        }
    }
    freshness
}

fn is_read(func: &Function, inst: InstId, checked: bool) -> bool {
    match func.inst(inst).kind {
        InstKind::Fmp => true,
        InstKind::Alloc { semantics, .. } => {
            !func.inst(inst).metadata.deferred_alloc()
                && (checked || semantics.failure != AllocationFailure::Checked)
        }
        _ => false,
    }
}

fn has_read(func: &Function, checked: bool) -> bool {
    func.instructions().any(|inst| is_read(func, inst, checked))
}

/// Whether `inst` writes memory through the initial pointer plus constants, which stays at or
/// above the heap floor.
fn writes_above_initial_pointer(func: &Function, inst: InstId) -> bool {
    match func.inst(inst).kind {
        InstKind::Store(address, _, PtrLocation::Memory)
        | InstKind::Store8(address, _)
        | InstKind::Copy(address, ..)
        | InstKind::MemoryZero(address, _)
        | InstKind::DataCopy(_, address, _)
        | InstKind::CodeCopy(address, ..)
        | InstKind::ExtCodeCopy(_, address, ..) => is_initial_pointer_plus_constants(func, address),
        _ => false,
    }
}

/// Whether `value` is an `fmp` or `alloc` pointer moved by constant offsets below `2^64`.
fn is_initial_pointer_plus_constants(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst) = func.value(value) else { return false };
    let small = |offset: ValueId| func.value_u64(offset).is_some();
    match func.inst(*inst).kind {
        InstKind::Fmp | InstKind::Alloc { .. } => true,
        InstKind::PtrToInt(pointer, 256) | InstKind::IntToPtr(pointer) => {
            is_initial_pointer_plus_constants(func, pointer)
        }
        InstKind::Add(base, offset) | InstKind::PtrAdd(base, offset) if small(offset) => {
            is_initial_pointer_plus_constants(func, base)
        }
        InstKind::Add(offset, base) if small(offset) => {
            is_initial_pointer_plus_constants(func, base)
        }
        _ => false,
    }
}

/// Whether `inst` may write the free memory pointer's slot.
fn may_write_slot(func: &Function, alias: &AliasAnalysis, inst: InstId) -> bool {
    match &func.inst(inst).kind {
        InstKind::Alloc { .. } => true,
        // A write through a heap pointer lies above the slot; any other address may not.
        InstKind::Store(_, _, PtrLocation::Memory)
        | InstKind::Store8(..)
        | InstKind::Copy(..)
        | InstKind::MemoryZero(..)
        | InstKind::DataCopy(..)
        | InstKind::CodeCopy(..)
        | InstKind::ExtCodeCopy(..)
        | InstKind::Call { .. }
        | InstKind::CallCode { .. }
        | InstKind::StaticCall { .. }
        | InstKind::DelegateCall { .. }
        | InstKind::SetFmp(_) => {
            !writes_above_initial_pointer(func, inst) && alias.instruction_may_reset_fmp(func, inst)
        }
        // Reads and pure operations write no memory; skip their provenance queries.
        kind if matches!(
            kind.effect_kind(),
            EffectKind::Pure
                | EffectKind::MemoryRead
                | EffectKind::StorageRead
                | EffectKind::TransientRead
                | EffectKind::EnvironmentRead
        ) =>
        {
            false
        }
        _ => alias
            .instruction_mod_ref(func, inst)
            .may_write(alias, Location::Memory(AliasAnalysis::fmp_location())),
    }
}
