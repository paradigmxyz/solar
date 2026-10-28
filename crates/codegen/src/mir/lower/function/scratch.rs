//! `@custom:solar-scratch` blocks: memory a block allocates is reused after it.
//!
//! ```solidity
//! bytes32 digest;
//! /// @custom:solar-scratch
//! {
//!     bytes memory encoded = abi.encode(a, b);
//!     digest = keccak256(encoded);
//! }
//! ```
//!
//! Other compilers read the tag as documentation, so the block's allocations stay allocated for
//! the rest of the call. This compiler reads the free memory pointer when the block starts and
//! writes it back when the block ends normally, so later allocations reuse the block's memory.
//! A path that leaves the block early, by `return`, `break` or `continue`, keeps it allocated,
//! as without the tag. Reuse is only equivalent while nothing reachable after the block still
//! refers to that memory, which the check below proves; a program it cannot prove is rejected.
//!
//! The references to the block's memory are the results of the block's allocations, of the
//! encodings and storage reads that allocate, of its calls that return memory, the pointers loaded
//! from that memory, and every value derived from those by pure operations. The check rejects a use
//! of one after the block, a `return` of one, a store of one into memory the block did not allocate
//! or into storage, and a call in the block whose callee may store a reference to the block's
//! memory where code after the block can reach it. Memory the block allocated is the object of one
//! of the block's own allocations or encodings: a store goes to the object its address offsets,
//! since lowering offsets a pointer only within its object, and the alias analysis names that
//! object. A reference only may point into the block's memory, and a pointer loaded from that
//! memory or returned by a call may point to older memory, so a store through one is a store
//! outside. A callee's summary says, for the object of each parameter and for any other memory
//! that exists when it is entered, which pointers it may store there: ones derived from which of
//! its parameters, or any other, such as memory it allocates, which in the block is the block's
//! memory, or a pointer it loads. A call escapes when it may store a pointer to the block's memory
//! into an object the block did not allocate, or into older memory. Callees are summarized over
//! the call graph to a fixed point, and a callee with inline assembly may store anything anywhere.
//! So may the dispatcher of an internal function pointer that inline assembly retyped when it
//! reinterprets a memory reference as a word of another type, or such a word as a reference:
//! through it, a reference can leave the block as a plain word, or reach a callee that stores it
//! as one. Lowering to HIR already rejected inline assembly in the block, and lowering to MIR
//! rejects the inline assembly of a function body that a modifier's `_` runs inside it. Scalars,
//! such as hashes, lengths, and loaded words, leave the block freely.
//!
//! The free memory pointer that code before the block reads, as inline assembly does, points at
//! the memory the block then allocates. The check rejects a use of it, or of a value derived from
//! it, that may run after the block ends normally, unless the path there reads the pointer again.
//! On the way into the block, it rejects a store of one, other than into the free memory pointer,
//! and a call or other operation that takes one as data rather than as an address it accesses:
//! code after the block could load the pointer back. The same check runs across internal calls
//! which may execute a scratch block, including recursive calls and function-pointer dispatchers.
//! Callee summaries also record free pointers captured by stores or calls, even when the callee
//! returns no value. It does not follow whether assembly reserved memory at the pointer first.
//! In assembly functions, a load at an unknown address may read the free memory pointer, as may
//! an unaligned load that overlaps its word. Typed reads stay inside valid objects when all
//! assembly promises memory safety and no function-pointer dispatcher reinterprets references;
//! otherwise the whole contract stays conservative. Loads from fresh allocation bases are
//! disjoint. A load of the last word stored at the same disjoint address in its block instead
//! carries the stored value's provenance; any intervening memory write cancels that fact. Writing
//! a scalar into the free-pointer slot makes it an allocation address, so forwarding cannot hide
//! a possible read of that slot.
//! Values installed into that slot are allocation addresses too, even without a subsequent
//! load. Their pure inputs and aliases are followed from their definitions, including captures
//! before the installation. Callee summaries carry installed parameters back to their callers.
//! Copies and calls that may capture its bytes also escape, including through callee summaries.
//! A byte-range capture through a parameter plus nonnegative u64 constants stays conditional:
//! a caller can discharge it with an exact bounded compiler allocation above reserved slots.
//! Dynamic offsets, narrowing casts and captured SSA words keep their conservative treatment.
//! These facts only guide validation, not codegen.
//! An unannotated read-only assembly block also keeps the whole contract conservative: it can
//! forge a reference without writing memory.
//!
//! The check runs on the contract's MIR right after lowering, before any pass, once the internal
//! function pointer dispatchers exist, so every body a call in the block may run is summarized.
//! It only reads the MIR: code without the tag lowers as before.
//!
//! NOTE: Callee returns are followed to a fixed point, including computed free-pointer reads
//! and integer casts. Assembly can also derive pointers from unrelated objects past their ends;
//! such arithmetic does not establish allocation provenance.

use super::*;
use crate::mir::{
    AbiEncodeMode, ArgIdx, Callee, EffectKind, InstId, RawMemoryAccess, RawMemorySize, Terminator,
    analysis::{AddressSpace, AliasAnalysis, LocalSummaryCache, MemoryBase, MemoryCallSummaries},
    transform::cfg_simplify::simplify_trivial_phis,
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    smallvec::SmallVec,
};

/// A `@custom:solar-scratch` block after lowering.
pub(in crate::mir::lower) struct ScratchRegion {
    /// The free memory pointer the block starts with, which its normal exit restores.
    saved: ValueId,
    /// The first instruction the block's statements lowered to.
    start: InstId,
    /// The first instruction after them.
    end: InstId,
    /// The blocks whose terminators the block's statements set.
    terminated: Vec<BlockId>,
    /// The tag's span.
    tag: Span,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Lowers a block tagged `@custom:solar-scratch`, whose allocations the code after it reuses.
    pub(super) fn lower_scratch_block(&mut self, block: hir::Block<'_>, tag: Span) -> Option<()> {
        // TODO: Infer reusable memory regions in MIR without a tag when escape and allocation
        // observations prove it safe. Retain allocations when the proof is incomplete.
        // A variable without an initializer gets its default value where it is first used, and
        // it outlives the block, so the defaults are made before the block starts.
        let mut pending = self
            .default_bindings
            .iter()
            .chain(&self.deferred_bindings)
            .copied()
            .filter(|id| !self.values.contains_key(id))
            .collect::<Vec<_>>();
        pending.sort_unstable();
        for id in pending {
            self.load_variable(id, block.span)?;
        }
        // saved = fmp
        let saved = self.builder.fmp();
        let start = InstId::from_usize(self.builder.func().num_insts());
        // The blocks already terminated before the block's statements.
        let mut closed = GrowableBitSet::new_empty();
        for (id, block) in self.builder.func().blocks.iter_enumerated() {
            if block.terminator.is_some() {
                closed.insert(id);
            }
        }
        self.open_scratch.push(tag);
        let lowered = self.lower_block(block);
        self.open_scratch.pop();
        lowered?;
        if !self.is_terminated() {
            // set_fmp saved
            self.builder.set_fmp(saved);
        }
        let end = InstId::from_usize(self.builder.func().num_insts());
        let terminated = self
            .builder
            .func()
            .blocks
            .iter_enumerated()
            .filter(|&(id, block)| block.terminator.is_some() && !closed.contains(id))
            .map(|(id, _)| id)
            .collect();
        self.cx.state.scratch_regions.push(ScratchRegion { saved, start, end, terminated, tag });
        Some(())
    }
}

