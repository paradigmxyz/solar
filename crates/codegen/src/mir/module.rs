//! MIR module (top-level container).

use super::{
    AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamLayoutRef, DataId, DataRef, Disambiguator,
    Function, FunctionId, ImmutableId, MangledSymbol, MirType,
};
use alloy_primitives::Bytes;
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
    map::FxHashMap,
};
use solar_interface::{Ident, Symbol, sym};
use solar_sema::hir::VariableId;
use std::{borrow::Cow, sync::Arc};

/// A named immutable declared by a MIR module.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Immutable {
    /// The source-level name used by textual MIR.
    pub(crate) name: Ident,
    /// The immutable's MIR type.
    pub(crate) ty: MirType,
    /// The source variable, when this module was lowered from Solidity.
    pub(crate) variable_id: Option<VariableId>,
}

/// An unresolved external library address referenced by a MIR module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryLink {
    /// Source unit containing the library.
    pub(crate) source: String,
    /// Library contract name.
    pub(crate) name: String,
    /// Fixed-width address placeholder emitted into bytecode.
    pub(crate) placeholder: [u8; 20],
}
/// One constant byte string and its optional display name.
#[derive(Clone, Debug)]
struct Data {
    bytes: Bytes,
    name: Option<Symbol>,
    emit_in_runtime: bool,
}

/// The lowering phase a [`Module`] is in.
///
/// MIR is a phased IR, like rustc's MIR: the same data structures pass through
/// well-defined phases, and passes declare what phase they expect and produce.
/// Phases only move forward. The enum order is the lowering order, so
/// [`MirPhase`] derives `Ord` and `Module::advance_phase` can assert monotonicity.
///
/// Optimization runs on the compact high-level form first; the progressive
/// lowering phases then rewrite high-level constructs into MIR itself instead
/// of leaving them as backend special cases. The codegen pipeline runs ABI,
/// dispatch, memory-object, allocation, and EVM-shape lowering by default. The
/// backend only consumes an `evm-shaped` module; a lowering pass that cannot
/// complete leaves the module at an earlier phase and codegen reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MirPhase {
    /// Fresh from HIR lowering: typed values, internal calls by function id,
    /// dispatch and ABI handling not yet materialized as MIR.
    #[default]
    Built,
    /// The canonical optimization pipeline has run.
    Optimized,
    /// Every external function has been rewritten into a self-decoding wrapper:
    /// it decodes calldata into typed arguments and calls the original body as
    /// an internal function; the body keeps its fused external termination.
    /// The wrapper keeps its selector but takes no MIR arguments.
    Abi,
    /// The selector switch has been materialized as an ordinary MIR `entry`
    /// function that routes to the ABI wrappers.
    Dispatch,
    /// Semantic memory objects have been lowered to physical pointer and word
    /// operations. Produced by the `lower-memory-objects` pass.
    MemoryLowered,
    /// Functions take the shape the backend expects: every call edge either
    /// returns or is an explicit `tail_call` (a call to a callee that cannot
    /// return is rewritten into one, arguments included). Produced by the
    /// `lower-evm-shaped` pass after all required representation lowering.
    EvmShaped,
}

impl MirPhase {
    /// Stable textual name, as printed in the module header.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Optimized => "optimized",
            Self::Abi => "abi",
            Self::Dispatch => "dispatch",
            Self::MemoryLowered => "memory-lowered",
            Self::EvmShaped => "evm-shaped",
        }
    }

    /// Looks up a phase by its textual name.
    #[must_use]
    pub(crate) fn by_name(name: Symbol) -> Option<Self> {
        Some(match name {
            sym::built => Self::Built,
            sym::optimized => Self::Optimized,
            sym::abi => Self::Abi,
            sym::dispatch => Self::Dispatch,
            sym::memory_dash_lowered => Self::MemoryLowered,
            sym::evm_dash_shaped => Self::EvmShaped,
            _ => return None,
        })
    }
}

