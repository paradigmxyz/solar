//! Declarative metadata for MIR operations.
//!
//! The table owns the typed [`InstKind`] enum as well as its compiler-facing
//! metadata. Several operations carry domain-specific layouts and
//! variable-length operands, so the generated representation remains a typed
//! Rust enum while the declaration stays in one place. The descriptors can
//! drive verification, textual serialization, and later machine serialization
//! without making the optimizer reason about untyped operands.
//!
//! Every field of an operation is either a value operand or an attribute, and
//! the [`Operands`] trait says which. Operand traversal is generated from the
//! declaration, so fields are listed in canonical operand order and tuple
//! operands carry names that document their meaning.
//!
//! The same declaration produces [`Op`], the copyable instruction view that
//! ISLE rewrite rules match on, and the ISLE prelude declaring it. Value-definition
//! extractors are generated separately so local opcode selection cannot follow
//! defining instructions or depend on a function-wide analysis context.
//!
//! `#[mir_op(...)]` attaches metadata directly to its typed operation declaration.
//! `#[when(pattern => { field: value, .. })]` overrides [`OpDef`] fields for the
//! instructions matching `pattern`, such as the spelling and effect of a calldata
//! load, without duplicating the rest of its metadata.
//! `#[commutative(lhs, rhs)]` generates the trait and canonicalizes exactly that
//! operand pair, leaving attributes and any remaining operands untouched.

use alloy_primitives::Bytes;
use solar_ir::link::LibraryId;

use super::{
    AbiEncodeMode, AbiLayoutRef, AbiParamLayoutRef, AddressCallKind, AllocationSemantics,
    ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, DataRef, DataSize, EffectKind, FrameMode,
    FrameSlotKind, Function, FunctionId, ImmutableId, InstructionMetadata, MemoryObjectKind,
    MemoryObjectLayout, MirPhase, MirType, PackedPart, PtrLocation, RevertKind, StructId, ValueId,
    ValueLayout, typing,
};
use smallvec::{Array, SmallVec, smallvec};
use std::fmt::Write as _;

/// The semantic operation family that a phase does not admit, named in phase-boundary
/// diagnostics. Each family has one required pass that removes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhaseCategory {
    AbiDecoding,
    AbiEncoding,
    AbiValidation,
    AbstractAllocation,
    /// Expanded by `lower-builtins`.
    Builtin,
    CheckedArithmetic,
    /// Expanded by `lower-checks`.
    ConditionalCheck,
    FrameSlot,
    ImmutableAssignment,
    IntegerConversion,
    MemoryZero,
    NarrowIntegerArithmetic,
    PointerArithmetic,
    Slice,
    StorageSlot,
    StructValue,
}

impl std::fmt::Display for PhaseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AbiDecoding => "ABI decoding",
            Self::AbiEncoding => "ABI encoding",
            Self::AbiValidation => "ABI validation",
            Self::AbstractAllocation => "abstract allocation",
            Self::Builtin => "builtin",
            Self::CheckedArithmetic => "checked arithmetic",
            Self::ConditionalCheck => "conditional check",
            Self::FrameSlot => "frame slot",
            Self::ImmutableAssignment => "immutable assignment",
            Self::IntegerConversion => "integer conversion",
            Self::MemoryZero => "memory zero",
            Self::NarrowIntegerArithmetic => "narrow integer arithmetic",
            Self::PointerArithmetic => "pointer arithmetic",
            Self::Slice => "slice",
            Self::StorageSlot => "storage slot",
            Self::StructValue => "struct value",
        })
    }
}

/// A compact set of MIR phases in which an operation is structurally valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhaseSet(u8);

const _: () = assert!(
    (MirPhase::Lowered as u32 + 1) < u8::BITS,
    "PhaseSet storage must be widened before adding another MIR phase"
);

impl PhaseSet {
    /// Semantic representation, before the lowered word-only boundary.
    pub(crate) const SEMANTIC: Self = Self::through(MirPhase::Semantic);
    /// All phases currently defined by MIR.
    pub(crate) const ALL: Self = Self::through(MirPhase::Lowered);
    /// Creates a set containing every phase up to and including `phase`.
    const fn through(phase: MirPhase) -> Self {
        Self((1u8 << (phase as u8 + 1)) - 1)
    }

    /// Returns whether this operation is valid in `phase`.
    pub(crate) const fn contains(self, phase: MirPhase) -> bool {
        self.0 & (1u8 << phase as u8) != 0
    }
}

/// Declarative operation properties used by analyses and rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpTraits(u16);

impl OpTraits {
    /// No additional operation traits.
    pub(crate) const NONE: Self = Self(0);
    /// The operation's binary operands may be exchanged by the scheduler.
    pub(crate) const REORDERABLE: Self = Self(1 << 0);
    /// The operation is cheap and stable enough to rematerialize when its
    /// operands can themselves be rebuilt from stable leaves.
    pub const REMATERIALIZABLE: Self = Self(1 << 1);
    /// The declared operand pair may be exchanged without changing the result.
    pub(crate) const COMMUTATIVE: Self = Self(1 << 3);
    /// The operation is a root of at least one local e-graph rewrite rule.
    pub const EGRAPH_REWRITE: Self = Self(1 << 4);

    /// Returns the union of two property sets.
    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns whether this set contains `trait_`.
    pub const fn contains(self, trait_: Self) -> bool {
        self.0 & trait_.0 == trait_.0
    }
}

/// The value an operation produces, when the operation alone determines it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResultKind {
    /// The operation produces no value.
    None,
    /// A 256-bit integer.
    I256,
    /// An integer with the same width as its operands.
    Integer,
    /// A 256-bit word read from memory: an integer, or a pointer when the loaded field holds one.
    Word,
    /// A raw memory pointer.
    MemPtr,
    /// The pointer type of the first operand, or a `ptr<memory>` when that operand is a raw word.
    Pointer,
    /// A 160-bit integer.
    I160,
    /// A one-bit integer.
    I1,
    /// A value whose type depends on the operation's attributes.
    Custom,
}

impl ResultKind {
    /// Returns the result type used when the textual form carries none.
    #[must_use]
    pub const fn default_type(self) -> Option<MirType> {
        match self {
            Self::None | Self::Custom => None,
            Self::I256 | Self::Integer | Self::Word => Some(MirType::I256),
            Self::MemPtr | Self::Pointer => Some(MirType::MEMPTR),
            Self::I160 => Some(MirType::I160),
            Self::I1 => Some(MirType::I1),
        }
    }

    /// Returns whether operations of this kind always produce a value.
    #[must_use]
    pub(crate) const fn produces_value(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Checks the exact result type; scalar conversions require explicit instructions. A pointer
    /// result admits the word it holds, as lowering erases pointer types.
    pub(crate) fn admits_type(self, ty: MirType) -> bool {
        if matches!(self, Self::I160 | Self::MemPtr | Self::Pointer) && ty == MirType::I256 {
            true
        } else if self == Self::Integer {
            matches!(ty, MirType::Int(bits) if MirType::valid_integer_width(bits.get()))
        } else if self == Self::Pointer {
            ty.is_pointer()
        } else {
            (self == Self::Word && ty == MirType::MEMPTR)
                || self.default_type().is_none_or(|expected| expected == ty)
        }
    }
}

/// How an operation accesses a raw memory range at an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawMemoryAccess {
    /// The operation reads the range.
    Read,
    /// The operation writes the whole range.
    Write,
    /// The operation writes a prefix of the range, which may be shorter or empty.
    PartialWrite,
}

/// The size of a raw memory range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawMemorySize {
    /// A fixed number of bytes.
    Bytes(u64),
    /// The number of bytes an operand holds.
    Value(ValueId),
}

impl RawMemorySize {
    /// Returns the size in bytes when it is a constant.
    pub fn constant(self, func: &Function) -> Option<u64> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Value(size) => func.value_u64(size),
        }
    }
}

/// Instruction field types, classified as value operands or attributes.
pub trait Operands {
    /// Copyable projection of the field seen by rewrite rules.
    type View: Copy;
    /// ISLE type name of the projection.
    const ISLE_TYPE: &'static str;