/// Rejects every reference to the memory of a `@custom:solar-scratch` block that code after the
/// block could still use.
///
/// `regions` lists each function's regions together, and `assembly` holds the functions that may
/// store any word: those whose code came from inline assembly, and the dispatchers that
/// reinterpret references for pointers it retyped.
pub(in crate::mir::lower) fn check_scratch_regions(
    gcx: Gcx<'_>,
    module: &Module,
    regions: &[(FunctionId, ScratchRegion)],
    assembly: &FxHashSet<FunctionId>,
    forged: bool,
) {
    if regions.is_empty() {
        return;
    }
    let calls =
        Arc::new(MemoryCallSummaries::new_cached(module, &mut LocalSummaryCache::default()));
    let scratch = scratch_callers(module, regions);
    let summaries = PointerWrites::compute(
        module,
        &calls,
        assembly,
        &scratch,
        forged,
        scratch.keys().copied().chain(regions.iter().map(|&(function, _)| function)),
    );
    for (id, func) in module.iter_functions() {
        let local = regions.iter().filter(|&&(function, _)| function == id);
        if !scratch.contains_key(&id) && local.clone().next().is_none() {
            continue;
        }
        let function = Analyzed::new(func, &calls, forged || assembly.contains(&id));
        let saved = local.clone().map(|(_, region)| region.saved).collect::<FxHashSet<_>>();
        for (_, region) in local {
            check_region(gcx, module, &function, &summaries, assembly, &saved, region);
        }
        let uses = UseSites::new(&function);
        for (block, data) in function.func.blocks.iter_enumerated() {
            for (index, &inst) in data.instructions.iter().enumerate() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    function.func.inst(inst).kind
                    && let Some(&tag) = scratch.get(&callee)
                {
                    let mut escapes = Vec::new();
                    check_free_pointers(
                        &function,
                        &uses,
                        &summaries,
                        &saved,
                        (block, index),
                        |_| false,
                        &mut escapes,
                    );
                    report_escapes(gcx, module, assembly, tag, escapes);
                }
            }
        }
    }
}

/// Where a reference to the block's memory reaches.
enum Escape {
    /// A use after the block.
    Outlives,
    /// A `return` of the reference.
    Returned,
    /// A store of the reference where code after the block can reach it.
    Stored,
    /// A call whose callee may store a reference where code after the block can reach it.
    Call(FunctionId),
    /// Passing a free pointer to a function which may restore scratch memory.
    Passed,
}

fn check_region(
    gcx: Gcx<'_>,
    module: &Module,
    function: &Analyzed,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    assembly: &FxHashSet<FunctionId>,
    saved: &FxHashSet<ValueId>,
    region: &ScratchRegion,
) {
    let func = &function.func;
    let inside = |inst: InstId| region.start <= inst && inst < region.end;
    let uses = UseSites::new(function);

    // The references to the block's memory, seeded by what the block allocates. A reference that
    // flowed through code after the block, such as a loop header, refers to memory already
    // reused, so every use of it is late.
    let mut references = FxHashSet::default();
    let mut late = FxHashSet::default();
    let mut stack = Vec::new();
    for inst in func.instructions().filter(|&inst| inside(inst)) {
        if allocates(func, inst)
            && let Some(result) = func.inst_result_value(inst)
            && !saved.contains(&result)
            && references.insert(result)
        {
            stack.push((result, false));
        }
    }
    // Whether `pointer` points into memory the block allocated, rather than only maybe referring
    // to it.
    let allocated = |pointer: ValueId| {
        let object = function.object(pointer);
        function.aa.memory_address(func, object).is_some_and(|address| match address.base {
            MemoryBase::Allocation(site)
            | MemoryBase::DynamicAllocation(site)
            | MemoryBase::FreeMemory(site) => inside(site),
            MemoryBase::Value(value) => match *func.value(value) {
                Value::Inst(inst) => inside(inst) && allocates_new(func, inst),
                _ => false,
            },
            _ => false,
        })
    };
    let mut escapes = Vec::new();
    while let Some((value, was_late)) = stack.pop() {
        for &block in uses.terminators.get(&value).into_iter().flatten() {
            let span = func.blocks[block].terminator_metadata.source_span();
            if was_late || !region.terminated.contains(&block) {
                escapes.push((span, Escape::Outlives));
            } else if matches!(func.blocks[block].terminator, Some(Terminator::Return { .. })) {
                escapes.push((span, Escape::Returned));
            }
        }
        for &inst in uses.instructions.get(&value).into_iter().flatten() {
            let instruction = func.inst(inst);
            let kind = &instruction.kind;
            let is_late = was_late || !inside(inst);
            let result = func.inst_result_value(inst);
            let derived = if kind.effect_kind() == EffectKind::Pure {
                pure_derivation(func, inst).is_some()
            } else if is_late {
                escapes.push((instruction.metadata.source_span(), Escape::Outlives));
                continue;
            } else {
                // A pointer the block's memory holds refers to it as well.
                is_load(kind) && result.is_some_and(|result| is_pointer(func.value_ty(result)))
            };
            if derived {
                let result = result.expect("a derived reference is a result");
                let set = if is_late { &mut late } else { &mut references };
                if set.insert(result) {
                    stack.push((result, is_late));
                }
                continue;
            }
            if let Some((destination, stored)) = store_operands(kind)
                && stored == value
                && destination.is_none_or(|destination| !allocated(destination))
            {
                escapes.push((instruction.metadata.source_span(), Escape::Stored));
            }
        }
    }

    // A pointer to free memory read before the block, as inline assembly reads one, points at the
    // memory the block allocates, which code after the block's normal exit reuses. So a use of
    // one that may run after that exit is late, unless the path there reads the pointer again, and
    // so is a store or a call before it that keeps the pointer for code after it to load again.
    if let Some(restore) = restore_position(func, region) {
        check_free_pointers(function, &uses, summaries, saved, restore, inside, &mut escapes);
    }

    // A call in the block may store a reference to memory it or the block allocated where code
    // after the block can reach it.
    for inst in func.instructions().filter(|&inst| inside(inst)) {
        let InstKind::ICall { function: Callee::Function(callee), args } = &func.inst(inst).kind
        else {
            continue;
        };
        let Some(writes) = summaries.get(callee) else { continue };
        let block_memory = |stored: &Stored| {
            stored.any
                || stored
                    .params
                    .iter()
                    .any(|param| args.get(param.index()).is_none_or(|arg| references.contains(arg)))
        };
        let reaches_outside = block_memory(&writes.other)
            || writes.params.iter().any(|(param, stored)| {
                args.get(param.index()).is_none_or(|&arg| !allocated(arg)) && block_memory(stored)
            });
        if reaches_outside {
            escapes.push((func.inst(inst).metadata.source_span(), Escape::Call(*callee)));
        }
    }

    report_escapes(gcx, module, assembly, region.tag, escapes);
}

/// Whether `inst` allocates the memory its result refers to.
fn allocates(func: &Function, inst: InstId) -> bool {
    match &func.inst(inst).kind {
        InstKind::Alloc { .. }
        | InstKind::Fmp
        | InstKind::AbiEncode { .. }
        | InstKind::AbiEncodePacked { hash: false, .. }
        | InstKind::AbiDecode { .. }
        | InstKind::StorageBytesLoad(..)
        | InstKind::StorageArrayLoad { .. }
        | InstKind::ICall {
            function: Callee::Builtin(crate::mir::Builtin::ReturndataBytes), ..
        } => true,
        // A callee's result may be memory it allocated in the block. Only inline assembly, or
        // the dispatcher of a pointer it retyped, can turn a pointer into a word, and either
        // rejects the call anyway.
        InstKind::ICall { function: Callee::Function(_), .. } => {
            func.inst_result_value(inst).is_some_and(|result| is_pointer(func.value_ty(result)))
        }
        _ => false,
    }
}

/// Whether `inst` allocates the memory its result refers to during the call, unlike a call to a
/// function, which may return older memory.
fn allocates_new(func: &Function, inst: InstId) -> bool {
    allocates(func, inst)
        && !matches!(func.inst(inst).kind, InstKind::ICall { function: Callee::Function(_), .. })
}

