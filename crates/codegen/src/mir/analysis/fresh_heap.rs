//! Free-memory-pointer reads that return the entry's initial pointer, and zero fills of memory
//! that nothing wrote.
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
//!
//! # Zero fills of untouched memory
//!
//! Memory starts zeroed, and allocation moves the pointer up, so the memory at and above the
//! pointer holds only zero bytes until something writes there. The pass that lowers abstract
//! allocation drops a zero fill of such memory: the initialization of a zeroed allocation, and a
//! `memory_zero` of a range of an allocation. A second dataflow, over the same entries and call
//! sites, finds the points where the memory at and above the pointer is still zero. It starts
//! zero at a function entered fresh in the same sense as above, and an instruction ends it when
//! it may write anything but zero bytes at or above the pointer. Allocation keeps it, and so do
//! writes into allocations, which lie below the pointer, into the internal frames, which lie
//! below the heap floor, and into the scratch words. Writes through the pointer, through
//! an address the analysis cannot place, or over the pointer's slot end it, and so do internal
//! calls whose callee may end it before it returns. A callee that keeps it writes only into
//! memory of its own, so it cannot write the range of an earlier allocation either.
//!
//! A zeroed allocation made while the memory is zero needs no fill. A `memory_zero` of a range
//! of an allocation made then needs none when no instruction between the allocation and the fill
//! may write the range: the allocation covers it, and the memory it covers was zero.
//!
//! A dropped fill leaves memory unexpanded, which changes what the program can observe: its gas,
//! and `msize`. A function that observes gas, directly or through a call or tail call (the
//! dispatch entry's tail calls excepted), or has a caller that does, keeps its fills, and so does
//! every function of a module that reads `msize`. A fill of more than
//! [`EvmMemoryLayout::MAX_DEAD_FILL`] bytes stays too: expanding that much memory costs more gas
//! than a call has, so the fill is what makes a huge allocation fail.

use super::{
    Access, AddressSpace, AliasAnalysis, CallGraphInfo, CfgInfo, GasObservations, Location,
    LocationSize, MemoryAddress, MemoryBase, MemoryCallSummaries, MemoryLocation,
    integers::unsigned_bounds_with_opcodes,
};
use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, BlockId, Callee, EffectKind,
    Function, FunctionId, InstId, InstKind, Module, PtrLocation, Terminator, Value, ValueId,
    memory::EvmMemoryLayout,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashSet,
};
use std::sync::Arc;

/// What the free-memory pointer and the memory above it still hold at some points of the
/// optimized functions of a module.
pub(crate) struct FreshHeap {
    reads: IndexVec<FunctionId, FxHashSet<InstId>>,
    zero_fills: IndexVec<FunctionId, FxHashSet<InstId>>,
}

