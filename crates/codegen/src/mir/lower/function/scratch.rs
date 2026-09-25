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
//! encodings and storage reads that allocate, of its calls that return memory, the pointers
//! loaded from that memory, and every value derived from those by pure operations. The check
//! rejects a use of one after the block, a `return` of one, a store of one into memory the block
//! did not allocate or into storage, and a call in the block whose callee may store a reference
//! into a parameter's object the block did not allocate, or anywhere else that outlives the call.
//! Callees are summarized over the call graph to a fixed point, and a callee with inline
//! assembly may store anything. Type checking already rejected inline assembly in the block.
//! Scalars, such as hashes, lengths, and loaded words, leave the block freely.
//!
//! NOTE: a callee that stores a pointer into a parameter's object is rejected even when the
//! pointer it stores refers to memory older than the block. Reading `msize` after the block sees
//! the memory the block used, as it would without reuse.

use super::{memory_facts::Analyzed, *};
use crate::mir::{
    ArgIdx, Callee, EffectKind, InstId, Module, Terminator,
    analysis::{MemoryBase, MemoryCallSummaries},
};
use solar_data_structures::smallvec::SmallVec;

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
        // A return variable's default object is made where it is first used, and it outlives
        // the block, so it is made before the block starts.
        let mut pending = self
            .default_bindings
            .iter()
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
        let open = self
            .builder
            .func()
            .blocks
            .iter()
            .map(|block| block.terminator.is_none())
            .collect::<Vec<_>>();
        self.lower_block(block)?;
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
            .filter(|&(id, block)| {
                block.terminator.is_some() && open.get(id.index()).copied().unwrap_or(true)
            })
            .map(|(id, _)| id)
            .collect();
        self.cx.state.scratch_regions.push(ScratchRegion { saved, start, end, terminated, tag });
        Some(())
    }
}