/// Whether a value of type `ty` can carry a memory pointer: a reference, an aggregate that may
/// hold one, or a raw word, which a pointer cast produces.
fn may_hold_pointer(ty: Option<MirType>) -> bool {
    matches!(
        ty,
        Some(
            MirType::Int(_)
                | MirType::MEMPTR
                | MirType::Struct(_)
                | MirType::Slice(PtrLocation::Memory)
        )
    )
}

/// Whether `ty` is a typed memory reference, rather than a word that may only be data.
fn is_pointer(ty: Option<MirType>) -> bool {
    matches!(ty, Some(MirType::MEMPTR | MirType::Struct(_) | MirType::Slice(PtrLocation::Memory)))
}

fn is_load(kind: &InstKind) -> bool {
    matches!(kind, InstKind::Load(_, PtrLocation::Memory))
}

/// The destination and stored value of a store: the memory address written, or `None` for
/// storage, transient storage, and immutables, which outlive the call.
fn store_operands(kind: &InstKind) -> Option<(Option<ValueId>, ValueId)> {
    match *kind {
        InstKind::Store(address, value, PtrLocation::Memory) | InstKind::Store8(address, value) => {
            Some((Some(address), value))
        }
        InstKind::Store(_, value, _) | InstKind::StoreImmutable(_, value) => Some((None, value)),
        _ => None,
    }
}

/// The result of a pure instruction that may still hold a pointer its operands hold: a length, a
/// comparison, or a scalar field of a reference is a value.
fn pure_derivation(func: &Function, inst: InstId) -> Option<ValueId> {
    let result = func.inst_result_value(inst)?;
    let ty = func.value_ty(result);
    match func.inst(inst).kind {
        InstKind::ExtractValue { .. } => is_pointer(ty),
        _ => may_hold_pointer(ty),
    }
    .then_some(result)
}

/// Whether `inst` may read the free memory pointer.
fn reads_free_pointer(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    inst: InstId,
) -> bool {
    let func = constants.func;
    match func.inst(inst).kind {
        InstKind::Fmp => true,
        InstKind::Load(address, PtrLocation::Memory) => {
            function.overlaps_free_pointer(
                constants,
                address,
                Some(U256::from(EvmMemoryLayout::WORD_SIZE)),
            ) && function.fresh_start(address).is_none()
        }
        _ => false,
    }
}

/// The instructions, and the blocks whose terminators, may run after the instruction at `from` on
/// a path that does not run the one at `barrier` first.
fn runs_after(
    func: &Function,
    (from_block, from): (BlockId, usize),
    barrier: Option<(BlockId, usize)>,
) -> (DenseBitSet<InstId>, DenseBitSet<BlockId>) {
    let mut instructions = DenseBitSet::new_empty(func.num_insts());
    let mut terminators = DenseBitSet::new_empty(func.blocks.len());
    let mut entered = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = vec![(from_block, from + 1)];
    while let Some((block, start)) = stack.pop() {
        let data = &func.blocks[block];
        let end = barrier
            .filter(|&(barrier_block, barrier)| block == barrier_block && start <= barrier)
            .map_or(data.instructions.len(), |(_, barrier)| barrier);
        for &inst in &data.instructions[start..end] {
            instructions.insert(inst);
        }
        if end < data.instructions.len() {
            continue;
        }
        terminators.insert(block);
        for successor in data.terminator.iter().flat_map(Terminator::successors) {
            if entered.insert(successor) {
                stack.push((successor, 0));
            }
        }
    }
    (instructions, terminators)
}

/// The instructions that may run before the one at `to` on a path to it that does not run the one
/// at `barrier`.
fn runs_before(
    func: &Function,
    (to_block, to): (BlockId, usize),
    barrier: Option<(BlockId, usize)>,
) -> DenseBitSet<InstId> {
    let mut predecessors: IndexVec<BlockId, Vec<BlockId>> =
        index_vec![Vec::new(); func.blocks.len()];
    for (block, data) in func.blocks.iter_enumerated() {
        for successor in data.terminator.iter().flat_map(Terminator::successors) {
            predecessors[successor].push(block);
        }
    }
    let mut instructions = DenseBitSet::new_empty(func.num_insts());
    let mut left = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = vec![(to_block, to)];
    while let Some((block, end)) = stack.pop() {
        let start = barrier
            .filter(|&(barrier_block, barrier)| block == barrier_block && barrier < end)
            .map_or(0, |(_, barrier)| barrier + 1);
        for &inst in &func.blocks[block].instructions[start..end] {
            instructions.insert(inst);
        }
        if start > 0 {
            continue;
        }
        for &predecessor in &predecessors[block] {
            if left.insert(predecessor) {
                stack.push((predecessor, func.blocks[predecessor].instructions.len()));
            }
        }
    }
    instructions
}

/// Whether `inst` takes `value` as data, which it may keep where a later load finds it, rather
/// than only as the address or size of memory it accesses: as the word a store writes, other than
/// into the free memory pointer, or as an operand of a call or an encoding.
fn captures(constants: &mut ConstantWords<'_>, inst: InstId, value: ValueId) -> bool {
    let func = constants.func;
    let kind = &func.inst(inst).kind;
    if matches!(kind, InstKind::SetFmp(_)) {
        return false;
    }
    if let InstKind::Store(address, _, PtrLocation::Memory) = *kind
        && constants.get(address) == Some(U256::from(EvmMemoryLayout::FMP_SLOT))
    {
        return false;
    }
    let mut accessed = 0;
    kind.visit_raw_memory(|_, address, size| {
        accessed += usize::from(address == value);
        accessed += usize::from(matches!(size, RawMemorySize::Value(size) if size == value));
    });
    kind.operands().into_iter().filter(|&operand| operand == value).count() > accessed
}

/// The memory pointers a function may store somewhere: ones derived from which of its parameters,
/// and whether any other, such as memory it allocates or a pointer it loads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Stored {
    params: SmallVec<[ArgIdx; 2]>,
    any: bool,
}

impl Stored {
    fn add(&mut self, other: &Self) {
        self.any |= other.any;
        for &param in &other.params {
            if !self.params.contains(&param) {
                self.params.push(param);
                self.params.sort_unstable();
            }
        }
    }
}

/// The pointers a function may store into the objects of its parameters, and into any other
/// memory that exists when it is entered, plus free-pointer provenance in returns and captures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PointerWrites {
    params: SmallVec<[(ArgIdx, Stored); 2]>,
    other: Stored,
    /// Whether a return may carry a read of the free memory pointer.
    returns_free: bool,
    /// Captures of free-pointer values or bytes, conditional on parameters reaching its slot.
    captures_free: Stored,
    /// Parameters whose values may be installed into the free memory pointer.
    installs_free: SmallVec<[ArgIdx; 2]>,
    /// Unknown installed addresses, conditional on which parameters may reach the FMP slot.
    unknown_fmp: Stored,
}

