//! `@custom:solar-view` declarations: `Bytes.slice` ranges and decoded bytes read in place.
//!
//! The declarations
//!
//! ```solidity
//! /// @custom:solar-view
//! bytes memory v = Bytes.slice(source, offset, count);
//! /// @custom:solar-view
//! (uint256 id, bytes memory payload) = abi.decode(data, (uint256, bytes));
//! ```
//!
//! mean what they mean to every compiler: `v` and `payload` hold copies, made after the checks
//! the copies make. Other compilers read the tag as documentation and make the copies. This
//! compiler instead binds each view to the slice of its bytes where they are and never copies,
//! which is only equivalent while those bytes cannot change between the declaration and a read
//! of the view, and while nothing can tell the view from a copy. Both conditions are checked, and
//! a program that breaks either is rejected rather than compiled with different behavior.
//!
//! A decode declaration may decode value types, `bytes`, and `string`, from `bytes` in memory or
//! calldata or from another view; its `bytes` and `string` values are the views, and a view of
//! calldata needs no borrow, since nothing can change calldata. The decode validates its input
//! exactly as the copying decode does, including the allocation checks each copy would make
//! (`Panic(0x41)` for a length that cannot be allocated), so every input fails where it would.
//!
//! A view itself can only be read in place: `.length`, indexing, `keccak256`, `abi.decode`, and
//! the reads of `solar:core/v1/Bytes.sol` and `Hash.sol` that take a range (a view of a view
//! included). Any other use, such as an assignment, a write through the view, passing it to a
//! function or returning it, is an error at the use, because it could keep the view or write the
//! source through it. A view of a view narrows the same bytes, so it extends the enclosing view's
//! borrow instead of starting one.
//!
//! Once the contract is lowered, and before any optimization, [`check_view_borrows`] rejects
//! every instruction that may write the source's payload between the view's creation and a later
//! read through it, on some path that does not create the view again. The reads are the uses of
//! every value the view's slice derives through pure operations, other than its length, which the
//! view fixed when it was made. Writes come from the MIR ModRef analysis, run on a copy of the
//! function whose trivial loop phis are resolved. A call writes whatever its callee may write of
//! the memory that exists when it is entered: the objects of some parameters, or anything,
//! summarized over the call graph to a fixed point.
//!
//! A write cannot reach the source's payload when it lands in the reserved words below the heap,
//! in an internal-call frame, or in memory allocated after the source existed: past any fresh
//! allocation of the function, or past the free memory pointer, when the source is a parameter's
//! object, and otherwise past one the view's creation dominates. A write into a parameter's object
//! cannot reach a source allocated by the function either, as long as it is a semantic object
//! store, which stays inside its object; a raw store may run past the object's end.
//!
//! NOTE: the check relies on the Solidity memory model: objects a variable can reach lie in
//! allocated memory, so an allocation that no free-memory-pointer reset precedes cannot overlap
//! one that already exists. It is conservative wherever the analysis loses a write's target, such
//! as a raw store at a computed offset, and then reports the write. Without intrinsic lowering
//! (`-Zno-core-intrinsics`) the declaration lowers as the copy and nothing is checked.

use super::{
    memory_facts::{Analyzed, below_heap},
    *,
};
use crate::mir::{
    ArgIdx, Callee, EffectKind, InstId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, AliasResult, CfgInfo, Location, LocationSize,
        MemoryBase, MemoryCallSummaries, MemoryLocation,
    },
    memory::MemoryLayoutPolicy,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    smallvec::SmallVec,
};

/// A view whose source's bytes must not change while the view is still read.
pub(in crate::mir::lower) struct ViewBorrow {
    /// The view's memory slice.
    view: ValueId,
    /// The `bytes memory` object the view reads.
    source: ValueId,
    /// The view variable's declaration.
    span: Span,
    /// The view variable's name.
    name: Symbol,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Lowers the declaration of the `@custom:solar-view` variable `id`: the `Bytes.slice` call
    /// initializing it yields the memory slice of its range instead of a copy.
    pub(super) fn lower_view_declaration(
        &mut self,
        id: VariableId,
        initializer: &hir::Expr<'_>,
    ) -> Option<()> {
        let call = initializer.peel_parens().id;
        let previous = self.forming_view.replace((call, id));
        let value = self.lower_expr(initializer);
        self.forming_view = previous;
        let value = value?;
        if self.builder.func().value_slice_location(value).is_some() {
            self.views.insert(id, value);
        } else {
            // Without intrinsic lowering the call returned the copy the portable body makes.
            let ty = self.cx.gcx.type_of_item(id.into());
            let value = self.materialize_call_argument(ty, value, initializer.span)?;
            self.values.insert(id, value);
        }
        Some(())
    }