/// Rejects every reference to the memory of a `@custom:solar-scratch` block that code after the
/// block could still use.
pub(in crate::mir::lower) fn check_scratch_regions(
    gcx: Gcx<'_>,
    module: &Module,
    regions: &[(FunctionId, ScratchRegion)],
) {
    if regions.is_empty() {
        return;
    }
    let calls = Arc::new(MemoryCallSummaries::new(module));
    let summaries =
        PointerWrites::compute(module, &calls, regions.iter().map(|&(function, _)| function));
    // The driver claims each function's regions together.
    for group in regions.chunk_by(|a, b| a.0 == b.0) {
        let function = Analyzed::new(module.function(group[0].0), &calls);
        let saved = group.iter().map(|(_, region)| region.saved).collect::<FxHashSet<_>>();
        for (_, region) in group {
            check_region(gcx, module, &function, &summaries, &saved, region);
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
}

fn check_region(
    gcx: Gcx<'_>,
    module: &Module,
    function: &Analyzed,
    summaries: &FxHashMap<FunctionId, PointerWrites>,
    saved: &FxHashSet<ValueId>,
    region: &ScratchRegion,
) {
    let func = &function.func;
    let inside = |inst: InstId| region.start <= inst && inst < region.end;
    // Trivial phis stand for the value they merge and are not uses of their own.
    let mut users = FxHashMap::<ValueId, Vec<InstId>>::default();
    let mut terminators = FxHashMap::<ValueId, Vec<BlockId>>::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            if func.inst_result_value(inst).is_some_and(|result| function.resolve(result) != result)
            {
                continue;
            }
            for operand in func.inst(inst).operands() {
                users.entry(operand).or_default().push(inst);
            }
        }
        if let Some(terminator) = &data.terminator {
            terminator.for_each_operand(|operand| {
                terminators.entry(operand).or_default().push(block);
            });
        }
    }

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
    let mut escapes = Vec::new();
    while let Some((value, was_late)) = stack.pop() {
        for &block in terminators.get(&value).into_iter().flatten() {
            let span = func.blocks[block].terminator_metadata.source_span();
            if was_late || !region.terminated.contains(&block) {
                escapes.push((span, Escape::Outlives));
            } else if matches!(func.blocks[block].terminator, Some(Terminator::Return { .. })) {
                escapes.push((span, Escape::Returned));
            }
        }
        for &inst in users.get(&value).into_iter().flatten() {
            let instruction = func.inst(inst);
            let kind = &instruction.kind;
            let is_late = was_late || !inside(inst);
            let result = func.inst_result_value(inst);
            let derived = if kind.effect_kind() == EffectKind::Pure {
                // A length, a comparison, or a scalar field of a reference is a value.
                match kind {
                    InstKind::SliceLen(_) => false,
                    InstKind::ExtractValue { .. } => {
                        result.is_some_and(|result| is_pointer(func.value_ty(result)))
                    }
                    _ => result.is_some_and(|result| may_hold_pointer(func.value_ty(result))),
                }
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
                && destination.is_none_or(|destination| !references.contains(&destination))
            {
                escapes.push((instruction.metadata.source_span(), Escape::Stored));
            }
        }
    }

    // A call in the block may store a reference to memory it or the block allocated.
    for inst in func.instructions().filter(|&inst| inside(inst)) {
        let InstKind::ICall { function: Callee::Function(callee), args } = &func.inst(inst).kind
        else {
            continue;
        };
        let Some(writes) = summaries.get(callee) else { continue };
        let reaches_outside = writes.other
            || writes
                .params
                .iter()
                .any(|param| args.get(param.index()).is_none_or(|arg| !references.contains(arg)));
        if reaches_outside {
            escapes.push((func.inst(inst).metadata.source_span(), Escape::Call(*callee)));
        }
    }

    let mut reported = FxHashSet::default();
    for (span, escape) in escapes {
        let span = span.unwrap_or(region.tag);
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
        };
        let mut err = gcx
            .dcx()
            .err(message)
            .span(span)
            .span_note(region.tag, "the memory this block allocates is reused after it");
        if let Escape::Call(callee) = escape
            && module.function(callee).attributes.inline_assembly
        {
            err = err.note(format!(
                "`{}` contains inline assembly, which may store any word",
                module.function(callee).name
            ));
        }
        err.help("keep only values past the block, or remove the tag").emit();
    }
}

/// Whether `inst` allocates the memory its result refers to.
fn allocates(func: &Function, inst: InstId) -> bool {
    match &func.inst(inst).kind {
        InstKind::Alloc { .. }
        | InstKind::Fmp
        | InstKind::AbiEncode { .. }
        | InstKind::AbiEncodePacked { .. }
        | InstKind::AbiDecode { .. }
        | InstKind::StorageBytesLoad(..)
        | InstKind::StorageArrayLoad { .. }
        | InstKind::ICall {
            function:
                Callee::Builtin(crate::mir::Builtin::Concat(_) | crate::mir::Builtin::ReturndataBytes),
            ..
        } => true,
        // A callee's result may be memory it allocated in the block. Without inline assembly,
        // which rejects the call anyway, a callee cannot turn a pointer into a word.
        InstKind::ICall { function: Callee::Function(_), .. } => {
            func.inst_result_value(inst).is_some_and(|result| is_pointer(func.value_ty(result)))
        }
        _ => false,
    }
}

/// Whether a value of type `ty` can carry a memory pointer: a reference, an aggregate that may
/// hold one, or a raw word, which a pointer cast produces.
fn may_hold_pointer(ty: Option<MirType>) -> bool {
    match ty {
        Some(MirType::I256 | MirType::MemPtr | MirType::MemoryObject(_) | MirType::Struct(_)) => {
            true
        }
        Some(MirType::Slice(location)) => location == SliceLocation::Memory,
        _ => false,
    }
}