impl PointerWrites {
    /// Summarizes every function the `roots` may call, to a fixed point over the call graph.
    fn compute(
        module: &Module,
        calls: &Arc<MemoryCallSummaries>,
        assembly: &FxHashSet<FunctionId>,
        scratch: &FxHashMap<FunctionId, Span>,
        forged: bool,
        roots: impl Iterator<Item = FunctionId>,
    ) -> FxHashMap<FunctionId, Self> {
        let mut order = Vec::new();
        let mut seen = FxHashSet::default();
        let mut stack = roots.collect::<Vec<_>>();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            let func = module.function(id);
            for inst in func.instructions() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                {
                    stack.push(callee);
                }
            }
        }
        let analyzed = order
            .iter()
            .map(|&id| {
                (id, Analyzed::new(module.function(id), calls, forged || assembly.contains(&id)))
            })
            .collect::<FxHashMap<_, _>>();
        let mut summaries =
            order.iter().map(|&id| (id, Self::default())).collect::<FxHashMap<_, _>>();
        // Summaries only grow, so the iteration ends.
        loop {
            let mut changed = false;
            for &id in &order {
                let summary = Self::of(
                    &analyzed[&id],
                    assembly.contains(&id),
                    scratch.contains_key(&id),
                    &summaries,
                );
                if summaries[&id] != summary {
                    summaries.insert(id, summary);
                    changed = true;
                }
            }
            if !changed {
                return summaries;
            }
        }
    }

    /// Summarizes a function given the summaries of its callees, and whether its code came from
    /// inline assembly.
    fn of(
        function: &Analyzed,
        assembly: bool,
        scratch: bool,
        summaries: &FxHashMap<FunctionId, Self>,
    ) -> Self {
        let func = &function.func;
        let mut writes = Self::default();
        writes.other.any = assembly;
        let mut constants = ConstantWords::new(func);
        let loads = stored_load_values(function);
        // A later reset cannot hide an installation that an earlier scratch block reclaimed.
        let reaching = if scratch {
            DenseBitSet::new_filled(func.num_insts())
        } else {
            reaching_fmp_writes(function, &mut constants, None)
        };
        for inst in reaching.iter() {
            writes.unknown_fmp.add(&unknown_fmp_write(function, &mut constants, summaries, inst));
        }
        let mut free = installed_free_pointers(
            function,
            &mut constants,
            summaries,
            &loads,
            func.instructions(),
        );
        writes.installs_free = free
            .iter()
            .filter_map(|&value| match *func.value(value) {
                Value::Arg(index) => Some(index),
                _ => None,
            })
            .collect();
        writes.installs_free.sort_unstable();
        loop {
            let before = free.len();
            for inst in func.instructions() {
                let kind = &func.inst(inst).kind;
                if let Some(result) = func.inst_result_value(inst)
                    && may_hold_pointer(func.value_ty(result))
                    && (matches!(loads.get(&inst), Some(LoadedWord::Value(value))
                            if free.contains(value))
                        || (!loads.contains_key(&inst)
                            && reads_free_pointer(function, &mut constants, inst))
                        || matches!(kind, InstKind::ICall { function: Callee::Function(callee), .. }
                            if summaries.get(callee).is_some_and(|summary| summary.returns_free))
                        || (kind.effect_kind() == EffectKind::Pure
                            && kind.operands().iter().any(|value| free.contains(value))))
                {
                    free.insert(result);
                }
            }
            if free.len() == before {
                break;
            }
        }
        writes.returns_free = func.blocks.iter().any(|block| {
            matches!(&block.terminator, Some(Terminator::Return { values })
                if values.iter().any(|value| free.contains(value)))
        });
        for inst in func.instructions() {
            let kind = &func.inst(inst).kind;
            writes.captures_free.add(&captures_free_memory(
                function,
                &mut constants,
                summaries,
                inst,
            ));
            writes.captures_free.any |= kind.effect_kind() != EffectKind::Pure
                && kind
                    .operands()
                    .iter()
                    .any(|value| free.contains(value) && captures(&mut constants, inst, *value));
        }
        // Where a store of `stored` through `address` lands: this call's own memory, a
        // parameter's object, or memory older than the call.
        let record = |writes: &mut Self, address: ValueId, stored: &Stored| {
            let Some(address) = function.aa.memory_address(func, function.object(address)) else {
                writes.other.add(stored);
                return;
            };
            match address.base {
                MemoryBase::InternalFrame
                | MemoryBase::Allocation(_)
                | MemoryBase::DynamicAllocation(_) => {}
                MemoryBase::FreeMemory(read) => {
                    if func
                        .inst_result_value(read)
                        .and_then(|value| function.fresh_start(value))
                        .is_none()
                    {
                        writes.other.add(stored);
                    }
                }
                MemoryBase::Absolute => {
                    if address.offset >= EvmMemoryLayout::HEAP_START {
                        writes.other.add(stored);
                    }
                }
                MemoryBase::Value(value) => match *func.value(value) {
                    Value::Arg(index) => writes.store_into_param(index, stored),
                    _ if function.fresh_start(value).is_some() => {}
                    _ => writes.other.add(stored),
                },
            }
        };
        // The values that may carry a pointer: every typed reference and what derives from one.
        let mut pointers = func
            .live_values()
            .filter(|&value| is_pointer(func.value_ty(value)))
            .collect::<FxHashSet<_>>();
        loop {
            let before = pointers.len();
            for inst in func.instructions() {
                let kind = &func.inst(inst).kind;
                if kind.effect_kind() == EffectKind::Pure
                    && let Some(result) = func.inst_result_value(inst)
                    && match kind {
                        InstKind::ExtractValue { .. } => is_pointer(func.value_ty(result)),
                        _ => may_hold_pointer(func.value_ty(result)),
                    }
                    && kind.operands().iter().any(|operand| pointers.contains(operand))
                {
                    pointers.insert(result);
                }
            }
            if pointers.len() == before {
                break;
            }
        }
        // The pointers a value in `pointers` may be: parameters, or anything else.
        let sources = |value: ValueId| {
            let mut stored = Stored::default();
            let mut seen = FxHashSet::default();
            let mut stack = vec![value];
            while let Some(value) = stack.pop() {
                if !seen.insert(value) {
                    continue;
                }
                match *func.value(value) {
                    Value::Arg(index) => {
                        stored.add(&Stored { params: SmallVec::from_slice(&[index]), any: false })
                    }
                    Value::Inst(inst) => {
                        let kind = &func.inst(inst).kind;
                        if kind.effect_kind() == EffectKind::Pure {
                            stack.extend(
                                kind.operands()
                                    .into_iter()
                                    .filter(|operand| pointers.contains(operand)),
                            );
                        } else {
                            // An allocation, a loaded pointer, or a call's result.
                            stored.any = true;
                        }
                    }
                    _ => {}
                }
            }
            stored
        };
        // What a callee's `stored` is in this function, given the call's `args`.
        let translate = |stored: &Stored, args: &[ValueId]| {
            let mut translated = Stored { params: SmallVec::new(), any: stored.any };
            for param in &stored.params {
                match args.get(param.index()) {
                    Some(&arg) => translated.add(&sources(arg)),
                    None => translated.any = true,
                }
            }
            translated
        };
        for inst in func.instructions() {
            let kind = &func.inst(inst).kind;
            if let Some((destination, value)) = store_operands(kind)
                && pointers.contains(&value)
            {
                let stored = sources(value);
                match destination {
                    Some(destination) => record(&mut writes, destination, &stored),
                    None => writes.other.add(&stored),
                }
            }
            if let InstKind::ICall { function: Callee::Function(callee), args } = kind
                && let Some(callee) = summaries.get(callee)
            {
                writes.other.add(&translate(&callee.other, args));
                for (param, stored) in &callee.params {
                    let stored = translate(stored, args);
                    match args.get(param.index()) {
                        Some(&arg) => record(&mut writes, arg, &stored),
                        None => writes.other.add(&stored),
                    }
                }
            }
        }
        writes
    }

    fn store_into_param(&mut self, param: ArgIdx, stored: &Stored) {
        match self.params.iter_mut().find(|(index, _)| *index == param) {
            Some((_, existing)) => existing.add(stored),
            None => {
                self.params.push((param, stored.clone()));
                self.params.sort_unstable_by_key(|&(index, _)| index);
            }
        }
    }
}

/// Instruction and terminator uses, with trivial lowering phis resolved.
struct UseSites {
    instructions: FxHashMap<ValueId, Vec<InstId>>,
    terminators: FxHashMap<ValueId, Vec<BlockId>>,
    loads: FxHashMap<InstId, LoadedWord>,
}