/// A MIR module representing a compiled contract.
#[derive(Clone, Debug)]
pub struct Module {
    /// Module/contract name.
    pub(crate) name: Ident,
    /// All functions in this module.
    pub(crate) functions: IndexVec<FunctionId, Function>,
    /// The synthesized runtime dispatch entry, if this module has one.
    dispatch_entry: Option<FunctionId>,
    /// Most recently added function for each name before disambiguation.
    pub(crate) function_name_index: FxHashMap<Symbol, FunctionId>,
    /// Canonical ABI layouts referenced by semantic encoding operations.
    pub(crate) abi_layouts: Vec<AbiLayoutRef>,
    /// Canonical ABI input layouts referenced by decode instructions.
    pub(crate) abi_param_layouts: Vec<AbiParamLayoutRef>,
    /// Named immutable declarations indexed by their stable MIR identifiers.
    immutables: IndexVec<ImmutableId, Immutable>,
    /// Constant byte strings embedded in generated code.
    data: IndexVec<DataId, Data>,
    /// Exact data lookup used before the final subslice-packing pass.
    data_index: FxHashMap<Bytes, DataId>,
    /// Unresolved external library addresses used by this module.
    library_links: Vec<LibraryLink>,
    /// Whether this is an interface (no bytecode generation).
    pub(crate) is_interface: bool,
    /// Whether this module was lowered from a library.
    ///
    /// Library runtime code never checks `callvalue`, since a `DELEGATECALL` sees the
    /// caller's value, and guards its non-view external functions with
    /// [`Self::library_deploy_address`].
    pub(crate) is_library: bool,
    /// The lowering phase this module is in.
    pub(crate) phase: MirPhase,
    /// Whether passes must account for every instruction's source debug information.
    debug_info_tracked: bool,
}

impl Module {
    /// Parses textual MIR.
    pub fn parse(
        sess: &solar_interface::Session,
        source: &solar_interface::source_map::SourceFile,
    ) -> solar_interface::Result<Self> {
        super::parser::parse(sess, source)
    }

    /// Creates a new module.
    #[must_use]
    pub(crate) fn new(name: Ident) -> Self {
        Self {
            name,
            functions: IndexVec::new(),
            dispatch_entry: None,
            function_name_index: FxHashMap::default(),
            abi_layouts: Vec::new(),
            abi_param_layouts: Vec::new(),
            immutables: IndexVec::new(),
            data: IndexVec::new(),
            data_index: FxHashMap::default(),
            library_links: Vec::new(),
            is_interface: false,
            is_library: false,
            phase: MirPhase::Built,
            debug_info_tracked: false,
        }
    }

    /// Enables or disables source debug information auditing for optimization passes.
    pub(crate) fn set_debug_info_tracked(&mut self, tracked: bool) {
        self.debug_info_tracked = tracked;
    }

    /// Returns whether optimization passes must account for source debug information.
    #[must_use]
    pub(crate) const fn debug_info_is_tracked(&self) -> bool {
        self.debug_info_tracked
    }

    /// Advances this module to a later phase.
    ///
    /// Phases only move forward; a pipeline that would regress the phase is a
    /// bug in pass scheduling.
    pub(crate) fn advance_phase(&mut self, phase: MirPhase) {
        debug_assert!(
            phase >= self.phase,
            "MIR phase cannot regress: {} -> {}",
            self.phase.name(),
            phase.name()
        );
        self.phase = phase;
    }

