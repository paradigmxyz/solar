//! MIR module (top-level container).

use super::{Function, FunctionId, ImmutableId, MirType};
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
};
use solar_interface::{Ident, Symbol, sym};

/// One staged immutable value occupies one EVM word in constructor scratch memory.
pub(crate) const IMMUTABLE_SCRATCH_WORD_SIZE: usize = 32;

impl ImmutableId {
    /// Returns this immutable's byte offset in the constructor scratch area.
    #[must_use]
    pub(crate) fn scratch_offset(self) -> u64 {
        self.index() as u64 * IMMUTABLE_SCRATCH_WORD_SIZE as u64
    }
}

/// The lowering phase a [`Module`] is in.
///
/// MIR is a phased IR, like rustc's MIR: the same data structures pass through
/// well-defined phases, and passes declare what phase they expect and produce.
/// Phases only move forward. The enum order is the lowering order, so
/// [`MirPhase`] derives `Ord` and [`Module::advance_phase`] can assert
/// monotonicity.
///
/// Optimization runs on the compact high-level form first; the progressive
/// lowering phases then rewrite high-level constructs into MIR itself instead
/// of leaving them as backend special cases. The codegen pipeline runs
/// `lower-abi` and `lower-dispatch` by default and the backend consumes the
/// `dispatch`-phase module (opt out with `-Zno-mir-dispatch`); a module where
/// lowering bails keeps its phase and is dispatched by the backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum MirPhase {
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
    /// Functions take the shape the backend expects: every call edge either
    /// returns or is an explicit `tail_call` (a call to a callee that cannot
    /// return is rewritten into one, arguments included). Produced by the
    /// `lower-evm-shaped` pass.
    EvmShaped,
}

impl MirPhase {
    /// Stable textual name, as printed in the module header.
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Optimized => "optimized",
            Self::Abi => "abi",
            Self::Dispatch => "dispatch",
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
    /// Immutable types indexed by their stable MIR identifiers.
    immutables: IndexVec<ImmutableId, MirType>,
    /// Whether this is an interface (no bytecode generation).
    pub(crate) is_interface: bool,
    /// Whether optimization passes should favor bytecode size over runtime
    /// gas (`-O size`): multi-use functions are called rather than inlined.
    pub(crate) optimize_for_size: bool,
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
            immutables: IndexVec::new(),
            is_interface: false,
            optimize_for_size: false,
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

    /// Adds an immutable and returns its stable identifier.
    pub(crate) fn add_immutable(&mut self, ty: MirType) -> ImmutableId {
        self.immutables.push(ty)
    }

    /// Returns an immutable's MIR type.
    #[must_use]
    pub(crate) fn immutable_type(&self, id: ImmutableId) -> MirType {
        self.immutables[id]
    }

    /// Returns an immutable's MIR type if the identifier is allocated.
    #[must_use]
    pub(crate) fn get_immutable_type(&self, id: ImmutableId) -> Option<MirType> {
        self.immutables.get(id).copied()
    }

    /// Returns the size in bytes of the constructor scratch area that stages
    /// immutable words before they are patched into the runtime code.
    #[must_use]
    pub(crate) fn immutable_data_len(&self) -> usize {
        self.immutables.len() * IMMUTABLE_SCRATCH_WORD_SIZE
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
            write!(
                f,
                "{}",
                self.functions
                    .iter()
                    .map(|func| super::display::display_function_text(func, Some(&self.functions)))
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
                    .map(|func| super::display::display_function_dot(func, Some(&self.functions)))
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