impl UseSites {
    fn new(function: &Analyzed) -> Self {
        let func = &function.func;
        let mut uses = Self {
            instructions: FxHashMap::default(),
            terminators: FxHashMap::default(),
            loads: stored_load_values(function),
        };
        for (block, data) in func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                for operand in func.inst(inst).operands() {
                    uses.instructions.entry(operand).or_default().push(inst);
                }
                if let Some(&LoadedWord::Value(stored)) = uses.loads.get(&inst) {
                    uses.instructions.entry(stored).or_default().push(inst);
                }
            }
            if let Some(terminator) = &data.terminator {
                terminator.visit_operands(|operand| {
                    uses.terminators.entry(operand).or_default().push(block);
                });
            }
        }
        uses
    }
}

/// The normal exit that restores a region's FMP. Early exits retain its allocations.
fn restore_position(func: &Function, region: &ScratchRegion) -> Option<(BlockId, usize)> {
    func.blocks.iter_enumerated().find_map(|(block, data)| {
        let restores = |&inst: &InstId| {
            matches!(func.inst(inst).kind, InstKind::SetFmp(value) if value == region.saved)
        };
        Some((block, data.instructions.iter().position(restores)?))
    })
}

/// Functions whose internal calls may restore scratch memory, with one originating tag. The
/// fixed point includes recursive callers and the generated function-pointer dispatchers.
fn scratch_callers(
    module: &Module,
    regions: &[(FunctionId, ScratchRegion)],
) -> FxHashMap<FunctionId, Span> {
    let mut scratch = FxHashMap::default();
    for (function, region) in regions {
        if restore_position(module.function(*function), region).is_some() {
            scratch.entry(*function).or_insert(region.tag);
        }
    }
    loop {
        let before = scratch.len();
        for (id, func) in module.iter_functions() {
            if scratch.contains_key(&id) {
                continue;
            }
            if let Some(tag) = func.instructions().find_map(|inst| match func.inst(inst).kind {
                InstKind::ICall { function: Callee::Function(callee), .. } => {
                    scratch.get(&callee).copied()
                }
                _ => None,
            }) {
                scratch.entry(id).or_insert(tag);
            }
        }
        if scratch.len() == before {
            return scratch;
        }
    }
}

/// Checks free-pointer lifetimes across a normal region exit or a call that may execute one.
/// Captures are conservative: they may be reloaded in this function, in a caller, or after a
/// later invocation of the scratch function. A read after an acyclic boundary is fresh.
fn check_free_pointers(
    function: &Analyzed,
    uses: &UseSites,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    saved: &FxHashSet<ValueId>,
    boundary: (BlockId, usize),
    inside: impl Fn(InstId) -> bool,
    escapes: &mut Vec<(Option<Span>, Escape)>,
) {
    let func = &function.func;
    let mut constants = ConstantWords::new(func);
    let prior = runs_before(func, boundary, None);
    let boundary_inst = func.blocks[boundary.0].instructions[boundary.1];
    let reaching = reaching_fmp_writes(function, &mut constants, Some(boundary));
    for inst in reaching.iter().filter(|&inst| !inside(inst)).chain(std::iter::once(boundary_inst))
    {
        let unknown = unknown_fmp_write(function, &mut constants, summaries, inst);
        if unknown.any || !unknown.params.is_empty() {
            let escape = match func.inst(inst).kind {
                InstKind::ICall { function: Callee::Function(callee), .. } => Escape::Call(callee),
                _ => Escape::Stored,
            };
            escapes.push((func.inst(inst).metadata.source_span(), escape));
        }
    }
    let installed = installed_free_pointers(
        function,
        &mut constants,
        summaries,
        &uses.loads,
        prior.iter().filter(|&inst| !inside(inst)).chain(std::iter::once(boundary_inst)),
    );
    for inst in prior.iter() {
        if inside(inst) {
            continue;
        }
        let captured = captures_free_memory(function, &mut constants, summaries, inst);
        if captured.any || !captured.params.is_empty() {
            let escape = match func.inst(inst).kind {
                InstKind::ICall { function: Callee::Function(callee), .. } => Escape::Call(callee),
                _ => Escape::Stored,
            };
            escapes.push((func.inst(inst).metadata.source_span(), escape));
        }
    }
    let mut pointers = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for (index, &read) in data.instructions.iter().enumerate() {
            let Some(pointer) = func.inst_result_value(read) else { continue };
            let returns_free = matches!(&func.inst(read).kind,
                InstKind::ICall { function: Callee::Function(callee), .. }
                    if summaries.get(callee).is_some_and(|summary| summary.returns_free));
            if !installed.contains(&pointer)
                && (inside(read)
                    || !prior.contains(read)
                    || saved.contains(&pointer)
                    || uses.loads.contains_key(&read)
                    || !(reads_free_pointer(function, &mut constants, read) || returns_free))
            {
                continue;
            }
            pointers.push((pointer, (!installed.contains(&pointer)).then_some((block, index))));
        }
    }
    pointers.extend(
        installed
            .into_iter()
            .filter(|&value| !matches!(func.value(value), Value::Inst(_)))
            .map(|value| (value, None)),
    );
    for (pointer, definition) in pointers {
        let (later, later_terminators) = runs_after(func, boundary, definition);
        let earlier = runs_before(func, boundary, definition);
        let mut derived = FxHashSet::from_iter([pointer]);
        let mut stack = vec![pointer];
        while let Some(value) = stack.pop() {
            for &block in uses.terminators.get(&value).into_iter().flatten() {
                if later_terminators.contains(block) {
                    let span = func.blocks[block].terminator_metadata.source_span();
                    escapes.push((span, Escape::Outlives));
                }
            }
            for &inst in uses.instructions.get(&value).into_iter().flatten() {
                if func.inst(inst).kind.effect_kind() == EffectKind::Pure
                    || matches!(uses.loads.get(&inst), Some(LoadedWord::Value(stored))
                        if *stored == value)
                {
                    if let Some(result) = func.inst_result_value(inst)
                        && may_hold_pointer(func.value_ty(result))
                        && derived.insert(result)
                    {
                        stack.push(result);
                    }
                } else if inst == boundary_inst && captures(&mut constants, inst, value) {
                    escapes.push((func.inst(inst).metadata.source_span(), Escape::Passed));
                } else if later.contains(inst) {
                    escapes.push((func.inst(inst).metadata.source_span(), Escape::Outlives));
                } else if earlier.contains(inst) && captures(&mut constants, inst, value) {
                    let escape = match func.inst(inst).kind {
                        InstKind::ICall { function: Callee::Function(callee), .. } => {
                            Escape::Call(callee)
                        }
                        _ => Escape::Stored,
                    };
                    escapes.push((func.inst(inst).metadata.source_span(), escape));
                }
            }
        }
    }
}

fn report_escapes(
    gcx: Gcx<'_>,
    module: &Module,
    assembly: &FxHashSet<FunctionId>,
    tag: Span,
    escapes: Vec<(Option<Span>, Escape)>,
) {
    let mut reported = FxHashSet::default();
    for (span, escape) in escapes {
        let span = span.unwrap_or(tag);
        if !reported.insert(span) {
            continue;
        }
        let message = match escape {
            Escape::Outlives => "memory of a `@custom:solar-scratch` block is used after the block",
            Escape::Returned => "this returns memory of a `@custom:solar-scratch` block",
            Escape::Stored => {
                "this stores a reference to memory of a `@custom:solar-scratch` block where code \
                 after the block can reach it"
            }
            Escape::Call(_) => {
                "this call may store a reference to memory of a `@custom:solar-scratch` block \
                 where code after the block can reach it"
            }
            Escape::Passed => {
                "this passes a free memory pointer to a function that reuses scratch memory"
            }
        };
        let mut err = gcx
            .dcx()
            .err(message)
            .span(span)
            .span_note(tag, "the memory this block allocates is reused after it");
        if let Escape::Call(callee) = escape
            && assembly.contains(&callee)
        {
            let callee = module.function(callee);
            err = err.note(if callee.attributes.is_function_pointer_dispatcher {
                "inline assembly may have retyped this function pointer, so its callee may take \
                 or return a reference as a plain word"
                    .to_string()
            } else {
                format!("`{}` contains inline assembly, which may store any word", callee.name)
            });
        }
        err.help("keep only values past the block, or remove the tag").emit();
    }
}

