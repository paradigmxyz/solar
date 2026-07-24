//! MIR module (top-level container).

use super::{
    AbiLayout, AbiLayoutRef, Function, FunctionId, ImmutableId, MirType, StorageLayout,
    StorageLayoutRef,
};
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
};
use solar_interface::{Ident, Symbol, sym};
use std::sync::Arc;

/// A named immutable declared by a MIR module.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Immutable {
    /// The source-level name used by textual MIR.
    pub(crate) name: Ident,
    /// The immutable's MIR type.
    pub(crate) ty: MirType,
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
/// dispatch, memory-object, and EVM-shape lowering by default, and the backend
/// consumes the `evm-shaped` module. A module where ABI/dispatch lowering bails
/// keeps its earlier phase and uses the backend dispatcher.
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
    /// function that routes to the ABI wrappers, instead of being generated
    /// inside the backend.
    Dispatch,
    /// Semantic memory objects have been lowered to physical pointer and word
    /// operations. Produced by the `lower-memory-objects` pass.
    MemoryLowered,
    /// Functions take the shape the backend expects: every call edge either
    /// returns or is an explicit `tail_call` (a call to a callee that cannot
    /// return is rewritten into one, arguments included). Produced by the
    /// `lower-evm-shaped` pass.
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
    /// Canonical ABI layouts referenced by semantic encoding operations.
    pub(crate) abi_layouts: Vec<AbiLayoutRef>,
    /// Canonical storage layouts referenced by semantic aggregate operations.
    pub(crate) aggregate_layouts: Vec<StorageLayoutRef>,
    /// Named immutable declarations indexed by their stable MIR identifiers.
    immutables: IndexVec<ImmutableId, Immutable>,
    /// Whether this is an interface (no bytecode generation).
    pub(crate) is_interface: bool,
    /// The lowering phase this module is in.
    pub(crate) phase: MirPhase,
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
            abi_layouts: Vec::new(),
            aggregate_layouts: Vec::new(),
            immutables: IndexVec::new(),
            is_interface: false,
            phase: MirPhase::Built,
        }
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
        self.functions.push(function)
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

    /// Interns a storage layout and returns its canonical shared reference.
    pub(crate) fn intern_storage_layout(&mut self, layout: StorageLayout) -> StorageLayoutRef {
        if let Some(existing) =
            self.aggregate_layouts.iter().find(|existing| existing.as_ref() == &layout)
        {
            return Arc::clone(existing);
        }
        let layout = Arc::new(layout);
        self.aggregate_layouts.push(Arc::clone(&layout));
        layout
    }

    /// Adds a named immutable and returns its stable identifier.
    pub(crate) fn add_immutable(&mut self, name: Ident, ty: MirType) -> ImmutableId {
        self.immutables.push(Immutable { name, ty })
    }

    /// Returns an immutable declaration.
    #[must_use]
    pub(crate) fn immutable(&self, id: ImmutableId) -> &Immutable {
        &self.immutables[id]
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

    /// Returns the number of immutable declarations.
    #[must_use]
    pub(crate) fn immutable_count(&self) -> usize {
        self.immutables.len()
    }

    /// Returns an iterator over all immutable declarations.
    pub(crate) fn iter_immutables(&self) -> impl Iterator<Item = (ImmutableId, &Immutable)> {
        self.immutables.iter_enumerated()
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
            if !self.immutables.is_empty() {
                writeln!(f, "immutables:")?;
                for immutable in &self.immutables {
                    writeln!(f, "  {}: {}", immutable.name, immutable.ty)?;
                }
                writeln!(f)?;
            }
            write!(
                f,
                "{}",
                self.functions
                    .iter()
                    .map(|func| super::display::display_function_text(func, Some(self)))
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