    /// Appends every value operand held by this field in canonical order.
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>);
    /// Visits every value operand held by this field in canonical order.
    fn visit(&self, f: &mut impl FnMut(ValueId));
    /// Visits every value operand held by this field mutably.
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId));
    /// Projects the field for rewrite rules.
    fn view(&self) -> Self::View;
    /// Applies `f` to every value operand of a projection.
    fn map_view(view: Self::View, f: &mut impl FnMut(ValueId) -> ValueId) -> Self::View;
    /// Rebuilds the field from its projection, unless the projection elided it.
    fn from_view(view: Self::View) -> Option<Self>
    where
        Self: Sized;
    /// Whether the field is exactly one value operand.
    const IS_OPERAND: bool;
    /// Builds the field from one value operand; only value operands support this.
    fn from_operand(value: ValueId) -> Self
    where
        Self: Sized;
}

impl Operands for ValueId {
    type View = Self;
    const ISLE_TYPE: &'static str = "Value";

    #[inline]
    fn collect<A: Array<Item = Self>>(&self, out: &mut SmallVec<A>) {
        out.push(*self);
    }

    #[inline]
    fn visit(&self, f: &mut impl FnMut(Self)) {
        f(*self);
    }

    #[inline]
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
    }

    #[inline]
    fn view(&self) -> Self {
        *self
    }

    #[inline]
    fn map_view(view: Self, f: &mut impl FnMut(Self) -> Self) -> Self {
        f(view)
    }

    #[inline]
    fn from_view(view: Self) -> Option<Self> {
        Some(view)
    }

    const IS_OPERAND: bool = true;

    #[inline]
    fn from_operand(value: ValueId) -> Self {
        value
    }
}

impl Operands for Option<ValueId> {
    type View = Self;
    const ISLE_TYPE: &'static str = "OptionValue";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(*self);
    }

    #[inline]
    fn visit(&self, f: &mut impl FnMut(ValueId)) {
        if let Some(value) = *self {
            f(value);
        }
    }

    #[inline]
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId)) {
        if let Some(value) = self {
            f(value);
        }
    }

    #[inline]
    fn view(&self) -> Self {
        *self
    }

    #[inline]
    fn map_view(view: Self, f: &mut impl FnMut(ValueId) -> ValueId) -> Self {
        view.map(f)
    }

    #[inline]
    fn from_view(view: Self) -> Option<Self> {
        Some(view)
    }

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("an optional operand is not built from one value")
    }
}

impl Operands for Box<[ValueId]> {
    type View = ();
    const ISLE_TYPE: &'static str = "Unit";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend_from_slice(self);
    }

    #[inline]
    fn visit(&self, f: &mut impl FnMut(ValueId)) {
        self.iter().copied().for_each(f);
    }

    #[inline]
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId)) {
        self.iter_mut().for_each(f);
    }

    #[inline]
    fn view(&self) {}

    #[inline]
    fn map_view((): (), _f: &mut impl FnMut(ValueId) -> ValueId) {}

    #[inline]
    fn from_view((): ()) -> Option<Self> {
        None
    }

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("an operand list is not built from one value")
    }
}

impl Operands for Vec<(BlockId, ValueId)> {
    type View = ();
    const ISLE_TYPE: &'static str = "Unit";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(self.iter().map(|(_, value)| *value));
    }

    #[inline]
    fn visit(&self, f: &mut impl FnMut(ValueId)) {
        self.iter().for_each(|&(_, value)| f(value));
    }

    #[inline]
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId)) {
        self.iter_mut().for_each(|(_, value)| f(value));
    }

    #[inline]
    fn view(&self) {}

    #[inline]
    fn map_view((): (), _f: &mut impl FnMut(ValueId) -> ValueId) {}

    #[inline]
    fn from_view((): ()) -> Option<Self> {
        None
    }

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("phi incoming values are not built from one value")
    }
}

impl Operands for Box<[PackedPart]> {
    type View = ();
    const ISLE_TYPE: &'static str = "Unit";
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(self.iter().filter_map(PackedPart::value));
    }
    fn visit(&self, f: &mut impl FnMut(ValueId)) {
        self.iter().filter_map(PackedPart::value).for_each(f);
    }
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId)) {
        self.iter_mut().filter_map(PackedPart::value_mut).for_each(f);
    }
    fn view(&self) {}
    fn map_view((): (), _: &mut impl FnMut(ValueId) -> ValueId) {}
    fn from_view((): ()) -> Option<Self> {
        None
    }
    const IS_OPERAND: bool = false;
    fn from_operand(_: ValueId) -> Self {
        unreachable!("packed parts need their types")
    }
}

/// The indexed topics of a log, at most four.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LogTopics {
    len: u8,
    values: [ValueId; 4],
}

impl LogTopics {
    /// The most topics a log carries.
    pub const MAX: usize = 4;

    /// Collects topics, returning `None` when there are more than [`Self::MAX`].
    pub fn new(topics: &[ValueId]) -> Option<Self> {
        let mut values = [ValueId::new(0); Self::MAX];
        values.get_mut(..topics.len())?.copy_from_slice(topics);
        Some(Self { len: topics.len() as u8, values })
    }

    /// The topics in order.
    pub(crate) fn as_slice(&self) -> &[ValueId] {
        &self.values[..usize::from(self.len)]
    }

    /// The number of topics.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        usize::from(self.len)
    }
}

impl Operands for LogTopics {
    type View = Self;
    const ISLE_TYPE: &'static str = "LogTopics";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend_from_slice(self.as_slice());
    }

    #[inline]
    fn visit(&self, f: &mut impl FnMut(ValueId)) {
        self.as_slice().iter().copied().for_each(f);
    }

    #[inline]
    fn visit_mut(&mut self, f: &mut impl FnMut(&mut ValueId)) {
        let len = self.len();
        self.values[..len].iter_mut().for_each(f);
    }

    #[inline]
    fn view(&self) -> Self {
        *self
    }

    #[inline]
    fn map_view(mut view: Self, f: &mut impl FnMut(ValueId) -> ValueId) -> Self {
        view.visit_mut(&mut |value| *value = f(*value));
        view
    }

    #[inline]
    fn from_view(view: Self) -> Option<Self> {
        Some(view)
    }

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("log topics are not built from one value")
    }
}

/// Declares field types that never hold value operands.
macro_rules! attributes {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Operands for $ty {
                type View = Self;
                const ISLE_TYPE: &'static str = stringify!($ty);

                #[inline]
                fn collect<A: Array<Item = ValueId>>(&self, _out: &mut SmallVec<A>) {}

                #[inline]
                fn visit(&self, _f: &mut impl FnMut(ValueId)) {}

                #[inline]
                fn visit_mut(&mut self, _f: &mut impl FnMut(&mut ValueId)) {}

                #[inline]
                fn view(&self) -> Self {
                    *self
                }

                #[inline]
                fn map_view(view: Self, _f: &mut impl FnMut(ValueId) -> ValueId) -> Self {
                    view
                }

                #[inline]
                fn from_view(view: Self) -> Option<Self> {
                    Some(view)
                }

                const IS_OPERAND: bool = false;

                fn from_operand(_: ValueId) -> Self {
                    unreachable!("an attribute is not built from a value operand")
                }
            }
        )+
    };
}

/// Declares attribute types that rewrite rules cannot inspect.
macro_rules! opaque_attributes {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Operands for $ty {
                type View = ();
                const ISLE_TYPE: &'static str = "Unit";

                #[inline]
                fn collect<A: Array<Item = ValueId>>(&self, _out: &mut SmallVec<A>) {}

                #[inline]
                fn visit(&self, _f: &mut impl FnMut(ValueId)) {}

                #[inline]
                fn visit_mut(&mut self, _f: &mut impl FnMut(&mut ValueId)) {}

                #[inline]
                fn view(&self) {}

                #[inline]
                fn map_view((): (), _f: &mut impl FnMut(ValueId) -> ValueId) {}

                #[inline]
                fn from_view((): ()) -> Option<Self> {
                    None
                }

                const IS_OPERAND: bool = false;

                fn from_operand(_: ValueId) -> Self {
                    unreachable!("an attribute is not built from a value operand")
                }
            }
        )+
    };
}