    /// Records that the `@custom:solar-view` variable `id` reads `range` of `bytes` in place.
    ///
    /// A view of a view narrows bytes the enclosing view already borrows, and its reads derive
    /// from that view's slice, so only a view of an object starts a borrow.
    pub(super) fn form_slice_view(&mut self, id: VariableId, range: ValueId, bytes: ValueId) {
        if self.builder.func().value_slice_location(bytes).is_some() {
            let root = self.view_roots.get(&bytes).copied().flatten();
            self.view_roots.insert(range, root);
            return;
        }
        self.view_roots.insert(range, Some(bytes));
        self.push_view_borrow(id, range, bytes);
    }

    /// Records that `view`, the slice of the view variable `id`, reads the object `source`.
    fn push_view_borrow(&mut self, id: VariableId, view: ValueId, source: ValueId) {
        let variable = self.cx.gcx.hir.variable(id);
        self.cx.state.view_borrows.push(ViewBorrow {
            view,
            source,
            span: variable.span,
            name: variable.name.map_or(kw::Empty, |name| name.name),
        });
    }

    /// Lowers a `@custom:solar-view` declaration of `ids` initialized by `abi.decode(data,
    /// (T...))`: value types decode as usual, and every `bytes` or `string` value is a view of
    /// its bytes in `data`, validated as the copying decode validates it.
    ///
    /// The data is decoded where it is: a calldata argument stays in calldata, where nothing can
    /// change it, and memory data, or a view's slice, is borrowed like any view's source.
    pub(super) fn lower_view_decode(
        &mut self,
        ids: &[Option<VariableId>],
        call: &hir::Expr<'_>,
    ) -> Option<()> {
        let ExprKind::Call(_, args, _) = &call.peel_parens().kind else {
            return self.cx.report_unsupported(call.span, "view decode");
        };
        let args = self.builtin_args::<2>(Builtin::AbiDecode, args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => return self.cx.report_unsupported(args[1].span, "abi.decode target type"),
        };
        let mut decoded = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) = self.cx.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind)
            else {
                return self.cx.report_unsupported(ty_expr.span, "abi.decode target type");
            };
            decoded.push(ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory));
        }
        if decoded.len() != ids.len() {
            return self.cx.report_unsupported(call.span, "view decode");
        }

        let data_expr = &args[0];
        let (data, root) = match self.view_operand(data_expr) {
            Some(view) => (view, self.view_roots.get(&view).copied().flatten()),
            None => {
                let data_ty = self.cx.gcx.type_of_expr(data_expr.id)?;
                if data_ty.is_ref_at(DataLocation::Calldata) {
                    let value = self.lower_expr(data_expr)?;
                    if self.builder.func().value_slice_location(value)
                        != Some(SliceLocation::Calldata)
                    {
                        return self.cx.report_unsupported(data_expr.span, "view decode data");
                    }
                    (value, None)
                } else {
                    let memory_ty = data_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                    let value = self.lower_typed_expr(data_expr, memory_ty)?;
                    let value =
                        self.materialize_memory_argument(memory_ty, value, data_expr.span)?;
                    (value, Some(value))
                }
            }
        };
        let location = match self.builder.func().value_slice_location(data) {
            Some(SliceLocation::Calldata) => SliceLocation::Calldata,
            _ => SliceLocation::Memory,
        };
        let layout = self.abi_decode_layout(&decoded, call.span)?;
        let fields = decoded
            .iter()
            .zip(&layout.types)
            .map(|(&ty, abi_type)| match abi_type {
                crate::mir::AbiParamType::Bytes => MirType::Slice(location),
                _ => types::TypeLowerer::mir_type(ty),
            })
            .collect::<Vec<_>>();
        let layout = self.cx.module.intern_abi_param_layout(layout);
        let result_ty = self.cx.module.intern_return_type(fields.clone())?;
        // result = abi_decode layout, data
        let result = self.builder.abi_decode(layout, data, result_ty);
        let values = match result_ty {
            MirType::Struct(id) => fields
                .iter()
                .enumerate()
                // value = extract_value result, index
                .map(|(index, &field)| self.builder.extract_value(id, result, index as u32, field))
                .collect::<Vec<_>>(),
            _ => vec![result],
        };
        for ((&id, value), field) in ids.iter().zip(values).zip(fields) {
            let Some(id) = id else { continue };
            if !matches!(field, MirType::Slice(_)) {
                self.values.insert(id, value);
                continue;
            }
            // Decoding ends what derives from the data, so each view borrows the data itself.
            self.views.insert(id, value);
            self.view_roots.insert(value, root);
            if let Some(root) = root {
                self.push_view_borrow(id, value, root);
            }
        }
        Some(())
    }

    /// Whether `initializer` of a `@custom:solar-view` declaration is an `abi.decode` whose
    /// `bytes` and `string` values become views. Without intrinsic lowering, the declaration
    /// decodes copies, like every view.
    pub(super) fn is_view_decode(&self, initializer: &hir::Expr<'_>) -> bool {
        !self.cx.gcx.sess.opts.unstable.no_core_intrinsics
            && matches!(
                initializer.peel_parens().kind,
                ExprKind::Call(callee, ..)
                    if self.cx.gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode)
            )
    }

    /// The slice `expr` reads when it names a `@custom:solar-view` variable, directly or through
    /// a conversion between `bytes` and `string`.
    pub(super) fn view_operand(&self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        if self.views.is_empty() {
            return None;
        }
        let expr = self.peel_bytes_conversion(expr);
        self.views.get(&self.cx.gcx.resolved_variable(expr)?).copied()
    }

    /// Lowers `expr`, or yields the slice of the `@custom:solar-view` variable it names.
    pub(super) fn lower_view_or_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        match self.view_operand(expr) {
            Some(view) => Some(view),
            None => self.lower_expr(expr),
        }
    }

    /// Reports a use of a `@custom:solar-view` variable that could keep it or write through it.
    pub(super) fn report_view_use<T>(&self, id: VariableId, span: Span) -> Option<T> {
        let name = self.cx.gcx.hir.variable(id).name.map_or(kw::Empty, |name| name.name);
        self.cx
            .gcx
            .dcx()
            .err(format!("the view `{name}` can only be read in place"))
            .span(span)
            .note(
                "a `@custom:solar-view` variable supports `.length`, indexing, `keccak256`, \
                 `abi.decode`, and the `Bytes` and `Hash` reads of a range",
            )
            .help("remove the tag to work with a copy of the bytes")
            .emit();
        None
    }
}

