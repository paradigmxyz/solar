//! MIR module (top-level container).

use super::{
    AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamLayoutRef, BackendMemory, Data, DataBytes,
    DataId, DataRef, Disambiguator, Function, FunctionId, HirVariableId, ImmutableId,
    MangledSymbol, MirType, StructId, StructType, Terminator, ValueId,
};
use crate::analysis::{CallGraphInfo, CfgInfo, direct_callees};
use alloy_primitives::Bytes;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    fmt::FmtIteratorExt,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_interface::{Ident, Session, Symbol, config::OptimizationMode, sym};
use solar_ir::link::{ContractCode, LibraryTable, QualifiedName, Trailer};
use std::{borrow::Cow, fmt, sync::Arc};

/// Returns whether pass managers should validate IR after each pass.
pub fn should_validate_ir(sess: &Session) -> bool {
    sess.opts.unstable.validate_ir.unwrap_or(cfg!(debug_assertions))
}

/// A named immutable declared by a MIR module.
#[derive(Clone, Copy, Debug)]
pub struct Immutable {
    /// The source-level name used by textual MIR.
    pub(crate) name: Ident,
    /// The immutable's MIR type.
    pub ty: super::ValueLayout,
    /// The source variable, when this module was lowered from Solidity.
    pub variable_id: Option<HirVariableId>,
}

/// The representation contract of a MIR module.
///
/// Semantic MIR retains typed operations through optimization. Lowered MIR contains only
/// backend-supported word operations, with ABI routing and memory layouts made explicit.
/// Individual conversion passes may mix representations; only checked completion advances
/// the phase. Optimization history is not part of the representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MirPhase {
    /// Typed SSA with semantic operations and layouts.
    #[default]
    Semantic,
    /// Word SSA with explicit ABI, memory, and backend calling conventions.
    Lowered,
}

impl MirPhase {
    /// Stable textual name, as printed in the module header.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Lowered => "lowered",
        }
    }

    /// Looks up a phase by its textual name.
    #[must_use]
    pub(crate) fn by_name(name: Symbol) -> Option<Self> {
        Some(match name {
            sym::semantic => Self::Semantic,
            sym::lowered => Self::Lowered,
            _ => return None,
        })
    }
}

/// An immutable MIR view whose backend representation has been checked.
pub struct LoweredModule<'a>(&'a Module);

impl std::ops::Deref for LoweredModule<'_> {
    type Target = Module;

    fn deref(&self) -> &Module {
        self.0
    }
}

/// A MIR module representing a compiled contract.
#[derive(Clone, Debug)]
pub struct Module {
    pub libraries: LibraryTable,
    /// Module/contract name.
    pub name: Ident,
    /// Fixed aggregate types, with nested types declared before their users.
    pub struct_types: IndexVec<StructId, StructType>,
    /// All functions in this module.
    pub functions: IndexVec<FunctionId, Function>,
    /// The synthesized runtime dispatch entry, if this module has one.
    dispatch_entry: Option<FunctionId>,
    /// Most recently added function for each name before disambiguation.
    pub function_name_index: FxHashMap<Symbol, FunctionId>,
    /// Canonical ABI layouts referenced by semantic encoding operations.
    pub(crate) abi_layouts: Vec<AbiLayoutRef>,
    /// Canonical ABI input layouts referenced by decode instructions.
    pub(crate) abi_param_layouts: Vec<AbiParamLayoutRef>,
    /// Named immutable declarations indexed by their stable MIR identifiers.
    immutables: IndexVec<ImmutableId, Immutable>,
    /// Constant byte strings embedded in generated code.
    pub data: IndexVec<DataId, Data>,
    /// Exact data lookup used before the final subslice-packing pass.
    data_index: FxHashMap<Bytes, DataId>,
    /// Whether this is an interface (no bytecode generation).
    pub is_interface: bool,
    /// Whether this module was lowered from a library.
    ///
    /// Library runtime code never checks `callvalue`, since a `DELEGATECALL` sees the
    /// caller's value, and guards its non-view external functions with
    /// [`Self::library_deploy_address`].
    pub is_library: bool,
    /// Whether the module may contain inline assembly that does not promise memory safety, whose
    /// writes the compiler cannot place.
    ///
    /// Textual MIR may hold raw memory operations, so it counts as unsafe. Lowering Solidity
    /// clears the bit and sets it again for every assembly block that lacks the promise.
    unsafe_assembly: bool,
    /// The lowering phase this module is in.
    pub(crate) phase: MirPhase,
    /// Whether passes must account for every instruction's source debug information.
    debug_info_tracked: bool,
    /// Resolved backend layout, used only by execution tools and final dumps.
    pub backend_memory: Option<BackendMemory>,
    /// Whether the last full `lowered` check still describes this module: no change since
    /// `advance_phase` passed it.
    lowered_checked: bool,
    /// The objective this module's code is optimized for: the build's, or the one the contract's
    /// `@custom:solar-optimize` tag selects. A function's own tag may select another.
    pub optimization: OptimizationMode,
    /// Instructions `@custom:solar-inline` clones have added, which the inliner bounds over all
    /// of its runs.
    pub tagged_inline_growth: usize,
}