impl FreshHeap {
    /// Finds the fresh reads of every optimized function of `module`, those of checked allocations
    /// too in a function optimized for gas, and its zero fills of untouched memory.
    #[must_use]
    pub(crate) fn new(
        module: &Module,
        summaries: impl FnOnce() -> Arc<MemoryCallSummaries>,
    ) -> Self {
        let reads_msize = module.functions.iter().any(|func| {
            func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize))
        });
        let has_fill = module
            .functions
            .iter()
            .map(|func| !reads_msize && has_zero_fill(func))
            .collect::<IndexVec<FunctionId, _>>();
        let wanted = |id: FunctionId, func: &Function| {
            let objective = module.objective_of(func);
            !objective.is_none() && (has_read(func, objective.is_gas()) || has_fill[id])
        };
        if !module.functions.iter_enumerated().any(|(id, func)| wanted(id, func)) {
            return Self::none();
        }
        let summaries = summaries();
        let mut analysis = Analysis::new(module, &summaries);
        let mut reads = IndexVec::new();
        let mut zero_fills = IndexVec::new();
        for (id, func) in module.functions.iter_enumerated() {
            let mut fresh = FxHashSet::default();
            let mut zeros = FxHashSet::default();
            if wanted(id, func) {
                let checked = module.objective_of(func).is_gas();
                if has_read(func, checked) && analysis.entered(Fact::Slot, id) {
                    let freshness = analysis.facts(Fact::Slot, id);
                    fresh.extend(func.instructions().filter(|&inst| {
                        is_read(func, inst, checked) && freshness.insts.contains(&inst)
                    }));
                }
                if has_fill[id] && analysis.entered(Fact::Zero, id) {
                    zeros = analysis.zero_fills(id);
                }
            }
            reads.push(fresh);
            zero_fills.push(zeros);
        }
        Self { reads, zero_fills }
    }

    /// No read is fresh and no fill is dead.
    #[must_use]
    pub(crate) fn none() -> Self {
        Self { reads: IndexVec::new(), zero_fills: IndexVec::new() }
    }

    /// Whether `inst` of `func` reads the entry's initial pointer.
    #[must_use]
    pub(crate) fn contains(&self, func: FunctionId, inst: InstId) -> bool {
        self.reads.get(func).is_some_and(|reads| reads.contains(&inst))
    }

    /// What the allocation `inst` of `func` must initialize: nothing when its zero fill is dead.
    #[must_use]
    pub(crate) fn initialization(
        &self,
        func: FunctionId,
        inst: InstId,
        semantics: AllocationSemantics,
    ) -> AllocationInitialization {
        if self.skips_zero_fill(func, inst) {
            AllocationInitialization::Uninitialized
        } else {
            semantics.initialization
        }
    }

    /// Whether the zero fill of `inst` of `func` writes memory that is still zero: the
    /// initialization of a zeroed allocation, a `memory_zero`, or a zero word store.
    #[must_use]
    pub(crate) fn skips_zero_fill(&self, func: FunctionId, inst: InstId) -> bool {
        self.zero_fills.get(func).is_some_and(|fills| fills.contains(&inst))
    }
}

/// A fact that holds at the points of a function before the first instruction that ends it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fact {
    /// No write has changed the free-memory pointer's slot.
    Slot,
    /// The memory at and above the free-memory pointer holds only zero bytes.
    Zero,
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

/// The reads and call sites of a function that precede the first instruction ending a fact, when
/// the function is entered with the fact.
#[derive(Clone, Default)]
struct Freshness {
    insts: FxHashSet<InstId>,
    tail_calls: FxHashSet<BlockId>,
    /// Whether the fact still holds at every return, and at every tail call to a function that
    /// keeps it. Only the zero fact tracks it.
    clean_exit: bool,
}

impl Freshness {
    fn contains(&self, site: CallSite) -> bool {
        match site {
            CallSite::Inst(_, inst) => self.insts.contains(&inst),
            CallSite::Tail(_, block) => self.tail_calls.contains(&block),
        }
    }
}

/// What the analysis has settled about each function for one fact.
struct FactMemo {
    entered: IndexVec<FunctionId, Option<bool>>,
    freshness: IndexVec<FunctionId, Option<Freshness>>,
}