/// Rejects every write that may change bytes a `@custom:solar-view` variable still reads.
pub(in crate::mir::lower) fn check_view_borrows(
    gcx: Gcx<'_>,
    module: &Module,
    borrows: &[(FunctionId, ViewBorrow)],
) {
    if borrows.is_empty() {
        return;
    }
    // Which callees may reset the free memory pointer, and so end the freshness of the
    // allocations after a call to them.
    let calls = Arc::new(MemoryCallSummaries::new(module));
    let summaries =
        EntryWrites::compute(module, &calls, borrows.iter().map(|&(function, _)| function));
    // The driver claims each function's borrows together.
    for group in borrows.chunk_by(|a, b| a.0 == b.0) {
        let facts = FunctionFacts::new(module.function(group[0].0), &calls, &summaries);
        for (_, borrow) in group {
            facts.check(gcx, borrow);
        }
    }
}

/// Memory an instruction may write.
#[derive(Clone, Copy)]
enum Target {
    /// Anything, including memory that existed before the function was entered.
    Anything,
    /// A range whose base the alias analysis knows.
    Range {
        location: MemoryLocation,
        /// Whether the write stays inside the object its base names, rather than possibly
        /// running past its end.
        contained: bool,
    },
}

/// Where the bytes a view reads came from.
#[derive(Clone, Copy)]
enum Origin {
    /// A parameter: memory that exists before the function is entered.
    Entry,
    /// An allocation site of the function.
    Site(InstId),
    /// Anything else.
    Unknown,
}

