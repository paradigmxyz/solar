//! The runtime side of the MIR operation schema.
//!
//! Every MIR operation and every property the compiler reads from it is declared in the
//! operation definition language, in `crates/mir/ops/*.ops`. The build script compiles those
//! files with `solar-op-dsl` into the typed [`InstKind`] enum, its [`OpDef`] table, operand
//! traversal and type signatures, raw memory accesses, declared [`Semantics`], the copyable
//! [`Op`] view that ISLE rewrite rules match on, and `FunctionBuilder` methods; this module
//! includes the result. See `docs/OP_DSL.md` for the language.
//!
//! What stays here is the vocabulary the generated code is written against: phase sets,
//! traits, result kinds, raw memory accesses, and the [`Operands`], [`AttributeType`], and
//! [`OperandTypes`] traits through which field types take part in traversal, the rewrite view,
//! and operand signatures.

use alloy_primitives::Bytes;
use solar_ir::link::LibraryId;

use super::{
    AbiEncodeMode, AbiLayoutRef, AbiParamLayoutRef, AddressCallKind, AllocationFailure,
    AllocationSemantics, ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, DataRef, DataSize,
    EffectKind, FrameMode, FrameSlotKind, Function, ImmutableId, InstructionMetadata, MirPhase,
    MirType, PackedPart, PtrLocation, Semantics, ValueId, ValueLayout, typing,
};
use smallvec::{Array, SmallVec};
use std::fmt::Write as _;

/// A compact set of MIR phases in which an operation is structurally valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhaseSet(u8);

const _: () = assert!(
    (MirPhase::Lowered as u32 + 1) < u8::BITS,
    "PhaseSet storage must be widened before adding another MIR phase"
);

impl PhaseSet {
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

/// An attribute that denotes a MIR type, for `type(attribute)` in operand signatures.
pub(crate) trait AttributeType {
    /// The type the attribute denotes.
    fn mir_type(&self) -> MirType;
}

impl AttributeType for ArithmeticKind {
    fn mir_type(&self) -> MirType {
        self.ty().mir_type()
    }
}

impl AttributeType for FrameSlotKind {
    fn mir_type(&self) -> MirType {
        self.result_type()
    }
}

/// A field whose Rust type computes the signature of some operands, for `from(field)` in
/// operand signatures: an ABI layout, a callee, or packed parts.
pub(crate) trait OperandTypes {
    /// Appends the types of `operands`, or returns `None` when the dedicated type checker must
    /// decide.
    fn operand_types(
        &self,
        func: &Function,
        operands: &[ValueId],
        out: &mut SmallVec<[MirType; 8]>,
    ) -> Option<()>;
}

impl OperandTypes for AbiLayoutRef {
    fn operand_types(
        &self,
        func: &Function,
        operands: &[ValueId],
        out: &mut SmallVec<[MirType; 8]>,
    ) -> Option<()> {
        out.extend(typing::abi_encode_types(func, false, operands, self));
        Some(())
    }
}

impl OperandTypes for Callee {
    fn operand_types(
        &self,
        func: &Function,
        operands: &[ValueId],
        out: &mut SmallVec<[MirType; 8]>,
    ) -> Option<()> {
        out.extend(typing::callee_types(func, self, operands)?);
        Some(())
    }
}

impl OperandTypes for Box<[PackedPart]> {
    fn operand_types(
        &self,
        func: &Function,
        _operands: &[ValueId],
        out: &mut SmallVec<[MirType; 8]>,
    ) -> Option<()> {
        out.extend(typing::packed_types(func, self));
        Some(())
    }
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
        for (_, _, fields) in Self::FIELDS {
            for (_, ty) in *fields {
                if !declared.contains(ty) {
                    declared.push(ty);
                    writeln!(out, "(type {ty} (primitive {ty}))").unwrap();
                }
            }
        }

        out.push_str("\n(type Op extern (enum\n");
        for (variant, _, fields) in Self::FIELDS {
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
        for (variant, name, fields) in Self::FIELDS {
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

    /// Returns the value operands in visit order.
    #[must_use]
    pub fn operands(self) -> SmallVec<[ValueId; 8]> {
        let mut operands = SmallVec::new();
        // Only the visit matters; the mapped copy is discarded.
        let _ = self.map_values(|value| {
            operands.push(value);
            value
        });
        operands
    }
}

include!(concat!(env!("OUT_DIR"), "/mir_ops.rs"));