/// Whether `ty` is a typed memory reference, rather than a word that may only be data.
fn is_pointer(ty: Option<MirType>) -> bool {
    matches!(
        ty,
        Some(
            MirType::MemPtr
                | MirType::MemoryObject(_)
                | MirType::Struct(_)
                | MirType::Slice(SliceLocation::Memory)
        )
    )
}

fn is_load(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::MLoad(_)
            | InstKind::MemoryObjectLoadField { .. }
            | InstKind::MemoryObjectLoadElement { .. }
    )
}

/// The destination and stored value of a store: the memory address or object written, or `None`
/// for storage and immutables, which outlive the call.
fn store_operands(kind: &InstKind) -> Option<(Option<ValueId>, ValueId)> {
    match *kind {
        InstKind::MStore(address, value) | InstKind::MStore8(address, value) => {
            Some((Some(address), value))
        }
        InstKind::MemoryObjectStoreField { object, value, .. }
        | InstKind::MemoryObjectStoreElement { object, value, .. }
        | InstKind::MemoryObjectStoreByte { object, value, .. }
        | InstKind::MemoryObjectStoreWord { object, value, .. } => Some((Some(object), value)),
        InstKind::SStore(_, value)
        | InstKind::TStore(_, value)
        | InstKind::StoreImmutable(_, value) => Some((None, value)),
        _ => None,
    }
}

/// The parameters into whose objects a function may store a memory pointer, and whether it may
/// store one anywhere else that exists when it is entered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PointerWrites {
    params: SmallVec<[ArgIdx; 2]>,
    other: bool,
}

impl PointerWrites {
    /// Summarizes every function the `roots` may call, to a fixed point over the call graph.
    fn compute(
        module: &Module,
        calls: &Arc<MemoryCallSummaries>,
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
            .map(|&id| (id, Analyzed::new(module.function(id), calls)))
            .collect::<FxHashMap<_, _>>();
        let mut summaries =
            order.iter().map(|&id| (id, Self::default())).collect::<FxHashMap<_, _>>();
        // Summaries only grow, so the iteration ends.
        loop {
            let mut changed = false;
            for &id in &order {
                let summary = Self::of(&analyzed[&id], &summaries);
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

    /// Summarizes a function given the summaries of its callees.
    fn of(function: &Analyzed, summaries: &FxHashMap<FunctionId, Self>) -> Self {
        let func = &function.func;
        let mut writes = Self { other: func.attributes.inline_assembly, ..Self::default() };
        // Where a store through `address` lands: this call's own memory, a parameter's object, or
        // memory older than the call.
        let record = |writes: &mut Self, address: ValueId| {
            let Some(address) = function.aa.memory_address(func, address) else {
                writes.other = true;
                return;
            };
            match address.base {
                MemoryBase::InternalFrame
                | MemoryBase::Allocation(_)
                | MemoryBase::DynamicAllocation(_) => {}
                MemoryBase::Absolute => {
                    writes.other |= address.offset >= EvmMemoryLayout::HEAP_START;
                }
                MemoryBase::Param(value) | MemoryBase::Value(value) => match *func.value(value) {
                    Value::Arg(index) => {
                        if !writes.params.contains(&index) {
                            writes.params.push(index);
                            writes.params.sort_unstable();
                        }
                    }
                    _ if function.fresh_start(value).is_some() => {}
                    _ => writes.other = true,
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
                        InstKind::SliceLen(_) => false,
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
        for inst in func.instructions() {
            let kind = &func.inst(inst).kind;
            if let Some((destination, value)) = store_operands(kind)
                && pointers.contains(&value)
            {
                match destination {
                    Some(destination) => record(&mut writes, destination),
                    None => writes.other = true,
                }
            }
            if let InstKind::ICall { function: Callee::Function(callee), args } = kind
                && let Some(callee) = summaries.get(callee)
            {
                writes.other |= callee.other;
                for param in &callee.params {
                    match args.get(param.index()) {
                        Some(&arg) => record(&mut writes, arg),
                        None => writes.other = true,
                    }
                }
            }
        }
        writes
    }
}
