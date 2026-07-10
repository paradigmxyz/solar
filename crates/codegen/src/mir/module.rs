//! MIR module (top-level container).

use super::{Function, FunctionId, MirType};
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
};
use solar_interface::Ident;

/// Current immutable staging and placeholder width.
///
/// TODO: Support immutable references with byte widths `<= 32` instead of
/// forcing every immutable through a full `PUSH32`/word patch. Solidity's
/// standard JSON format permits shorter immutable reference lengths, and solc
/// can emit `PUSH<N>` for small immutable types. Doing that here requires
/// carrying the byte width through MIR, assembler immutable refs, and the
/// constructor patch loop instead of blindly patching with `MSTORE`.
pub const IMMUTABLE_WORD_SIZE: usize = 32;

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
            Self::EvmShaped => "evm-shaped",
        }
    }

    /// Looks up a phase by its textual name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "built" => Self::Built,
            "optimized" => Self::Optimized,
            "abi" => Self::Abi,
            "dispatch" => Self::Dispatch,
            "evm-shaped" => Self::EvmShaped,
            _ => return None,
        })
    }
}

/// A MIR module representing a compiled contract.
#[derive(Clone, Debug)]
pub struct Module {
    /// Module/contract name.
    pub name: Ident,
    /// All functions in this module.
    pub functions: IndexVec<FunctionId, Function>,
    /// Data segments (for string literals, etc.).
    pub data_segments: Vec<DataSegment>,
    /// Storage layout.
    pub storage_layout: Vec<StorageSlot>,
    /// Immutable scratch-area layout (currently one staged word per immutable).
    pub immutables: Vec<ImmutableSlot>,
    /// Whether this is an interface (no bytecode generation).
    pub is_interface: bool,
    /// Whether optimization passes should favor bytecode size over runtime
    /// gas (`-O size`): multi-use functions are called rather than inlined.
    pub optimize_for_size: bool,
    /// The lowering phase this module is in.
    pub phase: MirPhase,
}

impl Module {
    /// Creates a new module.
    #[must_use]
    pub fn new(name: Ident) -> Self {
        Self {
            name,
            functions: IndexVec::new(),
            data_segments: Vec::new(),
            storage_layout: Vec::new(),
            immutables: Vec::new(),
            is_interface: false,
            optimize_for_size: false,
            phase: MirPhase::Built,
        }
    }

    /// Advances this module to a later phase.
    ///
    /// Phases only move forward; a pipeline that would regress the phase is a
    /// bug in pass scheduling.
    pub fn advance_phase(&mut self, phase: MirPhase) {
        debug_assert!(
            phase >= self.phase,
            "MIR phase cannot regress: {} -> {}",
            self.phase.name(),
            phase.name()
        );
        self.phase = phase;
    }

    /// Adds a function to the module.
    pub fn add_function(&mut self, function: Function) -> FunctionId {
        self.functions.push(function)
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

    /// Adds a data segment.
    pub fn add_data_segment(&mut self, data: Vec<u8>) -> usize {
        let index = self.data_segments.len();
        self.data_segments.push(DataSegment { data });
        index
    }

    /// Adds a storage slot.
    pub fn add_storage_slot(&mut self, slot: StorageSlot) -> usize {
        let index = self.storage_layout.len();
        self.storage_layout.push(slot);
        index
    }

    /// Adds an immutable data slot.
    pub fn add_immutable_slot(&mut self, slot: ImmutableSlot) -> usize {
        let index = self.immutables.len();
        self.immutables.push(slot);
        index
    }

    /// Returns the size in bytes of the constructor scratch area that stages
    /// immutable words before they are patched into the runtime code.
    #[must_use]
    pub fn immutable_data_len(&self) -> usize {
        self.immutables.len() * IMMUTABLE_WORD_SIZE
    }

    /// Returns an iterator over all functions.
    pub fn iter_functions(&self) -> impl Iterator<Item = (FunctionId, &Function)> {
        self.functions.iter_enumerated()
    }

    /// Returns the human-readable textual MIR representation of this module.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            if self.phase == MirPhase::default() {
                writeln!(f, "; module @{}", self.name)?;
            } else {
                writeln!(f, "; module @{} [phase = {}]", self.name, self.phase.name())?;
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

/// A data segment in the module.
#[derive(Clone, Debug)]
pub struct DataSegment {
    /// The raw bytes of this segment.
    pub data: Vec<u8>,
}

/// A storage slot in the contract.
#[derive(Clone, Debug)]
pub struct StorageSlot {
    /// The slot number.
    pub slot: u64,
    /// The offset within the slot (for packed storage).
    pub offset: u8,
    /// The type of the value stored.
    pub ty: MirType,
    /// The variable name (for debugging).
    pub name: Option<Ident>,
}

/// An immutable value staged in constructor scratch memory and patched into the
/// runtime code's immutable placeholders at deploy time.
#[derive(Clone, Debug)]
pub struct ImmutableSlot {
    /// Byte offset from the start of the immutable scratch area.
    pub offset: u32,
    /// The type of the value stored.
    pub ty: MirType,
    /// The variable name (for debugging).
    pub name: Option<Ident>,
}

impl StorageSlot {
    /// Creates a new storage slot.
    #[must_use]
    pub fn new(slot: u64, ty: MirType) -> Self {
        Self { slot, offset: 0, ty, name: None }
    }

    /// Creates a new storage slot with an offset.
    #[must_use]
    pub fn with_offset(slot: u64, offset: u8, ty: MirType) -> Self {
        Self { slot, offset, ty, name: None }
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "module {} {{", self.name)?;

        if !self.storage_layout.is_empty() {
            writeln!(f, "  storage:")?;
            for slot in &self.storage_layout {
                writeln!(f, "    slot {} @ {}: {}", slot.slot, slot.offset, slot.ty)?;
            }
            writeln!(f)?;
        }

        for (id, func) in self.functions.iter_enumerated() {
            writeln!(f, "  ; function {}", id.index())?;
            writeln!(f, "  {func}")?;
        }

        writeln!(f, "}}")
    }
}