/// Generates one `FunctionBuilder` method for an operation marked
/// `#[builder(name)]` or `#[builder(name, void)]`, taking its value operands in
/// declaration order. The result type comes from the operation's result kind.
/// Tuple operands and nullary operations are generated; operations with
/// attribute-dependent results keep a hand-written builder.
macro_rules! builder_method {
    ($inst:ident::$variant:ident; []; ($($operand:ident)*); {$($field:ident)*}) => {};
    ($inst:ident::$variant:ident; [$name:ident]; (); {}) => {
        impl crate::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub fn $name(&mut self) -> ValueId {
                let kind = $inst::$variant;
                let ty = kind.op_def().result.default_type();
                // %result = op()
                self.emit_inst(kind, ty)
            }
        }
    };
    ($inst:ident::$variant:ident; [$name:ident]; ($($operand:ident)+); {}) => {
        impl crate::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub fn $name(&mut self $(, $operand: ValueId)+) -> ValueId {
                let kind = $inst::$variant($($operand),+);
                let ty = kind.op_def().result.default_type();
                // %result = op(operands)
                self.emit_inst(kind, ty)
            }
        }
    };
    ($inst:ident::$variant:ident; [$name:ident, void]; ($($operand:ident)+); {}) => {
        impl crate::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub fn $name(&mut self $(, $operand: ValueId)+) {
                // op(operands)
                self.emit_void_inst($inst::$variant($($operand),+));
            }
        }
    };
}

/// Returns the ISLE term name of an operation: the lower-cased variant name,
/// with the bitwise operations named as in Cranelift because `and` is an ISLE
/// keyword.
fn isle_op_name(variant: &str) -> String {
    match variant {
        "And" => "band".into(),
        "Or" => "bor".into(),
        "Xor" => "bxor".into(),
        "Not" => "bnot".into(),
        _ => variant.to_ascii_lowercase(),
    }
}

type OptionU64 = Option<u64>;

attributes! {
    bool, StructId, MirType, ValueLayout, AddressCallKind, CheckedOp, ArithmeticKind, OptionU64,
    u32,
    u64,
    LibraryId,
    AbiEncodeMode,
    AllocationSemantics,
    DataRef,
    DataSize,
    FrameMode,
    FrameSlotKind,
    FunctionId,
    ImmutableId,
    MemoryObjectKind,
    MemoryObjectLayout,
    PtrLocation,
}

opaque_attributes! {
    Bytes, Callee, RevertKind,
    AbiLayoutRef,
    AbiParamLayoutRef,
}

/// Emits a wildcard for a named tuple field in a generated match.
macro_rules! ignore_field {
    ($field:ident) => {
        _
    };
}

/// Marks only operations that declare a commutative operand pair.
macro_rules! commutative_trait {
    () => {
        OpTraits::NONE
    };
    ($lhs:ident, $rhs:ident) => {
        OpTraits::COMMUTATIVE
    };
}

/// Emits the size of a raw memory range declared with `#[raw_memory]`.
macro_rules! raw_memory_size {
    ($bytes:literal) => {
        RawMemorySize::Bytes($bytes)
    };
    ($size:ident) => {
        RawMemorySize::Value(*$size)
    };
}

/// Visits the size operand of a raw memory range declared with `#[raw_memory]`, if it has one.
macro_rules! visit_raw_memory_size {
    ($f:ident, $bytes:literal) => {};
    ($f:ident, $size:ident) => {
        $f($size)
    };
}