struct Analysis<'a> {
    module: &'a Module,
    summaries: &'a Arc<MemoryCallSummaries>,
    call_graph: CallGraphInfo,
    runtime_entries: DenseBitSet<FunctionId>,
    callers: IndexVec<FunctionId, Vec<CallSite>>,
    /// Indexed by [`Fact`].
    memo: [FactMemo; 2],
    observes_gas: IndexVec<FunctionId, Option<bool>>,
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
        let memo = || FactMemo {
            entered: index_vec![None; module.functions.len()],
            freshness: index_vec![None; module.functions.len()],
        };
        Self {
            module,
            summaries,
            call_graph: CallGraphInfo::new(module),
            runtime_entries: module.runtime_entries(),
            callers,
            memo: [memo(), memo()],
            observes_gas: index_vec![None; module.functions.len()],
        }
    }

    /// Whether every execution of `id` starts with `fact`.
    fn entered(&mut self, fact: Fact, id: FunctionId) -> bool {
        if let Some(entered) = self.memo[fact as usize].entered[id] {
            return entered;
        }
        let module = self.module;
        let entered = if module.constructor() == Some(id)
            || module.dispatch_entry() == Some(id)
            || self.call_graph.is_recursive(id)
        {
            false
        } else if self.runtime_entries.contains(id) {
            let dispatch = module.dispatch_entry();
            self.callers[id].iter().all(|site| Some(site.caller()) == dispatch)
        } else {
            // A caller that is itself recursive ends the climb: recursive functions are never
            // fresh.
            let sites = self.callers[id].len();
            sites != 0
                && (0..sites).all(|index| {
                    let site = self.callers[id][index];
                    self.entered(fact, site.caller())
                })
                && (0..sites).all(|index| {
                    let site = self.callers[id][index];
                    self.facts(fact, site.caller()).contains(site)
                })
        };
        self.memo[fact as usize].entered[id] = Some(entered);
        entered
    }

    /// The reads and call sites of `id` that precede the first instruction ending `fact`, if `id`
    /// is entered with it.
    fn facts(&mut self, fact: Fact, id: FunctionId) -> &Freshness {
        if self.memo[fact as usize].freshness[id].is_none() {
            let computed = self.compute(fact, id);
            self.memo[fact as usize].freshness[id] = Some(computed);
        }
        self.memo[fact as usize].freshness[id].get_or_insert_default()
    }

    /// Forward must-analysis of the points before the first instruction that ends `fact`.
    fn compute(&mut self, fact: Fact, id: FunctionId) -> Freshness {
        let module = self.module;
        let func = &module.functions[id];
        let mut freshness = Freshness::default();
        if func.blocks.is_empty() {
            return freshness;
        }
        let keepers = if fact == Fact::Zero { self.zero_keepers(id) } else { FxHashSet::default() };
        let alias = AliasAnalysis::with_call_summaries(func, Arc::clone(self.summaries));
        let cfg = CfgInfo::new(func);
        let first_ends = func
            .blocks
            .iter()
            .map(|block| {
                block.instructions.iter().position(|&inst| match fact {
                    Fact::Slot => may_write_slot(func, &alias, inst),
                    Fact::Zero => may_write_heap(func, &alias, &keepers, inst),
                })
            })
            .collect::<IndexVec<BlockId, _>>();

        // A block holds the fact on entry when every predecessor holds it on exit; loops start
        // optimistic.
        let mut holds_in = index_vec![true; func.blocks.len()];
        let mut changed = true;
        while changed {
            changed = false;
            for &block in cfg.rpo() {
                let holds = cfg.predecessors(block).iter().all(|&pred| {
                    !cfg.is_reachable(pred) || (holds_in[pred] && first_ends[pred].is_none())
                });
                if holds_in[block] != holds {
                    holds_in[block] = holds;
                    changed = true;
                }
            }
        }

        let mut clean_exit = true;
        for &block in cfg.rpo() {
            let intact = holds_in[block] && first_ends[block].is_none();
            match func.blocks[block].terminator {
                Some(Terminator::Return { .. }) if !intact => clean_exit = false,
                Some(Terminator::TailCall { function, .. })
                    if !intact || !keepers.contains(&function) =>
                {
                    clean_exit = false;
                }
                _ => {}
            }
            if !holds_in[block] {
                continue;
            }
            // The first instruction to end the fact is itself fresh: it reads before it writes.
            let end = first_ends[block].map_or(func.blocks[block].instructions.len(), |at| at + 1);
            for &inst in &func.blocks[block].instructions[..end] {
                if matches!(
                    func.inst(inst).kind,
                    InstKind::Fmp | InstKind::Alloc { .. } | InstKind::ICall { .. }
                ) {
                    freshness.insts.insert(inst);
                }
            }
            if intact && matches!(func.blocks[block].terminator, Some(Terminator::TailCall { .. }))
            {
                freshness.tail_calls.insert(block);
            }
        }
        freshness.clean_exit = clean_exit;
        freshness
    }

    /// Whether `id`, or a function that calls it, may observe gas.
    fn exposed_to_gas(&mut self, id: FunctionId) -> bool {
        let mut seen = DenseBitSet::new_empty(self.module.functions.len());
        let mut worklist = vec![id];
        while let Some(function) = worklist.pop() {
            if !seen.insert(function) {
                continue;
            }
            if self.observes_gas(function) {
                return true;
            }
            worklist.extend(self.callers[function].iter().map(|site| site.caller()));
        }
        false
    }

    fn observes_gas(&mut self, id: FunctionId) -> bool {
        if let Some(observes) = self.observes_gas[id] {
            return observes;
        }
        let func = &self.module.functions[id];
        let alias = AliasAnalysis::with_call_summaries(func, Arc::clone(self.summaries));
        let cfg = CfgInfo::new(func);
        // A tail call hands the rest of the call to a callee, which may observe gas. The dispatch
        // entry runs no code of its own before the entry it picks, so its tail calls measure
        // nothing.
        let tail_observes = self.module.dispatch_entry() != Some(id)
            && cfg.rpo().iter().any(|&block| {
                matches!(func.blocks[block].terminator, Some(Terminator::TailCall { function, .. })
                if self.summaries.get(function).is_none_or(|summary| summary.may_observe_gas()))
            });
        let observes = tail_observes || GasObservations::any(func, &cfg, &alias);
        self.observes_gas[id] = Some(observes);
        observes
    }

    /// The functions `id` calls that keep the memory above the pointer zero.
    fn zero_keepers(&mut self, id: FunctionId) -> FxHashSet<FunctionId> {
        let mut callees = self.call_graph.callees(id).collect::<FxHashSet<_>>();
        callees.retain(|&callee| {
            !self.call_graph.is_recursive(callee) && self.facts(Fact::Zero, callee).clean_exit
        });
        callees
    }

    /// The zero fills of `id`, entered with zero memory above the pointer, that write memory
    /// still zero.
    fn zero_fills(&mut self, id: FunctionId) -> FxHashSet<InstId> {
        let module = self.module;
        let func = &module.functions[id];
        let mut fills = FxHashSet::default();
        if self.exposed_to_gas(id) {
            return fills;
        }
        let alias = AliasAnalysis::with_call_summaries(func, Arc::clone(self.summaries));
        let cfg = CfgInfo::new(func);
        let keepers = self.zero_keepers(id);
        let zero = self.facts(Fact::Zero, id);
        let mut positions = index_vec![None; func.num_insts()];
        for (block, data) in func.blocks.iter_enumerated() {
            for (at, &inst) in data.instructions.iter().enumerate() {
                positions[inst] = Some((block, at));
            }
        }
        for (fill, fill_at) in positions.iter_enumerated() {
            let Some(fill_at) = *fill_at else { continue };
            if let InstKind::Alloc { size, .. } = func.inst(fill).kind {
                if is_zeroed_alloc(func, fill) && is_small(func, size) && zero.insts.contains(&fill)
                {
                    fills.insert(fill);
                }
            } else if let Some((address, size)) = zero_fill(func, fill)
                && size.is_none_or(|size| is_small(func, size))
                && let Some((alloc, location)) =
                    zeroed_range(func, &alias, fill, fill_at, address, size)
                && !func.inst(alloc).metadata.deferred_alloc()
                && zero.insts.contains(&alloc)
                && let Some(alloc_at) = positions[alloc]
                && !written_between(func, &cfg, alloc_at, fill_at, |inst, repeats| {
                    // A copy that ends at the fill, in the fill's own block, ran just before it.
                    let ends_here =
                        !repeats && positions[inst].is_some_and(|(block, _)| block == fill_at.0);
                    may_dirty_range(func, &alias, &keepers, inst, address, location, ends_here)
                })
            {
                fills.insert(fill);
            }
        }
        fills
    }
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
        | InstKind::DelegateCall { .. } => {
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

/// Whether `func` fills memory with zeros where a fresh heap makes the fill dead.
fn has_zero_fill(func: &Function) -> bool {
    let (mut allocates, mut fills) = (false, false);
    for inst in func.instructions() {
        if is_zeroed_alloc(func, inst) {
            return true;
        }
        allocates |= matches!(func.inst(inst).kind, InstKind::Alloc { .. })
            && !func.inst(inst).metadata.deferred_alloc();
        fills |= zero_fill(func, inst).is_some();
    }
    allocates && fills
}

/// Whether `inst` is an allocation that zeroes the memory it reserves.
fn is_zeroed_alloc(func: &Function, inst: InstId) -> bool {
    matches!(func.inst(inst).kind, InstKind::Alloc { semantics, .. }
        if semantics.initialization == AllocationInitialization::Zeroed)
        && !func.inst(inst).metadata.deferred_alloc()
}

/// The address of the bytes `inst` fills with zeros, and their count if it is not one word: a
/// `memory_zero`, or a store of a zero word.
fn zero_fill(func: &Function, inst: InstId) -> Option<(ValueId, Option<ValueId>)> {
    match func.inst(inst).kind {
        InstKind::MemoryZero(address, size) => Some((address, Some(size))),
        InstKind::Store(address, value, PtrLocation::Memory)
            if func.value_u64(value) == Some(0) =>
        {
            Some((address, None))
        }
        _ => None,
    }
}

/// Whether `inst` writes only zero bytes, outside the pointer's slot.
fn writes_zeros_off_slot(func: &Function, alias: &AliasAnalysis, inst: InstId) -> bool {
    zero_fill(func, inst).is_some()
        && !alias
            .instruction_mod_ref(func, inst)
            .may_write(alias, Location::Memory(AliasAnalysis::fmp_location()))
}

/// Whether `inst` may write a non-zero byte at or above the free-memory pointer, or move the
/// pointer by anything but allocating.
fn may_write_heap(
    func: &Function,
    alias: &AliasAnalysis,
    keepers: &FxHashSet<FunctionId>,
    inst: InstId,
) -> bool {
    match &func.inst(inst).kind {
        // An allocation bumps the pointer over memory that holds only zeros.
        InstKind::Alloc { .. } | InstKind::Fmp => false,
        InstKind::ICall { function: Callee::Function(callee), .. } => !keepers.contains(callee),
        kind if kind.effect_kind().is_read_only() => false,
        _ if writes_zeros_off_slot(func, alias, inst) => false,
        _ => alias.instruction_mod_ref(func, inst).writes().iter().any(|&access| match access {
            Access::Any(AddressSpace::Memory) => true,
            Access::Location(Location::Memory(location)) => !stays_below_heap(location),
            Access::Any(_) | Access::Location(_) => false,
        }),
    }
}

/// Whether memory `location` lies below the free-memory pointer, or below the heap: an
/// allocation, which the pointer has moved past, an internal frame, or a scratch word.
fn stays_below_heap(location: MemoryLocation) -> bool {
    match location.address.base {
        MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_) => true,
        MemoryBase::InternalFrame => location.size.as_const().is_some(),
        MemoryBase::Absolute => location
            .size
            .as_const()
            .and_then(|size| location.address.offset.checked_add(size))
            .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT),
        MemoryBase::UnboundedAllocation(_) | MemoryBase::Value(_) | MemoryBase::FreeMemory(_) => {
            false
        }
    }
}