/// Folds addresses in one immutable function without changing executable MIR. Cache both
/// constants and unknown values so shared expressions stay shared, including across queries.
/// An explicit stack avoids recursion on deep expressions. Cycles and nonconstant operands are
/// unknown; evaluation uses the same widths and wrapping semantics as the optimizer.
struct ConstantWords<'a> {
    func: &'a Function,
    /// `None` is unvisited; `Some(None)` is pending or known to be nonconstant.
    values: IndexVec<ValueId, Option<Option<U256>>>,
}

impl<'a> ConstantWords<'a> {
    fn new(func: &'a Function) -> Self {
        Self { func, values: index_vec![None; func.num_values()] }
    }

    fn get(&mut self, root: ValueId) -> Option<U256> {
        if let Some(value) = self.values[root] {
            return value;
        }
        self.values[root] = Some(None);
        let mut pending = vec![root];
        while let Some(&value) = pending.last() {
            let result = if let Some(word) = self.func.value_u256(value) {
                Ok(Some(word))
            } else if let Value::Inst(inst) = *self.func.value(value) {
                crate::mir::utils::eval::eval_typed_inst(
                    self.func,
                    &self.func.inst(inst).kind,
                    |operand| match self.values[operand] {
                        Some(word) => word.ok_or(None),
                        None => Err(Some(operand)),
                    },
                )
            } else {
                Ok(None)
            };
            match result {
                Err(Some(operand)) => {
                    // Mark the dependency before visiting it so a cycle stays unknown.
                    self.values[operand] = Some(None);
                    pending.push(operand);
                }
                result => {
                    self.values[value] = Some(result.ok().flatten());
                    pending.pop();
                }
            }
        }
        self.values[root].flatten()
    }
}

/// A word whose contents the last memory write establishes.
#[derive(Clone, Copy)]
enum LoadedWord {
    /// The value of a full word store.
    Value(ValueId),
    /// The constant header length of a static ABI encoding, which carries no pointer.
    EncodingLength,
}

/// The value a word load reads from the last memory write in its block. Following its provenance
/// avoids treating a stored scalar as a new pointer. The address must be disjoint from the FMP
/// word: a scalar stored there becomes the allocation address. Partial writes, calls and other
/// memory writes discard the previous store, and no fact crosses a block boundary.
fn stored_load_values(function: &Analyzed) -> FxHashMap<InstId, LoadedWord> {
    let func = &function.func;
    let mut loads = FxHashMap::default();
    let mut constants = ConstantWords::new(func);
    for block in &func.blocks {
        let mut stored = None;
        for &inst in &block.instructions {
            let kind = &func.inst(inst).kind;
            if let InstKind::Load(address, PtrLocation::Memory) = *kind
                && let Some((destination, value)) = stored
                && address == destination
                && func.inst(inst).result_ty
                    == match value {
                        LoadedWord::Value(value) => func.value_ty(value),
                        LoadedWord::EncodingLength => Some(MirType::I256),
                    }
                && (matches!(value, LoadedWord::EncodingLength)
                    || !reads_free_pointer(function, &mut constants, inst))
            {
                loads.insert(inst, value);
            }
            if function.aa.instruction_mod_ref(func, inst).writes_space(AddressSpace::Memory) {
                stored = None;
            }
            if let InstKind::Store(address, value, PtrLocation::Memory) = *kind {
                stored = Some((address, LoadedWord::Value(value)));
            }
            if let InstKind::AbiEncode { mode: AbiEncodeMode::Bytes, layout, .. } = kind
                && !layout.types.iter().any(AbiType::is_dynamic)
                && let Some(object) = func.inst_result_value(inst)
            {
                stored = Some((object, LoadedWord::EncodingLength));
            }
        }
    }
    loads
}

/// Raw copies and calls may retain the FMP bytes without ever loading them into an SSA value.
/// Calls can retain their input in returndata or external state even with an empty output range.
/// Hashes and other read-only operations do not carry the pointer into accessible memory.
fn captures_free_memory(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    inst: InstId,
) -> Stored {
    let kind = &constants.func.inst(inst).kind;
    let mut captures = matches!(kind.effect_kind(), EffectKind::ExternalCall | EffectKind::Create);
    let mut reads_free = Stored::default();
    kind.visit_raw_memory(|access, address, size| {
        let size = match size {
            RawMemorySize::Bytes(size) => Some(U256::from(size)),
            RawMemorySize::Value(size) => constants.get(size),
        };
        if size == Some(U256::ZERO) {
            return;
        }
        if access != RawMemoryAccess::Read {
            captures = true;
        } else if function.overlaps_free_pointer(constants, address, size)
            && function.fresh_start(address).is_none()
        {
            reads_free.add(&fmp_access_base(function, constants, address, true));
        }
    });
    if !captures {
        reads_free = Stored::default();
    }
    if let InstKind::ICall { function: Callee::Function(callee), args } = kind
        && let Some(summary) = summaries.get(callee)
    {
        reads_free.any |= summary.captures_free.any;
        for param in &summary.captures_free.params {
            let Some(&address) = args.get(param.index()) else {
                reads_free.any = true;
                continue;
            };
            // A bounded compiler allocation cannot wrap to the slot under a nonnegative u64 offset.
            // Raw ranges extending past the address space fail rather than wrap.
            if !function.read_start_above_fmp(address) {
                reads_free.add(&fmp_access_base(function, constants, address, true));
            }
        }
    }
    reads_free
}

fn range_overlaps_free_pointer(address: Option<U256>, size: Option<U256>) -> bool {
    if size == Some(U256::ZERO) {
        return false;
    }
    let Some(address) = address else { return true };
    let start = U256::from(EvmMemoryLayout::FMP_SLOT);
    let end = start + U256::from(EvmMemoryLayout::WORD_SIZE);
    // Compare the distance to the slot rather than adding a possibly wrapping size.
    address < end && (address >= start || size.is_none_or(|size| size > start - address))
}

/// A function prepared for the scratch check: a copy whose trivial phis are resolved, with its
/// alias analysis.
///
/// Lowering gives a loop header a phi for every variable in scope, including the ones the loop
/// never assigns. The alias analysis cannot see through a phi that merges a pointer with
/// itself, so every write through such a variable would reach anything.
pub(super) struct Analyzed {
    func: Function,
    aa: AliasAnalysis,
    /// The reads of the free memory pointer that no reset of it may precede.
    fresh_fmps: FxHashSet<InstId>,
    /// Whether this function may directly or indirectly receive forged memory references. Raw
    /// assembly stays conservative; typed accesses can trust the explicit memory-safety promise
    /// only when no unannotated block or reference-retyping dispatcher undermines it.
    forged: bool,
}

impl Analyzed {
    fn new(func: &Function, calls: &Arc<MemoryCallSummaries>, forged: bool) -> Self {
        let mut func = func.clone();
        simplify_trivial_phis(&mut func);
        let aa = AliasAnalysis::with_call_summaries(&func, Arc::clone(calls));
        let fresh_fmps = fresh_fmps(&func, &aa);
        Self { func, aa, fresh_fmps, forged }
    }

    /// Whether an access of `size` bytes at `address` may touch the free memory pointer's slot.
    /// Raw memory ranges do not wrap; an access past the address space fails instead, and
    /// zero-length accesses do not inspect their address.
    fn overlaps_free_pointer(
        &self,
        constants: &mut ConstantWords<'_>,
        address: ValueId,
        size: Option<U256>,
    ) -> bool {
        let address = constants.get(address);
        (address.is_some() || self.forged) && range_overlaps_free_pointer(address, size)
    }