impl Module {
    /// Interns a structural value type within this module.
    pub fn intern_struct(&mut self, fields: impl Into<Box<[MirType]>>) -> MirType {
        let fields = fields.into();
        if let Some((id, _)) =
            self.struct_types.iter_enumerated().find(|(_, ty)| ty.fields == fields)
        {
            return MirType::Struct(id);
        }
        MirType::Struct(self.struct_types.push(StructType { fields }))
    }

    /// Represents a function's logical outputs as zero, one, or one struct value.
    pub fn intern_return_type(&mut self, fields: Vec<MirType>) -> Option<MirType> {
        match fields.as_slice() {
            [] => None,
            [ty] => Some(*ty),
            _ => Some(self.intern_struct(fields)),
        }
    }

    /// Returns whether a value can contain a reference to caller-visible memory.
    pub fn type_may_reference_memory(&self, ty: MirType) -> bool {
        match ty {
            MirType::Struct(id) => self.struct_types[id]
                .fields
                .iter()
                .any(|&field| self.type_may_reference_memory(field)),
            _ => ty.is_memory_reference(),
        }
    }

    /// Parses textual MIR.
    pub fn parse(
        sess: &solar_interface::Session,
        source: &solar_interface::source_map::SourceFile,
    ) -> solar_interface::Result<Self> {
        super::parser::parse(sess, source)
    }

    /// Creates a new module.
    #[must_use]
    pub fn new(name: Ident) -> Self {
        Self {
            name,
            functions: IndexVec::new(),
            struct_types: IndexVec::new(),
            dispatch_entry: None,
            function_name_index: FxHashMap::default(),
            abi_layouts: Vec::new(),
            abi_param_layouts: Vec::new(),
            immutables: IndexVec::new(),
            data: IndexVec::new(),
            data_index: FxHashMap::default(),
            libraries: LibraryTable::default(),
            is_interface: false,
            is_library: false,
            unsafe_assembly: true,
            phase: MirPhase::Semantic,
            debug_info_tracked: false,
            backend_memory: None,
            lowered_checked: false,
            optimization: OptimizationMode::default(),
            tagged_inline_growth: 0,
        }
    }

    /// Returns whether the module may contain inline assembly that does not promise memory safety.
    #[must_use]
    pub const fn has_unsafe_assembly(&self) -> bool {
        self.unsafe_assembly
    }

    /// Records whether the module may contain inline assembly that does not promise memory safety.
    pub fn set_unsafe_assembly(&mut self, unsafe_assembly: bool) {
        self.unsafe_assembly = unsafe_assembly;
    }

    /// Returns the objective `func`'s code is optimized for: its own tag's, else the contract's.
    pub fn objective(&self, func: FunctionId) -> OptimizationMode {
        self.objective_of(&self.functions[func])
    }

    /// Returns the objective `func`'s code is optimized for, for a function this module has not
    /// yet received.
    pub fn objective_of(&self, func: &Function) -> OptimizationMode {
        func.attributes.optimization.unwrap_or(self.optimization)
    }