/// The allocation, and the memory range around it, that the zero `fill` at `address` of `size`
/// bytes, or one word, writes: a range of the allocation, or the padding after a `bytes` payload,
/// which starts at the sum of the payload's start and its length after a copy of the payload.
fn zeroed_range(
    func: &Function,
    alias: &AliasAnalysis,
    fill: InstId,
    (block, at): (BlockId, usize),
    address: ValueId,
    size: Option<ValueId>,
) -> Option<(InstId, MemoryLocation)> {
    let size = size.map_or(LocationSize::Const(EvmMemoryLayout::WORD_SIZE), |size| {
        alias.location_size(func, size)
    });
    if let Some(location) = alias.memory_location(func, fill, address, size)
        && let Some(alloc) = location.address.base.allocation()
    {
        return Some((alloc, location));
    }
    // The copy ran before the fill, so a sum that wrapped had a zero length.
    let start = func.blocks[block].instructions[..at].iter().find_map(|&inst| {
        match func.inst(inst).kind {
            InstKind::Copy(start, _, len, _) if ends_at(func, address, start, len) => Some(start),
            _ => None,
        }
    })?;
    let start = alias.memory_address(func, start)?;
    let alloc = start.base.allocation()?;
    // The padding starts at or above the payload, and lies in the pointer's family.
    let base = if matches!(start.base, MemoryBase::Allocation(_)) {
        MemoryBase::UnboundedAllocation(alloc)
    } else {
        MemoryBase::Value(func.inst_result_value(alloc)?)
    };
    let address = MemoryAddress { base, ..start };
    Some((alloc, MemoryLocation::new(address, LocationSize::Unknown)))
}