macro_rules! define_mir_ops {
    (
        enum $inst_name:ident {
            $(
                $(#[doc = $doc:expr])*
                #[mir_op(
                    mnemonic = $mnemonic:literal,
                    result = $result:ident,
                    phases = $phases:expr,
                    effect = $effect:ident,
                    traits = $traits:expr,
                    side_effects = $side_effects:expr,
                    category = $category:expr $(,)?
                )]
                $(#[when($when_pattern:pat => { $( $when_field:ident : $when_value:expr ),+ $(,)? })])*
                $(#[commutative($lhs:ident, $rhs:ident)])?
                $(#[builder($builder:ident $(, $void:ident)?)])?
                $(#[raw_memory($( $access:ident($address:ident, $size:tt) $(if $guard:expr)? ),+)])?
                #[operand_types($func:ident => $operand_types:expr)]
                $variant:ident
                $( ( $( $operand:ident : $operand_ty:ty ),+ $(,)? ) )?
                $( { $( $(#[$field_meta:meta])* $field:ident : $field_ty:ty ),+ $(,)? } )?
            ),+ $(,)?
        }
    ) => {
        /// The kind of a MIR instruction.
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $inst_name {
            $(
                $(#[doc = $doc])*
                $variant
                $( ( $( $operand_ty ),+ ) )?
                $( { $( $(#[$field_meta])* $field: $field_ty ),+ } )?,
            )+
        }

        /// Generated metadata for one MIR operation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct OpDef {
            /// Canonical textual operation name.
            pub mnemonic: &'static str,
            /// Value produced by the operation.
            pub result: ResultKind,
            /// Phases in which this operation is valid.
            pub(crate) phases: PhaseSet,
            /// Conservative effect classification.
            pub effect: EffectKind,
            /// Declarative operation properties.
            pub traits: OpTraits,
            /// Whether the operation must remain observable to DCE.
            pub has_side_effects: bool,
            /// Diagnostic category used when a phase boundary is violated.
            pub phase_category: Option<PhaseCategory>,
        }

        impl $inst_name {
            /// Returns the schema's exact operand signature, or `None` when result or module
            /// information is required by the dedicated type checker.
            #[allow(unused_variables)]
            pub(crate) fn operand_types(&self, function: &Function) -> Option<SmallVec<[MirType; 8]>> {
                match self {
                    $(
                        Self::$variant
                        $( ( $( $operand ),+ ) )?
                        $( { $( $field ),+ } )? => {
                            let $func = function;
                            $operand_types
                        },
                    )+
                }
            }

            /// Returns the declarative definition for this operation.
            #[inline]
            #[must_use]
            pub const fn op_def(&self) -> &'static OpDef {
                match self {
                    $(
                        Self::$variant $( ( $( ignore_field!($operand) ),+ ) )? $( { $( $field: _ ),+ } )? => {
                            const DEF: OpDef = OpDef {
                                mnemonic: $mnemonic,
                                result: ResultKind::$result,
                                phases: $phases,
                                effect: EffectKind::$effect,
                                traits: $traits.union(commutative_trait!($($lhs, $rhs)?)),
                                has_side_effects: $side_effects,
                                phase_category: $category,
                            };
                            match self {
                                $( $when_pattern => &OpDef { $( $when_field: $when_value, )+ ..DEF }, )*
                                _ => &DEF,
                            }
                        },
                    )+
                }
            }

            /// Collects every value operand in canonical order.
            ///
            /// This is the canonical operand list for liveness and scheduling.
            pub(crate) fn collect_operands<A: Array<Item = ValueId>>(
                &self,
                out: &mut SmallVec<A>,
            ) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $( Operands::collect($operand, out); )+ )?
                            $( $( Operands::collect($field, out); )+ )?
                        }
                    )+
                }
            }

            /// Visits every value operand in canonical order, without collecting them.
            #[inline]
            pub fn visit_operands(&self, mut f: impl FnMut(ValueId)) {
                self.visit_operands_dyn(&mut f);
            }

            /// Shares one copy of the operand match between all visitors, instead of
            /// instantiating it for every closure.
            fn visit_operands_dyn(&self, mut f: &mut dyn FnMut(ValueId)) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $( Operands::visit($operand, &mut f); )+ )?
                            $( $( Operands::visit($field, &mut f); )+ )?
                        }
                    )+
                }
            }

            /// Visits every value operand mutably, in canonical order.
            pub fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $( Operands::visit_mut($operand, &mut f); )+ )?
                            $( $( Operands::visit_mut($field, &mut f); )+ )?
                        }
                    )+
                }
            }

            /// Visits each raw memory range the operation reads or writes, with its address and
            /// size, reads before writes. Operations that reach memory only through
            /// slices, frames, or the free memory pointer have none.
            #[allow(unused_variables)]
            pub fn visit_raw_memory(
                &self,
                mut f: impl FnMut(RawMemoryAccess, ValueId, RawMemorySize),
            ) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $(
                                if true $( && $guard )? {
                                    f(RawMemoryAccess::$access, *$address, raw_memory_size!($size));
                                }
                            )+ )?
                        }
                    )+
                }
            }

            /// Visits the offset and size operands of the raw memory ranges the operation
            /// accesses, mutably.
            #[allow(unused_variables)]
            pub fn visit_raw_memory_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $(
                                if true $( && $guard )? {
                                    f($address);
                                    visit_raw_memory_size!(f, $size);
                                }
                            )+ )?
                        }
                    )+
                }
            }

            /// Returns the arity and constructor of an operation that is built
            /// from value operands alone, by textual mnemonic.
            #[must_use]
            pub(crate) fn operand_only(
                mnemonic: &str,
            ) -> Option<(usize, fn(&[ValueId]) -> Self)> {
                match mnemonic {
                    $(
                        $mnemonic => build::$variant::operand_only(),
                    )+
                    _ => None,
                }
            }

            /// Returns the rewrite-rule view of this instruction.
            #[must_use]
            pub fn op(&self) -> Op {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => Op::$variant
                            $( { $( $operand: Operands::view($operand) ),+ } )?
                            $( { $( $field: Operands::view($field) ),+ } )?,
                    )+
                }
            }

            /// Returns the diagnostic category of an operation that `phase` does not admit.
            #[must_use]
            pub(crate) fn phase_violation(
                &self,
                phase: MirPhase,
                metadata: &InstructionMetadata,
            ) -> Option<PhaseCategory> {
                let definition = self.op_def();
                if !definition.phases.contains(phase) {
                    return definition.phase_category;
                }
                if let Self::Alloc { semantics, .. } = self
                    && phase >= MirPhase::Lowered
                    && (!metadata.deferred_alloc() || *semantics != AllocationSemantics::INTERNAL)
                {
                    return Some(PhaseCategory::AbstractAllocation);
                }
                None
            }
        }

        /// Per-operation constructors from value operands, for the textual
        /// parser and the generated builders.
        #[allow(non_snake_case)]
        pub(crate) mod build {
            $(
                pub(crate) mod $variant {
                    use super::super::*;

                    /// Whether every field is a value operand.
                    pub(crate) const OPERAND_ONLY: bool = true
                        $( $( && <$operand_ty as Operands>::IS_OPERAND )+ )?
                        $( $( && <$field_ty as Operands>::IS_OPERAND )+ )?;

                    /// Number of fields.
                    pub(crate) const ARITY: usize = 0
                        $( $( + { let _ = stringify!($operand); 1 } )+ )?
                        $( $( + { let _ = stringify!($field); 1 } )+ )?;

                    /// Builds the operation from its operands in declaration order.
                    #[allow(unused_mut, unused_variables)]
                    pub(crate) fn from_operands(operands: &[ValueId]) -> $inst_name {
                        debug_assert_eq!(operands.len(), ARITY);
                        let mut operands = operands.iter().copied();
                        $inst_name::$variant
                            $( ( $( <$operand_ty as Operands>::from_operand(
                                operands.next().expect("arity")
                            ) ),+ ) )?
                            $( { $( $field: <$field_ty as Operands>::from_operand(
                                operands.next().expect("arity")
                            ) ),+ } )?
                    }

                    /// Returns the arity and constructor when every field is a value operand.
                    pub(crate) fn operand_only() -> Option<(usize, fn(&[ValueId]) -> $inst_name)> {
                        OPERAND_ONLY.then_some((ARITY, from_operands as fn(&[ValueId]) -> $inst_name))
                    }
                }
            )+
        }

        $(
            builder_method! {
                $inst_name::$variant;
                [ $( $builder $(, $void)? )? ];
                ( $( $( $operand )+ )? );
                { $( $( $field )+ )? }
            }
        )+

        /// Copyable view of an instruction for rewrite rules.
        ///
        /// Value operands keep their identity, attributes are carried by value,
        /// and variable-length payloads are elided.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Op {
            $(
                $variant
                $( { $( $operand: <$operand_ty as Operands>::View ),+ } )?
                $( { $( $field: <$field_ty as Operands>::View ),+ } )?,
            )+
        }

        impl Op {
            /// Returns the declared result kind without rebuilding an instruction.
            pub const fn result_kind(self) -> ResultKind {
                match self {
                    $(
                        Self::$variant $( { $( $operand: _ ),+ } )? $( { $( $field: _ ),+ } )? => ResultKind::$result,
                    )+
                }
            }

            /// Returns the first value operand retained by the rewrite view.
            pub fn first_operand(self) -> Option<ValueId> {
                let mut first = None;
                let _ = self.map_values(|value| {
                    first.get_or_insert(value);
                    value
                });
                first
            }

            /// Orders each declared commutative pair for value-numbering keys.
            /// Other operands, including a modular operation's modulus, stay in place.
            pub fn canonicalize_commutative(self) -> Self {
                self.canonicalize_commutative_by_key(|value| value.index())
            }

            /// Orders only the declared commutative pair by the caller's key.
            pub fn canonicalize_commutative_by_key<K: Ord>(
                self,
                mut key: impl FnMut(ValueId) -> K,
            ) -> Self {
                match self {
                    $(
                        Self::$variant $( { $( $operand ),+ } )? $( { $( $field ),+ } )? => {
                            $(
                                // op(lhs, rhs, rest) -> op(lhs, rhs, rest), ordered by key
                                let ($lhs, $rhs) = if key($rhs) < key($lhs) {
                                    ($rhs, $lhs)
                                } else {
                                    ($lhs, $rhs)
                                };
                            )?
                            Self::$variant
                                $( { $( $operand ),+ } )?
                                $( { $( $field ),+ } )?
                        }
                    )+
                }
            }

            /// Every operation with its field names and ISLE types, in declaration order.
            const FIELDS: &'static [(&'static str, &'static [(&'static str, &'static str)])] = &[
                $(
                    (stringify!($variant), &[
                        $( $( (stringify!($operand), <$operand_ty as Operands>::ISLE_TYPE), )+ )?
                        $( $( (stringify!($field), <$field_ty as Operands>::ISLE_TYPE), )+ )?
                    ]),
                )+
            ];

            /// Rebuilds the instruction, unless a payload was elided from the view.
            #[must_use]
            pub fn into_kind(self) -> Option<$inst_name> {
                Some(match self {
                    $(
                        Self::$variant $( { $( $operand ),+ } )? $( { $( $field ),+ } )? => $inst_name::$variant
                            $( ( $( <$operand_ty as Operands>::from_view($operand)? ),+ ) )?
                            $( { $( $field: <$field_ty as Operands>::from_view($field)? ),+ } )?,
                    )+
                })
            }

            /// Applies `f` to every value operand.
            #[must_use]
            pub fn map_values(self, mut f: impl FnMut(ValueId) -> ValueId) -> Self {
                match self {
                    $(
                        Self::$variant $( { $( $operand ),+ } )? $( { $( $field ),+ } )? => Self::$variant
                            $( { $( $operand: <$operand_ty as Operands>::map_view($operand, &mut f) ),+ } )?
                            $( { $( $field: <$field_ty as Operands>::map_view($field, &mut f) ),+ } )?,
                    )+
                }
            }
        }
    };
}

impl Op {
    /// Returns the ISLE declarations of the view: its primitive types, the
    /// `Op` enum, and one extractor per operation matching the instruction
    /// that defines a value.
    #[must_use]
    pub fn isle_prelude() -> String {
        let mut out = String::from(
            ";; Generated from the MIR operation schema by `Op::isle_prelude`; do not edit.\n\
             ;; `cargo nextest run -p solar-codegen isle_prelude` checks this file and\n\
             ;; `SNAPSHOTS=overwrite` refreshes it.\n\n\
             (type Value (primitive Value))\n\
             (type U256 (primitive U256))\n",
        );
        let mut declared = vec!["Value", "U256", "u32", "u64", "bool"];
        for (_, fields) in Self::FIELDS {
            for (_, ty) in *fields {
                if !declared.contains(ty) {
                    declared.push(ty);
                    writeln!(out, "(type {ty} (primitive {ty}))").unwrap();
                }
            }
        }

        out.push_str("\n(type Op extern (enum\n");
        for (variant, fields) in Self::FIELDS {
            write!(out, "  ({variant}").unwrap();
            for (name, ty) in *fields {
                write!(out, " ({name} {ty})").unwrap();
            }
            out.push_str(")\n");
        }
        out.push_str("))\n");
        out
    }