    /// Returns whether any of the module's code is optimized.
    pub fn is_optimized(&self) -> bool {
        self.runs_objective(|objective| !objective.is_none())
    }

    /// Returns whether code compiled for some objective satisfies `wanted`: the contract's own
    /// code, or the code of a function whose tag selects another objective.
    pub fn runs_objective(&self, wanted: impl Fn(OptimizationMode) -> bool) -> bool {
        wanted(self.optimization)
            || self.functions.iter().any(|func| func.attributes.optimization.is_some_and(&wanted))
    }

    /// Enables or disables source debug information auditing for optimization passes.
    pub fn set_debug_info_tracked(&mut self, tracked: bool) {
        self.debug_info_tracked = tracked;
    }

    /// Returns whether optimization passes must account for source debug information.
    #[must_use]
    pub const fn debug_info_is_tracked(&self) -> bool {
        self.debug_info_tracked
    }

    /// Returns the verified representation phase.
    #[must_use]
    pub const fn phase(&self) -> MirPhase {
        self.phase
    }

    /// Checks the destination representation before advancing the phase.
    ///
    /// Release builds check only the phase contract, unless `-Zvalidate-ir` is set.
    pub fn advance_phase(
        &mut self,
        sess: &Session,
        phase: MirPhase,
    ) -> solar_interface::Result<()> {
        assert!(phase >= self.phase, "MIR phase cannot regress");
        crate::analysis::validate_phase(
            &sess.dcx,
            self,
            phase,
            sess.is_parallel_codegen(),
            should_validate_ir(sess),
        )?;
        self.phase = phase;
        self.lowered_checked = phase == MirPhase::Lowered;
        Ok(())
    }

    /// Records that the module changed since its last full `lowered` check.
    pub fn invalidate_lowered_check(&mut self) {
        self.lowered_checked = false;
    }

    /// Clears the record of the last full `lowered` check and returns whether it still held.
    pub fn take_lowered_check(&mut self) -> bool {
        std::mem::take(&mut self.lowered_checked)
    }

    /// Restores a check recorded by `take_lowered_check`, for a module left unchanged since.
    pub fn restore_lowered_check(&mut self, checked: bool) {
        self.lowered_checked |= checked;
    }