/// The memory that exists when a function is entered and that the function may write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EntryWrites {
    /// Parameters whose memory the function may write, anywhere in the objects they point to.
    params: SmallVec<[ArgIdx; 2]>,
    /// Whether the function may write other memory that exists when it is entered.
    other: bool,
}

impl EntryWrites {
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
        let mut writes = Self::default();
        let mut record = |target| {
            let Target::Range { location, .. } = target else {
                writes.other = true;
                return;
            };
            match location.address.base {
                MemoryBase::Absolute => writes.other |= !below_heap(location),
                // Frames and fresh allocations belong to this call.
                MemoryBase::InternalFrame
                | MemoryBase::Allocation(_)
                | MemoryBase::DynamicAllocation(_) => {}
                MemoryBase::Param(value) | MemoryBase::Value(value) => match *func.value(value) {
                    Value::Arg(index) => {
                        if !writes.params.contains(&index) {
                            writes.params.push(index);
                            writes.params.sort_unstable();
                        }
                    }
                    // Past a fresh allocation or the free memory pointer lies only memory
                    // allocated after it.
                    _ if function.fresh_start(value).is_some() => {}
                    _ => writes.other = true,
                },
            }
        };
        for inst in func.instructions() {
            for target in write_targets(func, &function.aa, inst, summaries) {
                record(target);
            }
        }
        for block in &func.blocks {
            let Some(terminator) = &block.terminator else { continue };
            for &access in function.aa.terminator_mod_ref(func, terminator).writes() {
                match access {
                    Access::Any(AddressSpace::Memory) => record(Target::Anything),
                    Access::Location(Location::Memory(location)) => {
                        record(Target::Range { location, contained: false })
                    }
                    _ => {}
                }
            }
        }
        writes
    }
}

/// The memory `inst` may write.
fn write_targets(
    func: &Function,
    aa: &AliasAnalysis,
    inst: InstId,
    summaries: &FxHashMap<FunctionId, EntryWrites>,
) -> SmallVec<[Target; 2]> {
    let mut targets = SmallVec::new();
    match &func.inst(inst).kind {
        InstKind::ICall { function: Callee::Function(callee), args } => {
            let Some(callee) = summaries.get(callee) else {
                targets.push(Target::Anything);
                return targets;
            };
            if callee.other {
                targets.push(Target::Anything);
            }
            for &param in &callee.params {
                let Some(mut address) =
                    args.get(param.index()).and_then(|&arg| aa.memory_address(func, arg))
                else {
                    targets.push(Target::Anything);
                    continue;
                };
                // The callee may write anywhere in the object it is passed, and perhaps past its
                // end: for a fresh allocation, into the memory allocated after it.
                if let MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) =
                    address.base
                    && let Some(result) = func.inst_result_value(site)
                {
                    address.base = MemoryBase::Value(result);
                }
                let location = MemoryLocation::new(address, LocationSize::Unknown);
                targets.push(Target::Range { location, contained: false });
            }
        }
        // Each of these writes only memory it allocates, or memory past the free memory pointer
        // that it leaves unallocated.
        InstKind::AbiEncode { .. }
        | InstKind::AbiEncodePacked { .. }
        | InstKind::AbiDecode { .. }
        | InstKind::StorageBytesLoad(..)
        | InstKind::StorageArrayLoad { .. }
        | InstKind::ICall {
            function:
                Callee::Builtin(crate::mir::Builtin::Concat(_) | crate::mir::Builtin::ReturndataBytes),
            ..
        } => {}
        kind => {
            // Lowering emits a semantic object store only inside the object's allocation, after
            // any bounds check it needs; a raw store may run past the object it starts in.
            let contained = matches!(
                kind,
                InstKind::Alloc { .. }
                    | InstKind::SetMemoryObjectLen(..)
                    | InstKind::MemoryObjectStoreField { .. }
                    | InstKind::MemoryObjectStoreElement { .. }
                    | InstKind::MemoryObjectStoreByte { .. }
                    | InstKind::MemoryObjectStoreWord { .. }
                    | InstKind::MemoryObjectCopyFromSlice { .. }
                    | InstKind::MemoryObjectCopyFromSliceAt { .. }
            );
            for &access in aa.instruction_mod_ref(func, inst).writes() {
                match access {
                    Access::Any(AddressSpace::Memory) => targets.push(Target::Anything),
                    Access::Location(Location::Memory(location)) => {
                        targets.push(Target::Range { location, contained })
                    }
                    _ => {}
                }
            }
        }
    }
    targets
}