    /// Returns value-definition extractors for rules that inspect MIR operands.
    #[must_use]
    pub fn isle_extractors() -> String {
        let mut out = String::from(
            ";; Generated from the MIR operation schema by `Op::isle_extractors`; do not edit.\n\
             ;; `cargo nextest run -p solar-codegen isle_prelude` checks this file and\n\
             ;; `SNAPSHOTS=overwrite` refreshes it.\n\n\
             ;; The instruction defining a value.\n\
             (decl inst (Op) Value)\n\
             (extern extractor inst inst_data)\n",
        );
        for (variant, fields) in Self::FIELDS {
            let name = isle_op_name(variant);
            write!(out, "\n(decl {name} (").unwrap();
            for (index, (_, ty)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                out.push_str(ty);
            }
            write!(out, ") Value)\n(extractor ({name}").unwrap();
            for (field, _) in *fields {
                write!(out, " {field}").unwrap();
            }
            write!(out, ") (inst (Op.{variant}").unwrap();
            for (field, _) in *fields {
                write!(out, " {field}").unwrap();
            }
            out.push_str(")))\n");
        }
        out
    }
}

define_mir_ops! {
    enum InstKind {
    #[mir_op(mnemonic = "insert_value", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = false, category = Some(PhaseCategory::StructValue))]
    #[operand_types(func => None)]
    InsertValue { ty: MirType, aggregate: ValueId, index: u32, value: ValueId },
    #[mir_op(mnemonic = "extract_value", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = false, category = Some(PhaseCategory::StructValue))]
    #[when(InstKind::ExtractValue { ty: MirType::Slice(_), .. } => {
        phase_category: Some(PhaseCategory::Slice),
    })]
    #[operand_types(func => None)]
    ExtractValue { ty: MirType, aggregate: ValueId, index: u32 },
    #[mir_op(mnemonic = "zext", result = Custom, phases = PhaseSet::ALL,
        effect = Pure, traits = OpTraits::EGRAPH_REWRITE, side_effects = false, category = None)]
    #[operand_types(func => None)]
    Zext(operand0: ValueId),
    #[mir_op(mnemonic = "trunc", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::EGRAPH_REWRITE, side_effects = false, category = Some(PhaseCategory::IntegerConversion))]
    #[operand_types(func => None)]
    Trunc(operand0: ValueId, bits: u32),
    #[mir_op(mnemonic = "sext", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::EGRAPH_REWRITE, side_effects = false, category = Some(PhaseCategory::IntegerConversion))]
    #[operand_types(func => None)]
    Sext(operand0: ValueId, from_bits: u32, to_bits: u32),
    #[mir_op(mnemonic = "ptrtoint", result = Custom, phases = PhaseSet::ALL,
        effect = Pure, traits = OpTraits::EGRAPH_REWRITE, side_effects = false, category = None)]
    #[when(InstKind::PtrToInt(_, 0..=255) => {
        phases: PhaseSet::SEMANTIC,
        phase_category: Some(PhaseCategory::IntegerConversion),
    })]
    #[operand_types(func => None)]
    PtrToInt(operand0: ValueId, bits: u32),
    #[mir_op(mnemonic = "inttoptr", result = Custom, phases = PhaseSet::ALL,
        effect = Pure, traits = OpTraits::EGRAPH_REWRITE, side_effects = false, category = None)]
    #[operand_types(func => None)]
    IntToPtr(operand0: ValueId),
    /// Offset a pointer by an unsigned byte count, keeping its location type. The sum wraps at
    /// 256 bits: a forged object pointer or length can admit an index outside the allocation.
    /// The operation alone proves neither bounds nor disjointness from other objects.
    #[mir_op(mnemonic = "ptradd", result = Pointer, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = false, category = Some(PhaseCategory::PointerArithmetic))]
    #[operand_types(func => Some(smallvec![typing::pointer(func, *ptr), MirType::I256]))]
    PtrAdd(ptr: ValueId, offset: ValueId),
    #[mir_op(mnemonic = "checked_binary", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::CheckedArithmetic))]
    #[operand_types(func => Some(smallvec![arithmetic.ty().mir_type(), if *op == CheckedOp::Pow { MirType::I256 } else { arithmetic.ty().mir_type() }]))]
    CheckedBinary {
        op: CheckedOp,
        arithmetic: ArithmeticKind,
        lhs: ValueId,
        rhs: ValueId,
    },
    #[mir_op(mnemonic = "validate_storage_bytes", result = None, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    ValidateStorageBytes(operand0: ValueId),
    #[mir_op(mnemonic = "load_storage_bytes", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    StorageBytesLoad(operand0: ValueId),
    #[mir_op(mnemonic = "load_storage_array", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    StorageArrayLoad { slot: ValueId, element: ValueLayout, enum_variants: Option<u64> },
    #[mir_op(mnemonic = "store_storage_bytes", result = None, phases = PhaseSet::SEMANTIC,
        effect = StorageWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::MEMPTR]))]
    StorageBytesStore(operand0: ValueId, operand1: ValueId),
    #[mir_op(mnemonic = "store_storage_bytes_literal", result = None, phases = PhaseSet::SEMANTIC,
        effect = StorageWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    StorageBytesStoreLiteral { slot: ValueId, bytes: Bytes },
    #[mir_op(mnemonic = "clear_storage_words", result = None, phases = PhaseSet::SEMANTIC,
        effect = StorageWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, MirType::I256]))]
    StorageClearWords(operand0: ValueId, operand1: ValueId, operand2: ValueId),
    #[mir_op(mnemonic = "validate_abi", result = None, phases = PhaseSet::SEMANTIC,
        effect = Pure, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::AbiValidation))]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    ValidateAbi(operand0: ValueId),
    #[mir_op(mnemonic = "abi_encode_packed", result = Custom, phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(typing::packed_types(func, parts)))]
    AbiEncodePacked { parts: Box<[PackedPart]>, hash: bool },
    #[mir_op(mnemonic = "address_call", result = I1, phases = PhaseSet::SEMANTIC,
        effect = ExternalCall, traits = OpTraits::NONE, side_effects = true, category = Some(PhaseCategory::Builtin))]
    #[operand_types(func => Some(typing::address_call_types(func, *input, gas.is_some(), value.is_some())))]
    AddressCall {
        kind: AddressCallKind,
        address: ValueId,
        input: ValueId,
        gas: Option<ValueId>,
        value: Option<ValueId>,
    },

    // Arithmetic operations
    /// Addition: `a + b`
    #[mir_op(
        mnemonic = "add",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(add)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Add(a: ValueId, b: ValueId),
    /// Subtraction: `a - b`
    #[mir_op(
        mnemonic = "sub",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(sub)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Sub(a: ValueId, b: ValueId),
    /// Multiplication: `a * b`
    #[mir_op(
        mnemonic = "mul",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(mul)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Mul(a: ValueId, b: ValueId),
    /// Unsigned division: `a / b`
    #[mir_op(
        mnemonic = "div",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(div)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Div(a: ValueId, b: ValueId),
    /// Signed division: `a / b`
    #[mir_op(
        mnemonic = "sdiv",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(sdiv)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    SDiv(a: ValueId, b: ValueId),
    /// Unsigned modulo: `a % b`
    #[mir_op(
        mnemonic = "mod",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(mod_)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Mod(a: ValueId, b: ValueId),
    /// Signed modulo: `a % b`
    #[mir_op(
        mnemonic = "smod",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(smod)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    SMod(a: ValueId, b: ValueId),
    /// Exponentiation: `a ** b`
    #[mir_op(
        mnemonic = "exp",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(exp)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Exp(a: ValueId, b: ValueId),
    /// Add modulo: `(a + b) % n`
    #[mir_op(
        mnemonic = "addmod",
        result = I256,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(addmod)]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, MirType::I256]))]
    AddMod(a: ValueId, b: ValueId, n: ValueId),
    /// Multiply modulo: `(a * b) % n`
    #[mir_op(
        mnemonic = "mulmod",
        result = I256,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(mulmod)]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, MirType::I256]))]
    MulMod(a: ValueId, b: ValueId, n: ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    #[mir_op(
        mnemonic = "and",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(and)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    And(a: ValueId, b: ValueId),
    /// Bitwise OR: `a | b`
    #[mir_op(
        mnemonic = "or",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(or)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Or(a: ValueId, b: ValueId),
    /// Bitwise XOR: `a ^ b`
    #[mir_op(
        mnemonic = "xor",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(xor)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Xor(a: ValueId, b: ValueId),
    /// Bitwise NOT: `~a`
    #[mir_op(
        mnemonic = "not",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(not)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 1]))]
    Not(a: ValueId),
    /// Count leading zero bits.
    #[mir_op(
        mnemonic = "clz",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(clz)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 1]))]
    Clz(a: ValueId),
    /// Left shift: `value << shift`.
    ///
    /// Shift operands are stored in EVM stack order, `shift` first, so selection and rewrite rules
    /// match the opcodes directly. The builder, printer, and parser use `value, shift`.
    #[mir_op(
        mnemonic = "shl",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *shift); 2]))]
    Shl(shift: ValueId, value: ValueId),
    /// Logical right shift: `value >> shift`, with shift operands stored like [`Self::Shl`].
    #[mir_op(
        mnemonic = "shr",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *shift); 2]))]
    Shr(shift: ValueId, value: ValueId),
    /// Arithmetic right shift: `value >> shift` (signed), with shift operands stored like
    /// [`Self::Shl`].
    #[mir_op(
        mnemonic = "sar",
        result = Integer,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *shift); 2]))]
    Sar(shift: ValueId, value: ValueId),
    /// Extract a byte: `byte(i, x)`
    #[mir_op(
        mnemonic = "byte",
        result = I256,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(byte)]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256]))]
    Byte(index: ValueId, value: ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    #[mir_op(
        mnemonic = "lt",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(lt)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Lt(a: ValueId, b: ValueId),
    /// Greater than (unsigned): `a > b`
    #[mir_op(
        mnemonic = "gt",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(gt)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Gt(a: ValueId, b: ValueId),
    /// Less than (signed): `a < b`
    #[mir_op(
        mnemonic = "slt",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(slt)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    SLt(a: ValueId, b: ValueId),
    /// Greater than (signed): `a > b`
    #[mir_op(
        mnemonic = "sgt",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(sgt)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    SGt(a: ValueId, b: ValueId),
    /// Equality: `a == b`
    #[mir_op(
        mnemonic = "eq",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(eq)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Eq(a: ValueId, b: ValueId),
    /// Inequality: `a != b`.
    #[mir_op(
        mnemonic = "ne",
        result = I1,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(ne)]
    #[operand_types(func => Some(smallvec![typing::integer_type(func, *a); 2]))]
    Ne(a: ValueId, b: ValueId),

    // Memory operations
    /// Load the word at `ptr` from memory, calldata, or returndata, or the word in storage or
    /// transient storage slot `ptr`. Like an LLVM load from `ptr`, the address may be a pointer
    /// into the same location; lowering casts it at the opcode boundary.
    #[mir_op(
        mnemonic = "mload",
        result = Word,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[when(InstKind::Load(_, PtrLocation::Calldata) => {
        mnemonic: "calldataload",
        effect: EffectKind::EnvironmentRead,
    })]
    #[when(InstKind::Load(_, PtrLocation::Returndata) => {
        mnemonic: "returndataload",
        effect: EffectKind::EnvironmentRead,
    })]
    #[when(InstKind::Load(_, PtrLocation::Storage) => {
        mnemonic: "sload",
        effect: EffectKind::StorageRead,
    })]
    #[when(InstKind::Load(_, PtrLocation::Transient) => {
        mnemonic: "tload",
        effect: EffectKind::TransientRead,
    })]
    #[raw_memory(Read(ptr, 32) if *location == PtrLocation::Memory)]
    #[operand_types(func => Some(smallvec![typing::address(func, *ptr, *location)]))]
    Load(ptr: ValueId, location: PtrLocation),
    /// Store a word to memory at `ptr`, which may be a `ptr<memory>`, as for `mload`, or to
    /// storage or transient storage slot `ptr`.
    #[mir_op(
        mnemonic = "mstore",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[when(InstKind::Store(.., PtrLocation::Storage) => {
        mnemonic: "sstore",
        effect: EffectKind::StorageWrite,
    })]
    #[when(InstKind::Store(.., PtrLocation::Transient) => {
        mnemonic: "tstore",
        effect: EffectKind::TransientWrite,
    })]
    #[raw_memory(Write(ptr, 32) if *location == PtrLocation::Memory)]
    #[operand_types(func => Some(smallvec![typing::address(func, *ptr, *location), MirType::I256]))]
    Store(ptr: ValueId, value: ValueId, location: PtrLocation),
    /// Store the low byte of `value` to memory at `ptr`, as for `mstore`.
    #[mir_op(
        mnemonic = "mstore8",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Write(ptr, 1))]
    #[operand_types(func => Some(smallvec![typing::address(func, *ptr, PtrLocation::Memory), MirType::I256]))]
    Store8(ptr: ValueId, value: ValueId),
    /// Set a contiguous memory range to zero: `memory_zero(offset, size)`
    #[mir_op(
        mnemonic = "memory_zero",
        result = None,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::MemoryZero)
    )]
    #[builder(memory_zero, void)]
    #[raw_memory(Write(offset, size))]
    #[operand_types(func => Some(smallvec![typing::address(func, *offset, PtrLocation::Memory), MirType::I256]))]
    MemoryZero(offset: ValueId, size: ValueId),
    /// Get memory size: `msize()`
    #[mir_op(
        mnemonic = "msize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(msize)]
    #[operand_types(func => Some(smallvec![]))]
    MSize,
    /// Read the free-memory pointer.
    #[mir_op(
        mnemonic = "fmp",
        result = MemPtr,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::AbstractAllocation)
    )]
    #[builder(fmp)]
    #[operand_types(func => Some(smallvec![]))]
    Fmp,
    /// Set the free-memory pointer.
    #[mir_op(
        mnemonic = "set_fmp",
        result = None,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::AbstractAllocation)
    )]
    #[operand_types(func => Some(smallvec![MirType::MEMPTR]))]
    SetFmp(value: ValueId),
    /// The lowest word the free-memory pointer may hold: the initial pointer of every entry that
    /// can run this function, which codegen fixes once it has placed the frames and spill slots
    /// below it.
    #[mir_op(
        mnemonic = "heap_floor",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(heap_floor)]
    #[operand_types(func => Some(smallvec![]))]
    HeapFloor,
    /// Reserve memory and return the previous free-memory pointer.
    #[mir_op(
        mnemonic = "alloc",
        result = MemPtr,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::AbstractAllocation)
    )]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    Alloc {
        /// Requested byte count.
        size: ValueId,
        /// Initialization and failure behavior.
        semantics: AllocationSemantics,
    },
    /// ABI-encode values into memory.
    #[mir_op(
        mnemonic = "abi_encode",
        result = Custom,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::AbiEncoding)
    )]
    #[operand_types(func => Some(typing::abi_encode_types(func, selector.is_some(), args, layout)))]
    AbiEncode {
        /// Storage policy for the encoded result.
        mode: AbiEncodeMode,
        /// Optional left-aligned four-byte selector prefix.
        selector: Option<ValueId>,
        /// Values corresponding to the tuple layout.
        args: Box<[ValueId]>,
        /// Interned semantic ABI layout.
        layout: AbiLayoutRef,
    },
    /// Decode a memory-backed ABI tuple into semantic MIR values.
    ///
    /// The instruction result is the first tuple value. Additional values are
    /// published through the multi-return buffer, matching ordinary MIR calls.
    /// An empty tuple has no result and only checks the data length.
    #[mir_op(
        mnemonic = "abi_decode",
        result = Custom,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::AbiDecoding)
    )]
    #[operand_types(func => Some(smallvec![typing::read_object(func, *data)]))]
    AbiDecode {
        /// ABI-encoded bytes object, or a memory or calldata slice of the encoding.
        data: ValueId,
        /// Interned ABI input layout, including scalar validation types.
        layout: AbiParamLayoutRef,
    },
    /// Copy `len` bytes from `src` in memory, calldata, or returndata to `dest` in memory.
    #[mir_op(
        mnemonic = "mcopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[when(InstKind::Copy(.., PtrLocation::Calldata) => { mnemonic: "calldatacopy" })]
    #[when(InstKind::Copy(.., PtrLocation::Returndata) => { mnemonic: "returndatacopy" })]
    #[raw_memory(Read(src, len) if *location == PtrLocation::Memory, Write(dest, len))]
    #[operand_types(func => Some(smallvec![typing::address(func, *dest, PtrLocation::Memory), typing::address(func, *src, *location), MirType::I256]))]
    Copy(dest: ValueId, src: ValueId, len: ValueId, location: PtrLocation),

    // Calldata operations
    /// Get calldata size: `calldatasize()`
    #[mir_op(
        mnemonic = "calldatasize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(calldatasize)]
    #[operand_types(func => Some(smallvec![]))]
    CalldataSize,
    /// Construct a logical `(pointer, length, location)` slice.
    #[mir_op(
        mnemonic = "make_memory_slice",
        result = Custom,
        phases = PhaseSet::SEMANTIC,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::Slice)
    )]
    #[when(InstKind::MakeSlice { location: PtrLocation::Calldata, .. } => {
        mnemonic: "make_calldata_slice",
    })]
    #[when(InstKind::MakeSlice { location: PtrLocation::Returndata, .. } => {
        mnemonic: "make_returndata_slice",
    })]
    #[operand_types(func => Some(smallvec![MirType::Ptr(*location), MirType::I256]))]
    MakeSlice {
        /// Pointer to the first element or byte.
        ptr: ValueId,
        /// Logical element or byte length.
        len: ValueId,
        /// Address space containing the slice data.
        location: PtrLocation,
    },
    /// Address inside the current internal-call frame.
    #[mir_op(
        mnemonic = "internal_frame_addr",
        result = MemPtr,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![]))]
    InternalFrameAddr(offset: u64),
    /// Load a mutable local through its logical frame slot.
    ///
    /// A plain memory read: deletable when its result is dead. Ordering
    /// against frame stores, calls, and other frame traffic is carried by
    /// effect kinds and the alias model's `frame_location`.
    #[mir_op(
        mnemonic = "frame_load",
        result = Custom,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::FrameSlot)
    )]
    #[operand_types(func => Some(smallvec![]))]
    FrameLoad {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
    },
    /// Store a mutable local through its logical frame slot.
    #[mir_op(
        mnemonic = "frame_store",
        result = None,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::FrameSlot)
    )]
    #[operand_types(func => Some(smallvec![kind.result_type()]))]
    FrameStore {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
        /// Value to store.
        value: ValueId,
    },
    /// Base address of the constructor's copied ABI argument blob.
    #[mir_op(
        mnemonic = "constructor_args_base",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(constructor_args_base)]
    #[operand_types(func => Some(smallvec![]))]
    ConstructorArgsBase,
    /// End address of the constructor's copied ABI argument blob.
    #[mir_op(
        mnemonic = "constructor_args_end",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(constructor_args_end)]
    #[operand_types(func => Some(smallvec![]))]
    ConstructorArgsEnd,

    // Code operations
    /// Copy constant module data to memory.
    #[mir_op(
        mnemonic = "datacopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::REORDERABLE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Write(dest, size))]
    #[operand_types(func => Some(smallvec![typing::address(func, *dest, PtrLocation::Memory), MirType::I256]))]
    DataCopy(data: DataRef, dest: ValueId, size: ValueId),
    /// Byte length of deferred module data, such as another contract's
    /// bytecode, plus an addend, rounded down to a multiple of 32 when aligned.
    ///
    /// The length is only known once the data is linked during final assembly. Folding
    /// the addend and rounding into the operation keeps sizes derived from
    /// the length constant after resolution.
    #[mir_op(
        mnemonic = "datasize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![]))]
    DataSize(size: DataSize),
    /// Get code size: `codesize()`
    #[mir_op(
        mnemonic = "codesize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(codesize)]
    #[operand_types(func => Some(smallvec![]))]
    CodeSize,
    /// Copy code to memory: `codecopy(destOffset, offset, size)`
    #[mir_op(
        mnemonic = "codecopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(codecopy, void)]
    #[raw_memory(Write(dest, size))]
    #[operand_types(func => Some(smallvec![typing::address(func, *dest, PtrLocation::Memory), MirType::I256, MirType::I256]))]
    CodeCopy(dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code size: `extcodesize(addr)`
    #[mir_op(
        mnemonic = "extcodesize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(extcodesize)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    ExtCodeSize(addr: ValueId),
    /// Copy external code to memory: `extcodecopy(addr, destOffset, offset, size)`
    #[mir_op(
        mnemonic = "extcodecopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Write(dest, size))]
    #[operand_types(func => Some(smallvec![MirType::I256, typing::address(func, *dest, PtrLocation::Memory), MirType::I256, MirType::I256]))]
    ExtCodeCopy(addr: ValueId, dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code hash: `extcodehash(addr)`
    #[mir_op(
        mnemonic = "extcodehash",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(extcodehash)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    ExtCodeHash(addr: ValueId),
    /// Assign an immutable during construction: `storeimmutable <name>, value`.
    /// Lowered to constructor staging memory after MIR optimization.
    #[mir_op(
        mnemonic = "storeimmutable",
        result = None,
        phases = PhaseSet::SEMANTIC,
        effect = ImmutableWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some(PhaseCategory::ImmutableAssignment)
    )]
    #[operand_types(func => None)]
    StoreImmutable(id: ImmutableId, value: ValueId),
    /// Read an immutable declared by the module: `loadimmutable <name>`.
    ///
    /// In runtime code this assembles to a typed `PUSH<N>` placeholder that the
    /// constructor patches with the staged value before returning the runtime
    /// code. In constructor code it reads the staging word instead.
    #[mir_op(
        mnemonic = "loadimmutable",
        result = Custom,
        phases = PhaseSet::ALL,
        effect = ImmutableRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => None)]
    LoadImmutable(id: ImmutableId),

    /// A library address whose value is supplied by the linker.
    #[mir_op(
        mnemonic = "library_address",
        result = I160,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![]))]
    LibraryAddress(library: LibraryId),

    // Return data operations
    /// Get the current call's return data size: `returndatasize()`.
    ///
    /// Raw volatile query used by Yul and high-level call lowering.
    #[mir_op(
        mnemonic = "returndatasize",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(returndatasize)]
    #[operand_types(func => Some(smallvec![]))]
    ReturnDataSize,

    // Environment operations
    /// Get caller address: `caller()`
    #[mir_op(
        mnemonic = "caller",
        result = I160,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(caller)]
    #[operand_types(func => Some(smallvec![]))]
    Caller,
    /// Get call value: `callvalue()`
    #[mir_op(
        mnemonic = "callvalue",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(callvalue)]
    #[operand_types(func => Some(smallvec![]))]
    CallValue,
    /// Get origin address: `origin()`
    #[mir_op(
        mnemonic = "origin",
        result = I160,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(origin)]
    #[operand_types(func => Some(smallvec![]))]
    Origin,
    /// Get gas price: `gasprice()`
    #[mir_op(
        mnemonic = "gasprice",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(gasprice)]
    #[operand_types(func => Some(smallvec![]))]
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    #[mir_op(
        mnemonic = "blockhash",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(blockhash)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    BlockHash(number: ValueId),
    /// Get coinbase address: `coinbase()`
    #[mir_op(
        mnemonic = "coinbase",
        result = I160,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(coinbase)]
    #[operand_types(func => Some(smallvec![]))]
    Coinbase,
    /// Get block timestamp: `timestamp()`
    #[mir_op(
        mnemonic = "timestamp",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(timestamp)]
    #[operand_types(func => Some(smallvec![]))]
    Timestamp,
    /// Get block number: `number()`
    #[mir_op(
        mnemonic = "number",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(number)]
    #[operand_types(func => Some(smallvec![]))]
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    #[mir_op(
        mnemonic = "prevrandao",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(prevrandao)]
    #[operand_types(func => Some(smallvec![]))]
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    #[mir_op(
        mnemonic = "gaslimit",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(gaslimit)]
    #[operand_types(func => Some(smallvec![]))]
    GasLimit,
    /// Get beacon chain slot number: `slotnum()`
    #[mir_op(
        mnemonic = "slotnum",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => Some(smallvec![]))]
    SlotNum,
    /// Get chain ID: `chainid()`
    #[mir_op(
        mnemonic = "chainid",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(chainid)]
    #[operand_types(func => Some(smallvec![]))]
    ChainId,
    /// Get this contract's address: `address()`
    #[mir_op(
        mnemonic = "address",
        result = I160,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(address)]
    #[operand_types(func => Some(smallvec![]))]
    Address,
    /// Get balance: `balance(addr)`
    #[mir_op(
        mnemonic = "balance",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(balance)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    Balance(addr: ValueId),
    /// Get self balance: `selfbalance()`
    #[mir_op(
        mnemonic = "selfbalance",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(selfbalance)]
    #[operand_types(func => Some(smallvec![]))]
    SelfBalance,
    /// Get remaining gas: `gas()`
    #[mir_op(
        mnemonic = "gas",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(gas)]
    #[operand_types(func => Some(smallvec![]))]
    Gas,
    /// Get base fee: `basefee()`
    #[mir_op(
        mnemonic = "basefee",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(basefee)]
    #[operand_types(func => Some(smallvec![]))]
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    #[mir_op(
        mnemonic = "blobbasefee",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(blobbasefee)]
    #[operand_types(func => Some(smallvec![]))]
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    #[mir_op(
        mnemonic = "blobhash",
        result = I256,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(blobhash)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    BlobHash(index: ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    #[mir_op(
        mnemonic = "keccak256",
        result = I256,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(keccak256)]
    #[raw_memory(Read(offset, size))]
    #[operand_types(func => Some(smallvec![typing::address(func, *offset, PtrLocation::Memory), MirType::I256]))]
    Keccak256(offset: ValueId, size: ValueId),
    /// Hash a fixed-width mapping key and its parent slot.
    ///
    /// Late lowering writes scratch memory; effect analysis tracks that footprint.
    #[mir_op(
        mnemonic = "mapping_slot",
        result = I256,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::StorageSlot)
    )]
    #[builder(mapping_slot)]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256]))]
    MappingSlot(key: ValueId, slot: ValueId),
    /// Hash a `bytes` or `string` mapping key, a memory object or a calldata slice, and its
    /// parent mapping slot.
    ///
    /// Late lowering copies the key and the slot to free memory; effect analysis tracks that
    /// footprint.
    #[mir_op(
        mnemonic = "mapping_slot_bytes",
        result = I256,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::StorageSlot)
    )]
    #[builder(mapping_slot_bytes)]
    #[operand_types(func => Some(smallvec![typing::bytes_key(func, *key), MirType::I256]))]
    MappingSlotBytes(key: ValueId, slot: ValueId),
    /// Hash the slot of a dynamically-sized storage array to find its data.
    ///
    /// Late lowering writes scratch memory; effect analysis tracks that footprint.
    #[mir_op(
        mnemonic = "storage_array_data_slot",
        result = I256,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::StorageSlot)
    )]
    #[builder(storage_array_data_slot)]
    #[operand_types(func => Some(smallvec![MirType::I256]))]
    StorageArrayDataSlot(slot: ValueId),
    /// Resolve one element slot in a dynamic storage array.
    ///
    /// The array's base slot, element index, and logical slot stride stay
    /// semantic until the mapping-slot lowering pass expands the hash and
    /// offset calculation.
    #[mir_op(
        mnemonic = "storage_array_element_slot",
        result = I256,
        phases = PhaseSet::SEMANTIC,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some(PhaseCategory::StorageSlot)
    )]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256]))]
    StorageArrayElementSlot { slot: ValueId, index: ValueId, element_slots: u64 },

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    #[mir_op(
        mnemonic = "call",
        result = I1,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(args_offset, args_size), PartialWrite(ret_offset, ret_size))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, MirType::I256, typing::address(func, *args_offset, PtrLocation::Memory), MirType::I256, typing::address(func, *ret_offset, PtrLocation::Memory), MirType::I256]))]
    Call {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Call code: `callcode(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    #[mir_op(
        mnemonic = "callcode",
        result = I1,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(args_offset, args_size), PartialWrite(ret_offset, ret_size))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, MirType::I256, typing::address(func, *args_offset, PtrLocation::Memory), MirType::I256, typing::address(func, *ret_offset, PtrLocation::Memory), MirType::I256]))]
    CallCode {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Static call: `staticcall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    #[mir_op(
        mnemonic = "staticcall",
        result = I1,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(args_offset, args_size), PartialWrite(ret_offset, ret_size))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, typing::address(func, *args_offset, PtrLocation::Memory), MirType::I256, typing::address(func, *ret_offset, PtrLocation::Memory), MirType::I256]))]
    StaticCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Delegate call: `delegatecall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    #[mir_op(
        mnemonic = "delegatecall",
        result = I1,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(args_offset, args_size), PartialWrite(ret_offset, ret_size))]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256, typing::address(func, *args_offset, PtrLocation::Memory), MirType::I256, typing::address(func, *ret_offset, PtrLocation::Memory), MirType::I256]))]
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Internal function call lowered to a direct jump.
    #[mir_op(
        mnemonic = "icall",
        result = Custom,
        phases = PhaseSet::ALL,
        effect = ICall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[when(InstKind::ICall {
        function: Callee::Builtin(Builtin::Check { .. } | Builtin::Require(_)),
        ..
    } => {
        phases: PhaseSet::SEMANTIC,
        phase_category: Some(PhaseCategory::ConditionalCheck),
    })]
    #[when(InstKind::ICall { function: Callee::Builtin(_), .. } => {
        phases: PhaseSet::SEMANTIC,
        phase_category: Some(PhaseCategory::Builtin),
    })]
    #[operand_types(func => typing::callee_types(func, function, args))]
    ICall { function: Callee, args: Box<[ValueId]> },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    #[mir_op(
        mnemonic = "create",
        result = I160,
        phases = PhaseSet::ALL,
        effect = Create,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(create)]
    #[raw_memory(Read(offset, size))]
    #[operand_types(func => Some(smallvec![MirType::I256, typing::address(func, *offset, PtrLocation::Memory), MirType::I256]))]
    Create(value: ValueId, offset: ValueId, size: ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    #[mir_op(
        mnemonic = "create2",
        result = I160,
        phases = PhaseSet::ALL,
        effect = Create,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(offset, size))]
    #[operand_types(func => Some(smallvec![MirType::I256, typing::address(func, *offset, PtrLocation::Memory), MirType::I256, MirType::I256]))]
    Create2(value: ValueId, offset: ValueId, size: ValueId, salt: ValueId),

    // Log operations
    /// Log with up to four topics: `log(offset, size, topics...)`
    #[mir_op(
        mnemonic = "log",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = true,
        category = None
    )]
    #[raw_memory(Read(offset, size))]
    #[operand_types(func => {
        let mut types = smallvec![MirType::I256; 2 + topics.len()];
        types[0] = typing::address(func, *offset, PtrLocation::Memory);
        Some(types)
    })]
    Log {
        /// Start of the logged data.
        offset: ValueId,
        /// Byte length of the logged data.
        size: ValueId,
        /// Indexed topics, at most four.
        topics: LogTopics,
    },

    // SSA operations
    /// Phi node: merge values from different predecessors.
    #[mir_op(
        mnemonic = "phi",
        result = Custom,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => None)]
    Phi(incoming: Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    #[mir_op(
        mnemonic = "select",
        result = Custom,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[operand_types(func => None)]
    Select(cond: ValueId, true_val: ValueId, false_val: ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    #[mir_op(
        mnemonic = "signextend",
        result = I256,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(signextend)]
    #[operand_types(func => Some(smallvec![MirType::I256, MirType::I256]))]
    SignExtend(byte: ValueId, value: ValueId),
}
}