    /// Checks backend legality and borrows the module without permitting further rewrites.
    ///
    /// Skips the check when `advance_phase` passed it and nothing changed the module since,
    /// unless `recheck` is set. Without `recheck`, the check covers only the phase contract.
    /// Targets without shift opcodes always check that no shift is left.
    pub fn as_lowered(
        &self,
        sess: &Session,
        recheck: bool,
    ) -> solar_interface::Result<LoweredModule<'_>> {
        let dcx = &sess.dcx;
        if self.phase != MirPhase::Lowered {
            return Err(dcx
                .err(format!(
                    "EVM codegen requires MIR in the `lowered` phase, stopped at `{}`",
                    self.phase.name(),
                ))
                .span(self.name.span)
                .emit());
        }
        if recheck || !self.lowered_checked {
            crate::analysis::validate_phase(
                dcx,
                self,
                MirPhase::Lowered,
                sess.is_parallel_codegen(),
                recheck,
            )?;
        }
        if !sess.opts.evm_version.has_bitwise_shifting() {
            crate::analysis::validate_legal_shifts(dcx, self)?;
        }
        Ok(LoweredModule(self))
    }

    /// Returns whether all external entries have an explicit ABI implementation.
    pub fn has_explicit_abi(&self) -> bool {
        self.functions
            .iter()
            .all(|func| !func.is_external_entry() || func.attributes.is_abi_wrapper)
    }

    /// Returns whether a live value or function signature still uses an SSA struct.
    pub fn has_struct_values(&self) -> bool {
        self.has_value_type(|ty| matches!(ty, MirType::Struct(_)))
    }

    fn has_value_type(&self, predicate: impl Fn(MirType) -> bool) -> bool {
        let mut operands = SmallVec::<[ValueId; 8]>::new();
        self.functions.iter().any(|func| {
            let matches = |value| func.value_ty(value).is_some_and(&predicate);
            if func.arg_indices().any(|index| predicate(func.arg_ty(index)))
                || func.return_components().iter().any(|&ty| predicate(ty))
            {
                return true;
            }
            for block in &func.blocks {
                for &inst_id in &block.instructions {
                    let inst = func.inst(inst_id);
                    operands.clear();
                    inst.kind.collect_operands(&mut operands);
                    if operands.iter().copied().chain(inst.result()).any(matches) {
                        return true;
                    }
                }
                let mut found = false;
                if let Some(term) = &block.terminator {
                    term.visit_operands(|value| found |= matches(value));
                }
                if found {
                    return true;
                }
            }
            false
        })
    }

    /// Finds functions that may return to their caller, directly or through tail calls.
    ///
    /// Propagate direct returns backwards over tail-call edges. Cycles without a return stay
    /// nonreturning. Unreachable returns and invalid tail targets count conservatively; this
    /// query does not prove reachability or replace validation of call targets.
    pub fn returning_functions(&self) -> DenseBitSet<FunctionId> {
        let mut callers = index_vec![Vec::new(); self.functions.len()];
        let mut returning = DenseBitSet::new_empty(self.functions.len());
        let mut worklist = Vec::new();
        for (id, func) in self.iter_functions() {
            for block in &func.blocks {
                let may_return = match &block.terminator {
                    Some(Terminator::Return { .. }) => true,
                    Some(Terminator::TailCall { function, .. }) => {
                        if let Some(callers) = callers.get_mut(*function) {
                            callers.push(id);
                            false
                        } else {
                            true
                        }
                    }
                    _ => false,
                };
                if may_return && returning.insert(id) {
                    worklist.push(id);
                }
            }
        }
        while let Some(callee) = worklist.pop() {
            for &caller in &callers[callee] {
                if returning.insert(caller) {
                    worklist.push(caller);
                }
            }
        }
        returning
    }

    /// Adds a function to the module.
    pub fn add_function(&mut self, function: Function) -> FunctionId {
        let symbol = function.name.symbol;
        let function = self.functions.push(function);
        if let Some(duplicate) = self.function_name_index.insert(symbol, function) {
            let duplicate_func = &mut self.functions[duplicate];
            if duplicate_func.name.disambiguator.is_none() {
                duplicate_func.name =
                    MangledSymbol::disambiguated(symbol, Disambiguator::from_foreign(duplicate));
            }
            if self.functions[function].name.disambiguator.is_none() {
                self.functions[function].name =
                    MangledSymbol::disambiguated(symbol, Disambiguator::from_foreign(function));
            }
        }
        function
    }

    /// Returns the synthesized runtime dispatch entry, if present.
    #[must_use]
    pub const fn dispatch_entry(&self) -> Option<FunctionId> {
        self.dispatch_entry
    }

    /// Records the synthesized runtime dispatch entry.
    pub fn set_dispatch_entry(&mut self, entry: FunctionId) {
        assert!(
            self.dispatch_entry.replace(entry).is_none(),
            "module already has a dispatch entry"
        );
    }

    /// Updates the dispatch entry after remapping function IDs.
    pub fn remap_dispatch_entry(&mut self, entry: Option<FunctionId>) {
        self.dispatch_entry = entry;
    }

    /// Returns the functions that the runtime entry, a selector, `receive`, or `fallback`
    /// reaches through calls.
    #[must_use]
    pub fn runtime_reachable_functions(&self) -> DenseBitSet<FunctionId> {
        let roots = self.dispatch_entry().into_iter().chain(
            self.functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.is_runtime_entry().then_some(id)),
        );
        self.reachable_from(roots)
    }

    /// Returns the constructor and the functions it reaches through calls.
    #[must_use]
    pub fn constructor_reachable_functions(&self) -> DenseBitSet<FunctionId> {
        self.reachable_from(
            self.functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id)),
        )
    }

    /// Checks the phase contract for helpers that require runtime static frames.
    /// Constructor ABI staging may already occupy the free-memory pointer, so
    /// these helpers must not be reachable through any constructor call chain.
    pub fn assert_runtime_helpers(&self, helpers: impl IntoIterator<Item = FunctionId>) {
        if cfg!(debug_assertions) {
            let helpers = helpers.into_iter().collect::<Vec<_>>();
            if helpers.is_empty() {
                return;
            }
            let constructor = self.constructor_reachable_functions();
            for helper in helpers {
                assert!(
                    !constructor.contains(helper),
                    "runtime helper `{}` is constructor-reachable and may overlap ABI staging",
                    self.function(helper).name
                );
            }
        }
    }

    /// Returns the functions that neither `constructor` nor `runtime` contains, and the functions
    /// they reach through calls. Only ad-hoc MIR has them, and no one knows where they run.
    #[must_use]
    pub fn unrooted_reachable_functions(
        &self,
        constructor: &DenseBitSet<FunctionId>,
        runtime: &DenseBitSet<FunctionId>,
    ) -> DenseBitSet<FunctionId> {
        let mut unrooted = DenseBitSet::new_filled(self.functions.len());
        unrooted.subtract(constructor);
        unrooted.subtract(runtime);
        self.reachable_from(unrooted.iter())
    }

    /// Returns the functions that run only during deployment: the constructor and the functions
    /// it reaches through calls, less those the runtime also reaches and the unrooted ones.
    #[must_use]
    pub(crate) fn creation_only_functions(&self) -> DenseBitSet<FunctionId> {
        let constructor = self.constructor_reachable_functions();
        let runtime = self.runtime_reachable_functions();
        let mut creation = constructor.clone();
        creation.subtract(&runtime);
        creation.subtract(&self.unrooted_reachable_functions(&constructor, &runtime));
        creation
    }

    /// Returns the creation-only functions that no loop or recursion calls, and the subset of
    /// those without loops of their own. Their instructions run a bounded number of times, not
    /// scaled by a trip count, which the creation model's single execution approximates for the
    /// second set, and for the whole of the first when a transform weighs the function's own
    /// loops.
    #[must_use]
    pub fn creation_pricing_functions(&self) -> (DenseBitSet<FunctionId>, DenseBitSet<FunctionId>) {
        let creation = self.creation_only_functions();
        let (looping, repeated) = self.loops_and_repeats(&creation, &CallGraphInfo::new(self));
        let mut called_once = creation;
        called_once.subtract(&repeated);
        let mut straight = called_once.clone();
        straight.subtract(&looping);
        (called_once, straight)
    }

    /// Returns the functions of `domain` that run more than once per deployment because they
    /// are recursive, or because a function with a loop, or a recursive one, calls them
    /// directly or not.
    #[must_use]
    pub fn repeated_functions(
        &self,
        domain: &DenseBitSet<FunctionId>,
        call_graph: &CallGraphInfo,
    ) -> DenseBitSet<FunctionId> {
        self.loops_and_repeats(domain, call_graph).1
    }

    /// Returns the functions of `domain` with a loop of their own, and those it repeats.
    fn loops_and_repeats(
        &self,
        domain: &DenseBitSet<FunctionId>,
        call_graph: &CallGraphInfo,
    ) -> (DenseBitSet<FunctionId>, DenseBitSet<FunctionId>) {
        let mut looping = DenseBitSet::new_empty(self.functions.len());
        let mut repeated = DenseBitSet::new_empty(self.functions.len());
        let mut worklist = Vec::new();
        for id in domain.iter() {
            let func = self.function(id);
            let has_loop = !CfgInfo::new(func).cyclic_blocks().is_empty();
            let repeats_callees = has_loop || call_graph.is_recursive(id);
            if has_loop {
                looping.insert(id);
            }
            if repeats_callees {
                worklist.extend(
                    direct_callees(func)
                        .filter(|&callee| domain.contains(callee) && repeated.insert(callee)),
                );
            }
        }
        while let Some(id) = worklist.pop() {
            worklist.extend(
                direct_callees(self.function(id))
                    .filter(|&callee| domain.contains(callee) && repeated.insert(callee)),
            );
        }
        (looping, repeated)
    }

    /// Returns `roots` and every function they reach through calls.
    fn reachable_from(
        &self,
        roots: impl IntoIterator<Item = FunctionId>,
    ) -> DenseBitSet<FunctionId> {
        let mut reachable = DenseBitSet::new_empty(self.functions.len());
        let mut worklist = roots.into_iter().filter(|&id| reachable.insert(id)).collect::<Vec<_>>();
        while let Some(func_id) = worklist.pop() {
            worklist.extend(
                direct_callees(self.function(func_id)).filter(|&callee| reachable.insert(callee)),
            );
        }
        reachable
    }

    /// Returns the function for the given ID.
    #[must_use]
    pub fn function(&self, id: FunctionId) -> &Function {
        &self.functions[id]
    }

    /// Returns a mutable reference to the function.
    pub fn function_mut(&mut self, id: FunctionId) -> &mut Function {
        &mut self.functions[id]
    }

    /// Interns an ABI layout and returns its canonical shared reference.
    pub fn intern_abi_layout(&mut self, layout: AbiLayout) -> AbiLayoutRef {
        if let Some(existing) =
            self.abi_layouts.iter().find(|existing| existing.as_ref() == &layout)
        {
            return Arc::clone(existing);
        }
        let layout = Arc::new(layout);
        self.abi_layouts.push(Arc::clone(&layout));
        layout
    }

    /// Interns an ABI input layout and returns its canonical shared reference.
    pub fn intern_abi_param_layout(&mut self, layout: AbiParamLayout) -> AbiParamLayoutRef {
        if let Some(existing) =
            self.abi_param_layouts.iter().find(|existing| existing.as_ref() == &layout)
        {
            return Arc::clone(existing);
        }
        let layout = Arc::new(layout);
        self.abi_param_layouts.push(Arc::clone(&layout));
        layout
    }

    /// Adds a named immutable and returns its stable identifier.
    pub fn add_immutable(
        &mut self,
        name: Ident,
        ty: super::ValueLayout,
        variable_id: Option<HirVariableId>,
    ) -> ImmutableId {
        self.immutables.push(Immutable { name, ty, variable_id })
    }

    /// Returns an immutable declaration.
    #[must_use]
    pub fn immutable(&self, id: ImmutableId) -> &Immutable {
        &self.immutables[id]
    }

    /// Returns a mutable immutable declaration.
    pub fn immutable_mut(&mut self, id: ImmutableId) -> &mut Immutable {
        &mut self.immutables[id]
    }

    /// Returns an immutable declaration if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_immutable(&self, id: ImmutableId) -> Option<&Immutable> {
        self.immutables.get(id)
    }

    /// Returns an immutable's MIR type.
    #[must_use]
    pub fn immutable_type(&self, id: ImmutableId) -> super::ValueLayout {
        self.immutable(id).ty
    }

    /// Returns an immutable's MIR type if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_immutable_type(&self, id: ImmutableId) -> Option<super::ValueLayout> {
        self.get_immutable(id).map(|immutable| immutable.ty)
    }

    /// Returns the synthetic immutable holding a library's own deployment address.
    ///
    /// It is declared for libraries with non-view external functions, stored by the creation
    /// code, and compared against `address()` by the dispatch so that those functions only run
    /// through `DELEGATECALL`. It is never a source variable, and is the only immutable a
    /// library declares. Textual MIR drops source variables, so a contract's own immutable of
    /// the same name is told apart by the module not being a library.
    pub fn library_deploy_address(&self) -> Option<ImmutableId> {
        if !self.is_library {
            return None;
        }
        self.immutables
            .iter_enumerated()
            .find(|(_, immutable)| {
                immutable.variable_id.is_none()
                    && immutable.name.name == sym::library_deploy_address
            })
            .map(|(id, _)| id)
    }

    /// Returns the number of immutable declarations.
    #[must_use]
    pub fn immutable_count(&self) -> usize {
        self.immutables.len()
    }

    /// Returns an iterator over all immutable declarations.
    pub fn iter_immutables(&self) -> impl Iterator<Item = (ImmutableId, &Immutable)> {
        self.immutables.iter_enumerated()
    }

    /// Adds a data entry, interning its bytes when they are plain literal data.
    pub(crate) fn add_data_entry(&mut self, data: Data) -> DataId {
        let bytes = data.bytes.known().filter(|_| data.library_relocations.is_empty()).cloned();
        let id = self.data.push(data);
        if let Some(bytes) = bytes {
            self.data_index.entry(bytes).or_insert(id);
        }
        id
    }

    /// Interns another contract's bytecode, which final assembly links in.
    pub fn intern_contract_code(&mut self, code: ContractCode, name: Symbol) -> DataId {
        let existing =
            self.data.iter_enumerated().find(|(_, data)| data.bytes == DataBytes::Deferred(code));
        match existing {
            Some((id, _)) => id,
            None => self.data.push(Data::contract_code(code, Some(name))),
        }
    }

    /// Interns constant data and returns its stable identifier.
    pub fn intern_data(&mut self, data: Cow<'_, [u8]>, name: Option<Symbol>) -> DataRef {
        let name = name.unwrap_or(sym::literal);
        if let Some(&id) = self.data_index.get(data.as_ref()) {
            if self.data[id].name.is_none_or(|old| old == sym::literal) {
                self.data[id].name = Some(name);
            }
            return DataRef::new(id, 0);
        }
        let id = self.add_data(Bytes::from(data.into_owned()), Some(name));
        DataRef::new(id, 0)
    }

    pub(crate) fn add_data(&mut self, data: Bytes, name: Option<Symbol>) -> DataId {
        self.add_data_entry(Data::new(data, name))
    }

    /// Adds `contract`'s trailing runtime data, which must be emitted at the end of the runtime
    /// program.
    pub fn append_runtime_data(
        &mut self,
        data: Bytes,
        name: Option<Symbol>,
        contract: QualifiedName,
    ) -> DataId {
        let trailers =
            vec![Trailer { contracts: smallvec::smallvec![contract], range: 0..data.len() }];
        self.add_data_entry(Data { emit_in_runtime: true, trailers, ..Data::new(data, name) })
    }

    /// Returns an iterator over all functions.
    pub fn iter_functions(&self) -> impl Iterator<Item = (FunctionId, &Function)> {
        self.functions.iter_enumerated()
    }

    /// Returns the module's name, which its `@module` line declares.
    pub fn name(&self) -> Ident {
        self.name
    }

    /// Returns the human-readable textual MIR representation of this module.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            writeln!(f, "@module {}", self.name)?;
            if self.phase != MirPhase::default() {
                writeln!(f, "@phase {}", self.phase.name())?;
            }
            if self.is_library {
                writeln!(f, "@library")?;
            }
            if !self.struct_types.is_empty() {
                writeln!(f, "@types")?;
                for (id, ty) in self.struct_types.iter_enumerated() {
                    writeln!(f, "  struct{}: {{{}}}", id.index(), ty.fields.iter().format(", "))?;
                }
                writeln!(f)?;
            }
            write!(f, "{}", solar_ir::link::display_declarations(&self.libraries, &self.data))?;
            if !self.immutables.is_empty() {
                writeln!(f, "@immutables")?;
                for (id, immutable) in self.iter_immutables() {
                    writeln!(
                        f,
                        "  {}: {}",
                        super::display::display_immutable_ref(id, Some(self)),
                        immutable.ty
                    )?;
                }
                writeln!(f)?;
            }
            if let Some(layout) = &self.backend_memory {
                writeln!(f, "{}", layout.display(self))?;
            }
            write!(
                f,
                "{}",
                self.functions
                    .iter_enumerated()
                    .map(|(id, func)| {
                        super::display::display_function_text(
                            func,
                            Some(self),
                            self.dispatch_entry == Some(id),
                        )
                    })
                    .format("\n")
            )
        })
    }

    /// Returns this module's DOT-format CFGs.
    pub fn to_dot(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            if let Some(layout) = &self.backend_memory {
                writeln!(f, "{}", layout.display(self))?;
            }
            write!(
                f,
                "{}",
                self.functions
                    .iter()
                    .map(|func| super::display::display_function_dot(func, Some(self)))
                    .format("\n\n")
            )
        })
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_text())
    }
}