    /// The instruction `value` is the result of, when no object that exists before it runs lies
    /// at or past `value`: a fresh allocation, or a read of the free memory pointer that no reset
    /// of it may precede.
    ///
    /// The alias analysis bases a write past a fresh allocation's known extent, or anywhere past
    /// the free memory pointer, on that value. Such a write may reach objects allocated after the
    /// instruction, but none that existed before it.
    fn fresh_start(&self, value: ValueId) -> Option<InstId> {
        let Value::Inst(inst) = *self.func.value(value) else { return None };
        if self.fresh_fmps.contains(&inst) {
            return Some(inst);
        }
        match self.aa.memory_address(&self.func, value)?.base {
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) if site == inst => {
                Some(site)
            }
            _ => None,
        }
    }

    /// The object `pointer` points into: the pointer it offsets, through every `ptradd` and
    /// pointer cast.
    ///
    /// Lowering emits `ptradd` only for addresses inside the object its base points to, so the
    /// base names the object a store through `pointer` writes.
    fn object(&self, mut pointer: ValueId) -> ValueId {
        while let Value::Inst(inst) = *self.func.value(pointer) {
            pointer = match self.func.inst(inst).kind {
                InstKind::PtrAdd(base, _)
                | InstKind::IntToPtr(base)
                | InstKind::PtrToInt(base, 256) => base,
                _ => break,
            };
        }
        pointer
    }

    /// Whether a raw write can install an allocation address. The heap-floor pass clamps a
    /// direct FMP load used as a memory address above reserved slots, even after an assembly
    /// reset. An offset from that load can wrap, so only unoffset loads and casts qualify.
    /// Constant offsets from fresh allocations stay above reserved slots; dynamic ones can wrap.
    fn writes_free_pointer(
        &self,
        constants: &mut ConstantWords<'_>,
        address: ValueId,
        size: Option<U256>,
    ) -> bool {
        let mut base = address;
        let mut offset = 0_u64;
        while let Value::Inst(inst) = *self.func.value(base) {
            match self.func.inst(inst).kind {
                InstKind::IntToPtr(value) | InstKind::PtrToInt(value, 256) => base = value,
                InstKind::PtrAdd(value, delta) => {
                    let Some(next) =
                        self.func.value_u64(delta).and_then(|delta| offset.checked_add(delta))
                    else {
                        break;
                    };
                    offset = next;
                    base = value;
                }
                InstKind::Load(slot, PtrLocation::Memory)
                    if offset == 0
                        && self.func.value_u64(slot) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    return false;
                }
                _ => break,
            }
        }
        self.overlaps_free_pointer(constants, address, size) && self.fresh_start(base).is_none()
    }

    /// A bounded compiler allocation starts above reserved slots even if an FMP reset makes it
    /// alias an older object. Unlike `fresh_start`, this proves only disjointness from the FMP
    /// slot, not uniqueness. Do not follow offsets: their addition may wrap before the read.
    fn read_start_above_fmp(&self, address: ValueId) -> bool {
        self.fresh_start(address).is_some()
            || matches!(*self.func.value(address), Value::Inst(inst)
                if matches!(self.func.inst(inst).kind,
                    InstKind::AbiEncode { mode: AbiEncodeMode::Bytes, .. }
                        | InstKind::AbiEncodePacked { hash: false, .. }))
    }
}

/// The reads of the free memory pointer in `func` that no instruction that may reset it precedes
/// on any path.
fn fresh_fmps(func: &Function, aa: &AliasAnalysis) -> FxHashSet<InstId> {
    let mut fresh = FxHashSet::default();
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Fmp)) {
        return fresh;
    }
    let mut resets = DenseBitSet::new_empty(func.blocks.len());
    for (id, block) in func.blocks.iter_enumerated() {
        if block.instructions.iter().any(|&inst| aa.instruction_may_reset_fmp(func, inst)) {
            resets.insert(id);
        }
    }
    // Blocks some path into which may reset the free memory pointer.
    let mut reached = DenseBitSet::new_empty(func.blocks.len());
    let mut poisoned = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = vec![BlockId::ENTRY];
    reached.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop() {
        let out = poisoned.contains(block) || resets.contains(block);
        let Some(terminator) = &func.blocks[block].terminator else { continue };
        for successor in terminator.successors() {
            let mut changed = reached.insert(successor);
            if out {
                changed |= poisoned.insert(successor);
            }
            if changed {
                worklist.push(successor);
            }
        }
    }
    for (block, data) in func.blocks.iter_enumerated() {
        if !reached.contains(block) {
            continue;
        }
        let mut reset = poisoned.contains(block);
        for &inst in &data.instructions {
            if !reset && matches!(func.inst(inst).kind, InstKind::Fmp) {
                fresh.insert(inst);
            }
            reset |= aa.instruction_may_reset_fmp(func, inst);
        }
    }
    fresh
}

/// Values installed as allocation addresses by raw stores or internal calls. Follow pure inputs
/// and forwarded loads backwards too: an alias may predate the store and need not reload the FMP.
/// A full-word constant below the heap start is raised before allocation by the heap-floor pass;
/// partial and unknown writes are instead rejected by `unknown_fmp_write`.
fn installed_free_pointers(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    loads: &FxHashMap<InstId, LoadedWord>,
    instructions: impl Iterator<Item = InstId>,
) -> FxHashSet<ValueId> {
    let func = &function.func;
    let mut stack = Vec::new();
    for inst in instructions {
        match &func.inst(inst).kind {
            InstKind::Store(address, value, PtrLocation::Memory) => {
                if constants.get(*address) == Some(U256::from(EvmMemoryLayout::FMP_SLOT))
                    && constants
                        .get(*value)
                        .is_none_or(|word| word >= U256::from(EvmMemoryLayout::HEAP_START))
                {
                    stack.push(*value);
                }
            }
            InstKind::ICall { function: Callee::Function(callee), args } => {
                stack.extend(summaries.get(callee).into_iter().flat_map(|summary| {
                    summary
                        .installs_free
                        .iter()
                        .filter_map(|index| args.get(index.index()))
                        .copied()
                }))
            }
            _ => {}
        }
    }
    let mut installed = FxHashSet::default();
    while let Some(value) = stack.pop() {
        if !installed.insert(value) {
            continue;
        }
        let Value::Inst(inst) = *func.value(value) else { continue };
        if let Some(&LoadedWord::Value(stored)) = loads.get(&inst) {
            stack.push(stored);
        } else if func.inst(inst).kind.effect_kind() == EffectKind::Pure {
            stack.extend(func.inst(inst).operands());
        } else if let InstKind::ICall { function: Callee::Function(_), args } =
            &func.inst(inst).kind
        {
            // A returned word may forward an argument without reading the FMP itself.
            stack.extend(args);
        }
    }
    installed
}