/// Whether `inst` may write a non-zero byte into `location`, the range a zero fill at `address`
/// writes.
fn may_dirty_range(
    func: &Function,
    alias: &AliasAnalysis,
    keepers: &FxHashSet<FunctionId>,
    inst: InstId,
    address: ValueId,
    location: MemoryLocation,
    ends_here: bool,
) -> bool {
    match &func.inst(inst).kind {
        InstKind::Alloc { .. } | InstKind::Fmp => false,
        _ if zero_fill(func, inst).is_some() => false,
        // A callee that keeps the memory above the pointer zero writes memory of its own, and
        // never through a pointer to an earlier allocation. A range that may leave its
        // allocation can reach that memory.
        InstKind::ICall { function: Callee::Function(callee), .. }
            if keepers.contains(callee)
                && matches!(
                    location.address.base,
                    MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_)
                ) =>
        {
            false
        }
        kind if kind.effect_kind().is_read_only() => false,
        // A copy that ends where the fill starts, as the copy of a `bytes` payload does.
        &InstKind::Copy(dest, _, len, _) if ends_here && ends_at(func, address, dest, len) => false,
        _ => alias.instruction_mod_ref(func, inst).may_write(alias, Location::Memory(location)),
    }
}

/// Whether `address` is the sum of `start` and `len`, which a write of `len` bytes at `start`
/// ends at. A write of bytes past `2^64` runs out of gas, so the sum does not wrap.
fn ends_at(func: &Function, address: ValueId, start: ValueId, len: ValueId) -> bool {
    let strip = |value| AliasAnalysis::memory_address_value(func, value);
    let (start, len) = (strip(start), strip(len));
    let Value::Inst(sum) = func.value(strip(address)) else { return false };
    let (InstKind::Add(a, b) | InstKind::PtrAdd(a, b)) = func.inst(*sum).kind else {
        return false;
    };
    let operands = (strip(a), strip(b));
    operands == (start, len) || operands == (len, start)
}