    /// Adds a function to the module.
    pub(crate) fn add_function(&mut self, function: Function) -> FunctionId {
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
    pub(crate) const fn dispatch_entry(&self) -> Option<FunctionId> {
        self.dispatch_entry
    }

    /// Records the synthesized runtime dispatch entry.
    pub(crate) fn set_dispatch_entry(&mut self, entry: FunctionId) {
        assert!(
            self.dispatch_entry.replace(entry).is_none(),
            "module already has a dispatch entry"
        );
    }

    /// Updates the dispatch entry after remapping function IDs.
    pub(crate) fn remap_dispatch_entry(&mut self, entry: Option<FunctionId>) {
        self.dispatch_entry = entry;
    }

    /// Returns the function for the given ID.
    #[must_use]
    pub(crate) fn function(&self, id: FunctionId) -> &Function {
        &self.functions[id]
    }

    /// Returns a mutable reference to the function.
    pub(crate) fn function_mut(&mut self, id: FunctionId) -> &mut Function {
        &mut self.functions[id]
    }

    /// Interns an ABI layout and returns its canonical shared reference.
    pub(crate) fn intern_abi_layout(&mut self, layout: AbiLayout) -> AbiLayoutRef {
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
    pub(crate) fn intern_abi_param_layout(&mut self, layout: AbiParamLayout) -> AbiParamLayoutRef {
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
    pub(crate) fn add_immutable(
        &mut self,
        name: Ident,
        ty: MirType,
        variable_id: Option<VariableId>,
    ) -> ImmutableId {
        self.immutables.push(Immutable { name, ty, variable_id })
    }

    /// Returns an immutable declaration.
    #[must_use]
    pub(crate) fn immutable(&self, id: ImmutableId) -> &Immutable {
        &self.immutables[id]
    }

    /// Registers an unresolved external library address.
    pub(crate) fn add_library_link(&mut self, link: LibraryLink) {
        if !self.library_links.contains(&link) {
            self.library_links.push(link);
        }
    }

    /// Returns unresolved external library addresses used by this module.
    pub(crate) fn library_links(&self) -> &[LibraryLink] {
        &self.library_links
    }

    /// Returns an immutable declaration if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_immutable(&self, id: ImmutableId) -> Option<&Immutable> {
        self.immutables.get(id)
    }

    /// Returns an immutable's MIR type.
    #[must_use]
    pub(crate) fn immutable_type(&self, id: ImmutableId) -> MirType {
        self.immutable(id).ty
    }

    /// Returns an immutable's MIR type if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_immutable_type(&self, id: ImmutableId) -> Option<MirType> {
        self.get_immutable(id).map(|immutable| immutable.ty)
    }

    /// Returns the synthetic immutable holding a library's own deployment address.
    ///
    /// It is declared for libraries with non-view external functions, stored by the creation
    /// code, and compared against `address()` by the dispatch so that those functions only run
    /// through `DELEGATECALL`. It is never a source variable, and is the only immutable a
    /// library declares. Textual MIR drops source variables, so a contract's own immutable of
    /// the same name is told apart by the module not being a library.
    pub(crate) fn library_deploy_address(&self) -> Option<ImmutableId> {
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
    pub(crate) fn immutable_count(&self) -> usize {
        self.immutables.len()
    }

    /// Returns an iterator over all immutable declarations.
    pub(crate) fn iter_immutables(&self) -> impl Iterator<Item = (ImmutableId, &Immutable)> {
        self.immutables.iter_enumerated()
    }

    /// Interns constant data and returns its stable identifier.
    pub(crate) fn intern_data(&mut self, data: Cow<'_, [u8]>, name: Option<Symbol>) -> DataRef {
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
        self.push_data(data, name, false)
    }

    /// Adds opaque data that must be emitted at the end of the runtime program.
    pub(crate) fn append_runtime_data(&mut self, data: Bytes, name: Option<Symbol>) -> DataId {
        self.push_data(data, name, true)
    }

    fn push_data(&mut self, data: Bytes, name: Option<Symbol>, emit_in_runtime: bool) -> DataId {
        let id = self.data.push(Data { bytes: data.clone(), name, emit_in_runtime });
        self.data_index.entry(data).or_insert(id);
        id
    }

    pub(crate) fn data_name(&self, id: DataId) -> Option<Symbol> {
        self.data[id].name
    }

    pub(crate) fn data_is_emitted_in_runtime(&self, id: DataId) -> bool {
        self.data[id].emit_in_runtime
    }

    /// Returns constant data if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_data(&self, id: DataId) -> Option<&Bytes> {
        self.data.get(id).map(|data| &data.bytes)
    }

    /// Returns the number of constant data entries.
    #[must_use]
    pub(crate) fn data_count(&self) -> usize {
        self.data.len()
    }

    /// Returns all constant data entries.
    pub(crate) fn iter_data(&self) -> impl Iterator<Item = (DataId, &Bytes)> {
        self.data.iter_enumerated().map(|(id, data)| (id, &data.bytes))
    }

    /// Returns an iterator over all functions.
    pub(crate) fn iter_functions(&self) -> impl Iterator<Item = (FunctionId, &Function)> {
        self.functions.iter_enumerated()
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
            if !self.data.is_empty() {
                writeln!(f, "data:")?;
                for (id, data) in self.iter_data() {
                    if let Some(name) = self.data_name(id) {
                        write!(f, "  {}", crate::utils::display_data_name(name, id.index()))?;
                    } else {
                        write!(f, "  {}", id.index())?;
                    }
                    write!(f, ": hex\"")?;
                    for byte in data {
                        write!(f, "{byte:02x}")?;
                    }
                    writeln!(f, "\"")?;
                }
                writeln!(f)?;
            }
            if !self.immutables.is_empty() {
                writeln!(f, "immutables:")?;
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