/// Where one use of a value happens.
#[derive(Clone, Copy)]
enum User {
    Inst(InstId),
    Terminator(BlockId),
}

/// The per-function facts every borrow in a function shares.
struct FunctionFacts {
    function: Analyzed,
    cfg: CfgInfo,
    predecessors: IndexVec<BlockId, Vec<BlockId>>,
    /// Each instruction's block and index in it.
    positions: FxHashMap<InstId, (BlockId, usize)>,
    users: FxHashMap<ValueId, Vec<User>>,
    /// Every instruction that may write memory, with what it may write.
    writes: Vec<(InstId, SmallVec<[Target; 2]>)>,
}

impl FunctionFacts {
    fn new(
        func: &Function,
        calls: &Arc<MemoryCallSummaries>,
        summaries: &FxHashMap<FunctionId, EntryWrites>,
    ) -> Self {
        let function = Analyzed::new(func, calls);
        let func = &function.func;
        let aa = &function.aa;
        let cfg = CfgInfo::new(func);
        let mut predecessors = index_vec![Vec::new(); func.blocks.len()];
        let mut positions = FxHashMap::default();
        let mut users = FxHashMap::<_, Vec<_>>::default();
        let mut writes = Vec::new();
        for (block, data) in func.blocks.iter_enumerated() {
            for &successor in cfg.successors(block) {
                predecessors[successor].push(block);
            }
            for (index, &inst) in data.instructions.iter().enumerate() {
                positions.insert(inst, (block, index));
                for operand in func.inst(inst).operands() {
                    users.entry(operand).or_default().push(User::Inst(inst));
                }
                let targets = write_targets(func, aa, inst, summaries);
                if !targets.is_empty() {
                    writes.push((inst, targets));
                }
            }
            if let Some(terminator) = &data.terminator {
                terminator.for_each_operand(|operand| {
                    users.entry(operand).or_default().push(User::Terminator(block));
                });
            }
        }
        Self { function, cfg, predecessors, positions, users, writes }
    }

    /// Reports each write that may change the bytes `borrow` reads before a read through it.
    fn check(&self, gcx: Gcx<'_>, borrow: &ViewBorrow) {
        let func = &self.function.func;
        let view = self.function.resolve(borrow.view);
        let Value::Inst(def) = *func.value(view) else { return };
        let Some(&(def_block, def_index)) = self.positions.get(&def) else { return };
        let source = self.function.resolve(borrow.source);
        let Some(start) = self.function.aa.memory_address(func, source).and_then(|address| {
            address.checked_add(EvmMemoryLayout::object_data_offset(MemoryObjectKind::Bytes))
        }) else {
            return;
        };
        let payload = MemoryLocation::new(start, LocationSize::Unknown);
        let origin = match start.base {
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) => {
                Origin::Site(site)
            }
            MemoryBase::Param(_) => Origin::Entry,
            MemoryBase::Value(value) if matches!(func.value(value), Value::Arg(_)) => Origin::Entry,
            _ => Origin::Unknown,
        };

        // The last read through the view in each block that has one.
        let reads = self.reads(view);
        // Blocks from whose start a read is reachable without creating the view again.
        let mut live = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist =
            reads.keys().copied().filter(|&block| block != def_block).collect::<Vec<_>>();
        for &block in &worklist {
            live.insert(block);
        }
        while let Some(block) = worklist.pop() {
            for &predecessor in &self.predecessors[block] {
                if predecessor != def_block && live.insert(predecessor) {
                    worklist.push(predecessor);
                }
            }
        }
        // Blocks reachable from the view without creating it again.
        let mut after = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = vec![def_block];
        while let Some(block) = worklist.pop() {
            for &successor in self.cfg.successors(block) {
                if successor != def_block && after.insert(successor) {
                    worklist.push(successor);
                }
            }
        }