/// Instructions whose unknown FMP write may reach `boundary`, or a function exit. A definite
/// full-word replacement kills earlier writes, except those read to compute its replacement.
/// Partial writes, allocation bumps and scratch restores retain the original provenance.
fn reaching_fmp_writes(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    boundary: Option<(BlockId, usize)>,
) -> DenseBitSet<InstId> {
    let func = &function.func;
    let mut predecessors: IndexVec<BlockId, Vec<BlockId>> =
        index_vec![Vec::new(); func.blocks.len()];
    let mut positions = index_vec![None; func.num_insts()];
    for (block, data) in func.blocks.iter_enumerated() {
        for (index, &inst) in data.instructions.iter().enumerate() {
            positions[inst] = Some((block, index));
        }
        for successor in data.terminator.iter().flat_map(Terminator::successors) {
            predecessors[successor].push(block);
        }
    }
    let mut stack = match boundary {
        Some(boundary) => vec![boundary],
        None => func
            .blocks
            .iter_enumerated()
            .filter(|(_, block)| {
                block.terminator.iter().flat_map(Terminator::successors).next().is_none()
            })
            .map(|(block, data)| (block, data.instructions.len()))
            .collect(),
    };
    let mut reaching = DenseBitSet::new_empty(func.num_insts());
    let mut visited = DenseBitSet::new_empty(func.blocks.len());
    let mut sources = FxHashSet::default();
    while let Some((block, end)) = stack.pop() {
        let mut replaced = false;
        for &inst in func.blocks[block].instructions[..end].iter().rev() {
            reaching.insert(inst);
            if let InstKind::Store(address, value, PtrLocation::Memory) = func.inst(inst).kind
                && constants.get(address) == Some(U256::from(EvmMemoryLayout::FMP_SLOT))
            {
                let mut values = vec![value];
                while let Some(value) = values.pop() {
                    if !sources.insert(value) {
                        continue;
                    }
                    let Value::Inst(read) = *func.value(value) else { continue };
                    let kind = &func.inst(read).kind;
                    if kind.effect_kind() == EffectKind::Pure {
                        values.extend(kind.operands());
                    } else if reads_free_pointer(function, constants, read)
                        || matches!(kind, InstKind::ICall { function: Callee::Function(_), .. })
                    {
                        let Some((block, index)) = positions[read] else { continue };
                        // Calls can install the value they return as well as read the old slot.
                        let end = index + usize::from(matches!(kind, InstKind::ICall { .. }));
                        stack.push((block, end));
                    }
                }
                replaced = true;
                break;
            }
        }
        if !replaced {
            for &predecessor in &predecessors[block] {
                if visited.insert(predecessor) {
                    stack.push((predecessor, func.blocks[predecessor].instructions.len()));
                }
            }
        }
    }
    reaching
}

/// A write whose installed FMP value is unknown. A callee's write through a parameter is harmless
/// when the caller passes a fresh object; otherwise keep the destination's parameter provenance.
/// Full aligned word stores have explicit value roots instead. Partial stores and copies do not.
fn unknown_fmp_write(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    inst: InstId,
) -> Stored {
    let func = &function.func;
    let kind = &func.inst(inst).kind;
    let mut unknown = Stored::default();
    if let InstKind::Store(address, _, PtrLocation::Memory) = *kind
        && constants.get(address) == Some(U256::from(EvmMemoryLayout::FMP_SLOT))
    {
        return unknown;
    }
    kind.visit_raw_memory(|access, address, size| {
        let size = match size {
            RawMemorySize::Bytes(size) => Some(U256::from(size)),
            RawMemorySize::Value(size) => constants.get(size),
        };
        if access != RawMemoryAccess::Read && function.writes_free_pointer(constants, address, size)
        {
            unknown.add(&fmp_access_base(function, constants, address, false));
        }
    });
    if let InstKind::ICall { function: Callee::Function(callee), args } = kind
        && let Some(summary) = summaries.get(callee)
    {
        unknown.any |= summary.unknown_fmp.any;
        for param in &summary.unknown_fmp.params {
            let Some(&address) = args.get(param.index()) else {
                unknown.any = true;
                continue;
            };
            // A bounded fresh object cannot wrap to the slot under a constant u64 offset.
            if function.fresh_start(address).is_none() {
                unknown.add(&fmp_access_base(function, constants, address, false));
            }
        }
    }
    unknown
}

/// The parameter an access offsets by nonnegative u64 constants. No bound is inferred: a caller
/// must supply a bounded compiler allocation before the conditional overlap can be discharged.
/// Other origins or arithmetic keep no conditional proof. Raw read ranges also allow full-width
/// integer adds.
fn fmp_access_base(
    function: &Analyzed,
    constants: &mut ConstantWords<'_>,
    mut address: ValueId,
    integer_offsets: bool,
) -> Stored {
    let func = &function.func;
    let mut offset = 0_u64;
    loop {
        match *func.value(address) {
            Value::Arg(param) => {
                return Stored { params: SmallVec::from_slice(&[param]), any: false };
            }
            Value::Inst(inst) => match func.inst(inst).kind {
                InstKind::PtrToInt(value, 256) | InstKind::IntToPtr(value) => address = value,
                InstKind::PtrAdd(base, delta) => {
                    let Some(next) = constants
                        .get(delta)
                        .and_then(|word| u64::try_from(word).ok())
                        .and_then(|delta| offset.checked_add(delta))
                    else {
                        break;
                    };
                    address = base;
                    offset = next;
                }
                InstKind::Add(first, second)
                    if integer_offsets && func.value_ty(address) == Some(MirType::I256) =>
                {
                    let base = constants
                        .get(second)
                        .map(|delta| (first, delta))
                        .or_else(|| constants.get(first).map(|delta| (second, delta)));
                    let Some((base, next)) = base.and_then(|(base, delta)| {
                        Some((base, offset.checked_add(u64::try_from(delta).ok()?)?))
                    }) else {
                        break;
                    };
                    address = base;
                    offset = next;
                }
                _ => break,
            },
            _ => break,
        }
    }
    Stored { params: SmallVec::new(), any: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_words_share_deep_dependencies() {
        let mut func = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut func);
        let mut address = builder.imm(0);
        // address = add address, address
        for _ in 0..20_000 {
            address = builder.add(address, address);
        }
        // address = add address, 64
        let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
        let address = builder.add(address, slot);

        let mut constants = ConstantWords::new(&func);
        assert_eq!(constants.get(address), Some(U256::from(EvmMemoryLayout::FMP_SLOT)));
        assert_eq!(constants.get(address), constants.get(slot));
    }

    #[test]
    fn constant_words_keep_widths_and_unknowns() {
        let mut func = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut func);
        let input = builder.add_param(MirType::I256);
        let max = builder.imm(U256::MAX);
        let one = builder.imm(1);
        // wrapped = add max, 1
        // narrow = trunc i256 max to i160
        // word = zext i160 narrow to i256
        // unknown = add input, wrapped
        let wrapped = builder.add(max, one);
        let narrow = builder.cast(max, MirType::I160);
        let word = builder.cast(narrow, MirType::I256);
        let unknown = builder.add(input, wrapped);
        let header = builder.create_block();
        let latch = builder.create_block();
        builder.switch_to_block(header);
        // cycle = phi [entry: 0], [latch: cycle]
        let cycle = builder.phi(vec![(BlockId::ENTRY, wrapped)]);
        builder.add_phi_incoming(cycle, latch, cycle);

        let mut constants = ConstantWords::new(&func);
        assert_eq!(constants.get(unknown), None);
        assert_eq!(constants.get(cycle), None);
        assert_eq!(constants.get(wrapped), Some(U256::ZERO));
        assert_eq!(constants.get(word), Some(U256::MAX >> 96));
        assert_eq!(constants.get(unknown), None);
    }

    #[test]
    fn free_pointer_overlap_handles_empty_and_huge_ranges() {
        for (address, size, overlaps) in [
            (Some(0_u64), Some(64_u64), false),
            (Some(0), Some(65), true),
            (Some(32), Some(32), false),
            (Some(33), Some(32), true),
            (Some(64), Some(0), false),
            (Some(95), Some(1), true),
            (Some(96), Some(1), false),
            (Some(0), None, true),
            (Some(96), None, false),
            (None, Some(0), false),
            (None, Some(1), true),
        ] {
            assert_eq!(
                range_overlaps_free_pointer(address.map(U256::from), size.map(U256::from)),
                overlaps
            );
        }
        assert!(range_overlaps_free_pointer(Some(U256::ZERO), Some(U256::MAX)));
        assert!(!range_overlaps_free_pointer(Some(U256::MAX), Some(U256::MAX)));
    }
}