/// Whether `dirty` holds for an instruction on a path from `from` to `to`, where `from` precedes
/// `to` and dominates it. Each path is cut where it passes `from` again, as the allocation there
/// is a new object. `dirty` also learns whether `to` runs more than once per path from `from`.
fn written_between(
    func: &Function,
    cfg: &CfgInfo,
    (from_block, from_at): (BlockId, usize),
    (to_block, to_at): (BlockId, usize),
    dirty: impl Fn(InstId, bool) -> bool,
) -> bool {
    let any = |block: BlockId, range: std::ops::Range<usize>, repeats: bool| {
        func.blocks[block].instructions[range].iter().any(|&inst| dirty(inst, repeats))
    };
    if from_block == to_block {
        return any(from_block, from_at + 1..to_at, false);
    }
    // Every block that reaches `to` without passing `from` lies on a path from `from`.
    let mut seen = DenseBitSet::new_empty(func.blocks.len());
    let mut between = Vec::new();
    let mut worklist = cfg.predecessors(to_block).to_vec();
    let mut loops_back = false;
    while let Some(block) = worklist.pop() {
        if !cfg.is_reachable(block) {
            continue;
        }
        if block == to_block {
            loops_back = true;
        } else if seen.insert(block) {
            between.push(block);
            if block != from_block {
                worklist.extend_from_slice(cfg.predecessors(block));
            }
        }
    }
    let len = |block: BlockId| func.blocks[block].instructions.len();
    between.into_iter().any(|block| {
        let first = if block == from_block { from_at + 1 } else { 0 };
        any(block, first..len(block), loops_back)
    }) || any(to_block, 0..to_at, loops_back)
        // A loop around `to` runs the rest of its block before the next visit.
        || loops_back && any(to_block, to_at..len(to_block), true)
}

/// Whether a fill of `size` bytes is small enough to leave: see [`EvmMemoryLayout::MAX_DEAD_FILL`].
fn is_small(func: &Function, size: ValueId) -> bool {
    unsigned_bounds_with_opcodes(func, size).1 <= U256::from(EvmMemoryLayout::MAX_DEAD_FILL)
}