        let mut reported = FxHashSet::default();
        for (inst, targets) in &self.writes {
            let (block, index) = self.positions[inst];
            let started =
                if block == def_block { index > def_index } else { after.contains(block) };
            let read_later = reads.get(&block).is_some_and(|&last| last > index)
                || self
                    .cfg
                    .successors(block)
                    .iter()
                    .any(|&successor| successor != def_block && live.contains(successor));
            if !started
                || !read_later
                || !targets.iter().any(|&target| self.may_hit(target, payload, origin, def))
            {
                continue;
            }
            let span = func.inst(*inst).metadata.source_span().unwrap_or(borrow.span);
            if reported.insert(span) {
                gcx.dcx()
                    .err(format!("this may change bytes that the view `{}` still reads", borrow.name))
                    .span(span)
                    .span_note(
                        borrow.span,
                        format!("`{}` reads the bytes in place and is read after this", borrow.name),
                    )
                    .help("finish reading the view first, or remove `@custom:solar-view` to read a copy")
                    .emit();
            }
        }
    }

    /// The last read through `view` in each block that has one; a terminator reads after every
    /// instruction of its block.
    ///
    /// The reads are the uses of every value `view` derives through pure operations, other than
    /// its length, which is fixed once the view exists.
    fn reads(&self, view: ValueId) -> FxHashMap<BlockId, usize> {
        let func = &self.function.func;
        let mut last = FxHashMap::<BlockId, usize>::default();
        let mut derived = FxHashSet::from_iter([view]);
        let mut stack = vec![view];
        while let Some(value) = stack.pop() {
            for &user in self.users.get(&value).into_iter().flatten() {
                let (block, index) = match user {
                    User::Terminator(block) => (block, func.blocks[block].instructions.len()),
                    User::Inst(inst) => {
                        let kind = &func.inst(inst).kind;
                        if kind.effect_kind() == EffectKind::Pure
                            && !matches!(kind, InstKind::SliceLen(_))
                        {
                            if let Some(result) = func.inst_result_value(inst)
                                && derived.insert(result)
                            {
                                stack.push(result);
                            }
                            continue;
                        }
                        self.positions[&inst]
                    }
                };
                let entry = last.entry(block).or_insert(index);
                *entry = (*entry).max(index);
            }
        }
        last
    }

    /// Whether a write of `target` may change the view's `payload`, which came from `origin` and
    /// was borrowed at `def`.
    fn may_hit(
        &self,
        target: Target,
        payload: MemoryLocation,
        origin: Origin,
        def: InstId,
    ) -> bool {
        let Target::Range { location, contained } = target else { return true };
        if AliasAnalysis::memory_alias_locations(location, payload) == AliasResult::NoAlias {
            return false;
        }
        match location.address.base {
            MemoryBase::Absolute => !below_heap(location),
            MemoryBase::InternalFrame => false,
            // The alias analysis keeps an allocation as the base only of an access inside it,
            // and an allocation made after the source existed cannot overlap it.
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) => match origin {
                Origin::Entry => false,
                Origin::Site(source) => site == source,
                Origin::Unknown => !self.strictly_dominates(def, site),
            },
            // A parameter's object lies below every allocation of this function, so only a
            // write that may run past its end reaches one.
            MemoryBase::Param(_) => !(contained && matches!(origin, Origin::Site(_))),
            MemoryBase::Value(value) => match self.function.fresh_start(value) {
                // Past a fresh allocation or the free memory pointer lies only memory allocated
                // after it.
                Some(site) => match origin {
                    Origin::Entry => false,
                    Origin::Site(_) | Origin::Unknown => !self.strictly_dominates(def, site),
                },
                None => true,
            },
        }
    }

    /// Whether every path to `inst` passes `def` first.
    fn strictly_dominates(&self, def: InstId, inst: InstId) -> bool {
        let (Some(&(def_block, def_index)), Some(&(block, index))) =
            (self.positions.get(&def), self.positions.get(&inst))
        else {
            return false;
        };
        if def_block == block {
            index > def_index
        } else {
            self.cfg.dominators().dominates(def_block, block)
        }
    }
}
