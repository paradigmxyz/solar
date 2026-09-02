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
//! ISLE rewrite rules match on, and the ISLE prelude declaring it.

use super::{
    AbiEncodeMode, AbiLayoutRef, AbiParamLayoutRef, AllocationKind, AllocationSemantics, BlockId,
    DataRef, EffectKind, FrameMode, FrameSlotKind, FunctionId, ImmutableId, InstructionMetadata,
    MemoryObjectKind, MemoryObjectLayout, MirPhase, MirType, SliceLocation, StorageLayoutRef,
    ValueId,
};
use smallvec::{Array, SmallVec};
#[cfg(test)]
use std::fmt::Write as _;

/// A compact set of MIR phases in which an operation is structurally valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PhaseSet(u8);

impl PhaseSet {
    /// All phases currently defined by MIR.
    pub(crate) const ALL: Self = Self::through(MirPhase::EvmShaped);
    /// Phases before semantic memory lowering has completed.
    pub(crate) const THROUGH_DISPATCH: Self = Self::through(MirPhase::Dispatch);
    /// Phases before the physical EVM shape boundary.
    pub(crate) const THROUGH_MEMORY_LOWERED: Self = Self::through(MirPhase::MemoryLowered);

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
pub(crate) struct OpTraits(u16);

impl OpTraits {
    /// No additional operation traits.
    pub(crate) const NONE: Self = Self(0);
    /// The operation's binary operands may be exchanged by the scheduler.
    pub(crate) const REORDERABLE: Self = Self(1 << 0);
    /// The operation is cheap and stable enough to rematerialize at uses.
    pub(crate) const REMATERIALIZABLE: Self = Self(1 << 1);
    /// The operation still carries a semantic memory-object representation.
    pub(crate) const MEMORY_OBJECT: Self = Self(1 << 2);

    /// Returns whether this set contains `trait_`.
    pub(crate) const fn contains(self, trait_: Self) -> bool {
        self.0 & trait_.0 == trait_.0
    }
}

/// The value an operation produces, when the operation alone determines it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ResultKind {
    /// The operation produces no value.
    None,
    /// An unsigned 256-bit word.
    Word,
    /// A signed 256-bit word.
    SignedWord,
    /// A boolean.
    Bool,
    /// An address.
    Address,
    /// A 32-byte hash or slot.
    Bytes32,
    /// A memory pointer.
    MemPtr,
    /// A value whose type depends on the operation's attributes.
    Custom,
}

impl ResultKind {
    /// Returns the result type used when the textual form carries none.
    #[must_use]
    pub(crate) const fn default_type(self) -> Option<MirType> {
        match self {
            Self::None | Self::Custom => None,
            Self::Word => Some(MirType::uint256()),
            Self::SignedWord => Some(MirType::int256()),
            Self::Bool => Some(MirType::Bool),
            Self::Address => Some(MirType::Address),
            Self::Bytes32 => Some(MirType::bytes32()),
            Self::MemPtr => Some(MirType::MemPtr),
        }
    }

    /// Returns whether operations of this kind always produce a value.
    #[must_use]
    pub(crate) const fn produces_value(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Instruction field types, classified as value operands or attributes.
pub(crate) trait Operands {
    /// Copyable projection of the field seen by rewrite rules.
    type View: Copy;
    /// ISLE type name of the projection.
    #[cfg(test)]
    const ISLE_TYPE: &'static str;

    /// Appends every value operand held by this field in canonical order.
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>);
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
}

impl Operands for ValueId {
    type View = Self;
    #[cfg(test)]
    const ISLE_TYPE: &'static str = "Value";

    #[inline]
    fn collect<A: Array<Item = Self>>(&self, out: &mut SmallVec<A>) {
        out.push(*self);
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
}

impl Operands for Option<ValueId> {
    type View = Self;
    #[cfg(test)]
    const ISLE_TYPE: &'static str = "OptionValue";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(*self);
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
}

impl Operands for Box<[ValueId]> {
    type View = ();
    #[cfg(test)]
    const ISLE_TYPE: &'static str = "Unit";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(self.iter().copied());
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
}

impl Operands for Vec<(BlockId, ValueId)> {
    type View = ();
    #[cfg(test)]
    const ISLE_TYPE: &'static str = "Unit";

    #[inline]
    fn collect<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>) {
        out.extend(self.iter().map(|(_, value)| *value));
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
}

/// Declares field types that never hold value operands.
macro_rules! attributes {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Operands for $ty {
                type View = Self;
                #[cfg(test)]
                const ISLE_TYPE: &'static str = stringify!($ty);

                #[inline]
                fn collect<A: Array<Item = ValueId>>(&self, _out: &mut SmallVec<A>) {}

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
                #[cfg(test)]
                const ISLE_TYPE: &'static str = "Unit";

                #[inline]
                fn collect<A: Array<Item = ValueId>>(&self, _out: &mut SmallVec<A>) {}

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
            }
        )+
    };
}

#[cfg(test)]
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

attributes! {
    u32,
    u64,
    AbiEncodeMode,
    AllocationKind,
    AllocationSemantics,
    DataRef,
    FrameMode,
    FrameSlotKind,
    FunctionId,
    ImmutableId,
    MemoryObjectKind,
    MemoryObjectLayout,
    SliceLocation,
}

opaque_attributes! {
    AbiLayoutRef,
    AbiParamLayoutRef,
    StorageLayoutRef,
}

macro_rules! define_mir_ops {
    (
        enum $inst_name:ident {
            $(
                $(#[$meta:meta])*
                $variant:ident
                $( ( $( $operand:ident : $operand_ty:ty ),+ $(,)? ) )?
                $( { $( $(#[$field_meta:meta])* $field:ident : $field_ty:ty ),+ $(,)? } )?
            ),+ $(,)?
        }
        defs {
            $(
                $pattern:pat => {
                    mnemonic: $mnemonic:literal,
                    result: $result:ident,
                    phases: $phases:expr,
                    effect: $effect:ident,
                    traits: $traits:expr,
                    side_effects: $side_effects:expr,
                    category: $category:expr $(,)?
                }
            ),+ $(,)?
        }
    ) => {
        /// The kind of a MIR instruction.
        #[derive(Clone, Debug, PartialEq)]
        pub(crate) enum $inst_name {
            $(
                $(#[$meta])*
                $variant
                $( ( $( $operand_ty ),+ ) )?
                $( { $( $(#[$field_meta])* $field: $field_ty ),+ } )?,
            )+
        }

        /// Generated metadata for one MIR operation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) struct OpDef {
            /// Canonical textual operation name.
            pub(crate) mnemonic: &'static str,
            /// Value produced by the operation.
            pub(crate) result: ResultKind,
            /// Phases in which this operation is valid.
            pub(crate) phases: PhaseSet,
            /// Conservative effect classification.
            pub(crate) effect: EffectKind,
            /// Declarative operation properties.
            pub(crate) traits: OpTraits,
            /// Whether the operation must remain observable to DCE.
            pub(crate) has_side_effects: bool,
            /// Diagnostic category used when a phase boundary is violated.
            pub(crate) phase_category: Option<&'static str>,
        }

        impl $inst_name {
            /// Returns the declarative definition for this operation.
            #[inline]
            #[must_use]
            pub(crate) const fn op_def(&self) -> &'static OpDef {
                match self {
                    $(
                        $pattern => &OpDef {
                            mnemonic: $mnemonic,
                            result: ResultKind::$result,
                            phases: $phases,
                            effect: EffectKind::$effect,
                            traits: $traits,
                            has_side_effects: $side_effects,
                            phase_category: $category,
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

            /// Visits every value operand mutably, in canonical order.
            pub(crate) fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => {
                            $( $( Operands::visit_mut($operand, &mut f); )+ )?
                            $( $( Operands::visit_mut($field, &mut f); )+ )?
                        }
                    )+
                }
            }

            /// Returns the rewrite-rule view of this instruction.
            #[must_use]
            pub(crate) fn op(&self) -> Op {
                match self {
                    $(
                        Self::$variant $( ( $( $operand ),+ ) )? $( { $( $field ),+ } )? => Op::$variant
                            $( { $( $operand: Operands::view($operand) ),+ } )?
                            $( { $( $field: Operands::view($field) ),+ } )?,
                    )+
                }
            }

            /// Returns the operation's phase-boundary diagnostic category.
            #[inline]
            #[must_use]
            pub(crate) fn phase_violation(
                &self,
                phase: MirPhase,
                metadata: &InstructionMetadata,
            ) -> Option<&'static str> {
                let definition = self.op_def();
                if !definition.phases.contains(phase) {
                    return definition.phase_category;
                }
                if matches!(self, Self::Alloc { kind: AllocationKind::Object(_), .. })
                    && phase >= MirPhase::MemoryLowered
                {
                    return Some("memory-object");
                }
                if matches!(self, Self::Alloc { .. })
                    && phase >= MirPhase::EvmShaped
                    && !metadata.deferred_alloc()
                {
                    return Some("abstract allocation");
                }
                None
            }
        }

        /// Copyable view of an instruction for rewrite rules.
        ///
        /// Value operands keep their identity, attributes are carried by value,
        /// and variable-length payloads are elided.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Op {
            $(
                $variant
                $( { $( $operand: <$operand_ty as Operands>::View ),+ } )?
                $( { $( $field: <$field_ty as Operands>::View ),+ } )?,
            )+
        }

        impl Op {
            #[cfg(test)]
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
            pub(crate) fn into_kind(self) -> Option<$inst_name> {
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
            pub(crate) fn map_values(self, mut f: impl FnMut(ValueId) -> ValueId) -> Self {
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
    #[cfg(test)]
    pub(crate) fn isle_prelude() -> String {
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
        out.push_str("))\n\n;; The instruction defining a value.\n(decl inst (Op) Value)\n(extern extractor inst inst_data)\n");

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
    // Arithmetic operations
    /// Addition: `a + b`
    Add(a: ValueId, b: ValueId),
    /// Subtraction: `a - b`
    Sub(a: ValueId, b: ValueId),
    /// Multiplication: `a * b`
    Mul(a: ValueId, b: ValueId),
    /// Unsigned division: `a / b`
    Div(a: ValueId, b: ValueId),
    /// Signed division: `a / b`
    SDiv(a: ValueId, b: ValueId),
    /// Unsigned modulo: `a % b`
    Mod(a: ValueId, b: ValueId),
    /// Signed modulo: `a % b`
    SMod(a: ValueId, b: ValueId),
    /// Exponentiation: `a ** b`
    Exp(a: ValueId, b: ValueId),
    /// Add modulo: `(a + b) % n`
    AddMod(a: ValueId, b: ValueId, n: ValueId),
    /// Multiply modulo: `(a * b) % n`
    MulMod(a: ValueId, b: ValueId, n: ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    And(a: ValueId, b: ValueId),
    /// Bitwise OR: `a | b`
    Or(a: ValueId, b: ValueId),
    /// Bitwise XOR: `a ^ b`
    Xor(a: ValueId, b: ValueId),
    /// Bitwise NOT: `~a`
    Not(a: ValueId),
    /// Count leading zero bits.
    Clz(a: ValueId),
    /// Left shift: `a << b`
    Shl(shift: ValueId, value: ValueId),
    /// Logical right shift: `a >> b`
    Shr(shift: ValueId, value: ValueId),
    /// Arithmetic right shift: `a >> b` (signed)
    Sar(shift: ValueId, value: ValueId),
    /// Extract a byte: `byte(i, x)`
    Byte(index: ValueId, value: ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    Lt(a: ValueId, b: ValueId),
    /// Greater than (unsigned): `a > b`
    Gt(a: ValueId, b: ValueId),
    /// Less than (signed): `a < b`
    SLt(a: ValueId, b: ValueId),
    /// Greater than (signed): `a > b`
    SGt(a: ValueId, b: ValueId),
    /// Equality: `a == b`
    Eq(a: ValueId, b: ValueId),
    /// Check if zero: `a == 0`
    IsZero(a: ValueId),

    // Memory operations
    /// Load from memory: `mload(offset)`
    MLoad(offset: ValueId),
    /// Store to memory: `mstore(offset, value)`
    MStore(offset: ValueId, value: ValueId),
    /// Store a single byte: `mstore8(offset, value)`
    MStore8(offset: ValueId, value: ValueId),
    /// Set a contiguous memory range to zero: `memory_zero(offset, size)`
    MemoryZero(offset: ValueId, size: ValueId),
    /// Get memory size: `msize()`
    MSize,
    /// Read the free-memory pointer.
    Fmp,
    /// Set the free-memory pointer.
    SetFmp(value: ValueId),
    /// Reserve memory and return the previous free-memory pointer.
    Alloc {
        /// Requested byte count.
        size: ValueId,
        /// Semantic shape of the returned reference.
        kind: AllocationKind,
        /// Alignment, initialization, and failure behavior.
        semantics: AllocationSemantics,
    },
    /// Read the logical length of a dynamic memory object.
    MemoryObjectLen(object: ValueId, kind: MemoryObjectKind),
    /// Set the logical length of a dynamic memory object.
    SetMemoryObjectLen(object: ValueId, len: ValueId, kind: MemoryObjectKind),
    /// Project the address of the first payload byte from an object.
    MemoryObjectData(object: ValueId, kind: MemoryObjectKind),
    /// Address a direct field of a struct object.
    MemoryObjectFieldAddr {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Address an array element under the semantic object layout.
    MemoryObjectElementAddr {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one direct struct field without exposing its physical address.
    MemoryObjectLoadField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Store one direct struct field without exposing its physical address.
    MemoryObjectStoreField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
        /// Value to store.
        value: ValueId,
    },
    /// Load one array element without exposing its physical address.
    MemoryObjectLoadElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one byte from a bytes object without exposing its physical address.
    MemoryObjectLoadByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
    },
    /// Store one array element without exposing its physical address.
    MemoryObjectStoreElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
        /// Value to store.
        value: ValueId,
    },
    /// Store one byte in a bytes object without exposing its physical address.
    MemoryObjectStoreByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
        /// Low byte to store.
        value: ValueId,
    },
    /// Store one word at a byte offset in a bytes object without exposing its
    /// physical address.
    MemoryObjectStoreWord {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte offset from the payload start.
        offset: ValueId,
        /// Word to store.
        value: ValueId,
    },
    /// Load one word from a memory slice at a byte offset without exposing its
    /// physical address.
    MemorySliceLoadWord {
        /// Memory slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Load one word from a calldata slice at a byte offset without exposing
    /// the physical calldata address.
    CalldataSliceLoadWord {
        /// Calldata slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Copy a typed slice into the payload of a dynamic memory object.
    MemoryObjectCopyFromSlice {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a typed slice into a byte offset in a dynamic memory object.
    MemoryObjectCopyFromSliceAt {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Byte offset from the destination payload start.
        offset: ValueId,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a byte range between two dynamic memory objects.
    MemoryObjectCopy {
        /// Destination memory object reference.
        destination: ValueId,
        /// Destination memory object kind.
        destination_kind: MemoryObjectKind,
        /// Source memory object reference.
        source: ValueId,
        /// Source memory object kind.
        source_kind: MemoryObjectKind,
        /// Number of bytes to copy.
        length: ValueId,
    },
    /// ABI-encode values into memory.
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
    AbiDecode {
        /// ABI-encoded bytes object.
        data: ValueId,
        /// Interned ABI input layout, including scalar validation types.
        layout: AbiParamLayoutRef,
    },
    /// Copy a statically shaped aggregate from storage into an existing memory allocation.
    StorageToMemory {
        /// Base storage slot.
        storage: ValueId,
        /// Destination memory pointer.
        memory: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy a statically shaped aggregate from memory into storage.
    MemoryToStorage {
        /// Base storage slot.
        storage: ValueId,
        /// Source memory pointer.
        memory: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Clear every storage slot occupied by a statically shaped aggregate.
    ClearStorage {
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy memory: `mcopy(dest, src, len)`
    MCopy(dest: ValueId, src: ValueId, len: ValueId),

    // Storage operations
    /// Load from storage: `sload(slot)`
    SLoad(slot: ValueId),
    /// Store to storage: `sstore(slot, value)`
    SStore(slot: ValueId, value: ValueId),
    /// Transient load: `tload(slot)`
    TLoad(slot: ValueId),
    /// Transient store: `tstore(slot, value)`
    TStore(slot: ValueId, value: ValueId),

    // Calldata operations
    /// Load from calldata: `calldataload(offset)`
    CalldataLoad(offset: ValueId),
    /// Copy calldata to memory: `calldatacopy(destOffset, offset, size)`
    CalldataCopy(dest: ValueId, offset: ValueId, size: ValueId),
    /// Get calldata size: `calldatasize()`
    CalldataSize,
    /// Construct a logical `(pointer, length, location)` slice.
    MakeSlice {
        /// Address of the first element or byte.
        ptr: ValueId,
        /// Logical element or byte length.
        len: ValueId,
        /// Address space containing the slice data.
        location: SliceLocation,
    },
    /// Project the data pointer from a slice.
    SlicePtr(slice: ValueId),
    /// Project the logical length from a slice.
    SliceLen(slice: ValueId),
    /// Address inside the current internal-call frame.
    InternalFrameAddr(offset: u64),
    /// Load a mutable local through its logical frame slot.
    ///
    /// A plain memory read: deletable when its result is dead. Ordering
    /// against frame stores, calls, and other frame traffic is carried by
    /// effect kinds and the alias model's `frame_location`.
    FrameLoad {
        /// Byte offset within the function's local region.
        offset: u64,
        /// Calling convention that owns the local region.
        mode: FrameMode,
        /// Logical value representation stored in the slot.
        kind: FrameSlotKind,
    },
    /// Store a mutable local through its logical frame slot.
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
    ConstructorArgsBase,
    /// End address of the constructor's copied ABI argument blob.
    ConstructorArgsEnd,

    // Code operations
    /// Copy constant module data to memory.
    DataCopy(data: DataRef, dest: ValueId, size: ValueId),
    /// Get code size: `codesize()`
    CodeSize,
    /// Copy code to memory: `codecopy(destOffset, offset, size)`
    CodeCopy(dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code size: `extcodesize(addr)`
    ExtCodeSize(addr: ValueId),
    /// Copy external code to memory: `extcodecopy(addr, destOffset, offset, size)`
    ExtCodeCopy(addr: ValueId, dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code hash: `extcodehash(addr)`
    ExtCodeHash(addr: ValueId),
    /// Assign an immutable during construction: `storeimmutable <name>, value`.
    /// Lowered to constructor staging memory after MIR optimization.
    StoreImmutable(id: ImmutableId, value: ValueId),
    /// Read an immutable declared by the module: `loadimmutable <name>`.
    ///
    /// In runtime code this assembles to a typed `PUSH<N>` placeholder that the
    /// constructor patches with the staged value before returning the runtime
    /// code. In constructor code it reads the staging word instead.
    LoadImmutable(id: ImmutableId),

    // Return data operations
    /// Get the current call's return data size: `returndatasize()`.
    ///
    /// Raw volatile query used by Yul and high-level call lowering.
    ReturnDataSize,
    /// Copy return data to memory: `returndatacopy(destOffset, offset, size)`
    ReturnDataCopy(dest: ValueId, offset: ValueId, size: ValueId),

    // Environment operations
    /// Get caller address: `caller()`
    Caller,
    /// Get call value: `callvalue()`
    CallValue,
    /// Get origin address: `origin()`
    Origin,
    /// Get gas price: `gasprice()`
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    BlockHash(number: ValueId),
    /// Get coinbase address: `coinbase()`
    Coinbase,
    /// Get block timestamp: `timestamp()`
    Timestamp,
    /// Get block number: `number()`
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    GasLimit,
    /// Get beacon chain slot number: `slotnum()`
    SlotNum,
    /// Get chain ID: `chainid()`
    ChainId,
    /// Get this contract's address: `address()`
    Address,
    /// Get balance: `balance(addr)`
    Balance(addr: ValueId),
    /// Get self balance: `selfbalance()`
    SelfBalance,
    /// Get remaining gas: `gas()`
    Gas,
    /// Get base fee: `basefee()`
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    BlobHash(index: ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    Keccak256(offset: ValueId, size: ValueId),
    /// Keccak256 hash of a `memorybytes` object's contents:
    /// `keccak256_bytes(object)`.
    ///
    /// Consumes the object reference directly, so the optimizer sees one
    /// whole-object read instead of separate length and data-pointer
    /// projections. `lower-memory-objects` expands it into those projections
    /// and a physical `keccak256`.
    Keccak256Bytes(object: ValueId),
    /// Hash a fixed-width mapping key and its parent slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlot(key: ValueId, slot: ValueId),
    /// Hash a `[length][data...]` memory value and its parent mapping slot.
    MappingSlotMemory(key: ValueId, slot: ValueId),
    /// Hash a dynamically-sized calldata value and its parent mapping slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    MappingSlotCalldata(key: ValueId, slot: ValueId),
    /// Hash the slot of a dynamically-sized storage array to find its data.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    StorageArrayDataSlot(slot: ValueId),
    /// Resolve one element slot in a dynamic storage array.
    ///
    /// The array's base slot, element index, and logical slot stride stay
    /// semantic until the mapping-slot lowering pass expands the hash and
    /// offset calculation.
    StorageArrayElementSlot { slot: ValueId, index: ValueId, element_slots: u64 },

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
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
    StaticCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Delegate call: `delegatecall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// EOF external call: `extcall(addr, argsOffset, argsSize, value)`.
    ExtCall { addr: ValueId, args_offset: ValueId, args_size: ValueId, value: ValueId },
    /// EOF external delegate call: `extdelegatecall(addr, argsOffset, argsSize)`.
    ExtDelegateCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// EOF external static call: `extstaticcall(addr, argsOffset, argsSize)`.
    ExtStaticCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// Internal function call lowered to a direct jump.
    InternalCall { function: FunctionId, args: Box<[ValueId]>, returns: u32 },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    Create(value: ValueId, offset: ValueId, size: ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    Create2(value: ValueId, offset: ValueId, size: ValueId, salt: ValueId),

    // Log operations
    // TODO(codegen): Consider unifying log0..log4 as one instruction with a topic list.
    /// Log with no topics: `log0(offset, size)`
    Log0(offset: ValueId, size: ValueId),
    /// Log with 1 topic: `log1(offset, size, topic1)`
    Log1(offset: ValueId, size: ValueId, topic1: ValueId),
    /// Log with 2 topics: `log2(offset, size, topic1, topic2)`
    Log2(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId),
    /// Log with 3 topics: `log3(offset, size, topic1, topic2, topic3)`
    Log3(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId, topic3: ValueId),
    /// Log with 4 topics: `log4(offset, size, topic1, topic2, topic3, topic4)`
    Log4(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId, topic3: ValueId, topic4: ValueId),

    // SSA operations
    /// Phi node: merge values from different predecessors.
    Phi(incoming: Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    Select(cond: ValueId, true_val: ValueId, false_val: ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    SignExtend(byte: ValueId, value: ValueId),
}
    defs {
    Self::Add(_, _) => { mnemonic: "add", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Sub(_, _) => { mnemonic: "sub", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mul(_, _) => { mnemonic: "mul", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Div(_, _) => { mnemonic: "div", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SDiv(_, _) => { mnemonic: "sdiv", result: SignedWord, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Mod(_, _) => { mnemonic: "mod", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SMod(_, _) => { mnemonic: "smod", result: SignedWord, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Exp(_, _) => { mnemonic: "exp", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::AddMod(_, _, _) => { mnemonic: "addmod", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MulMod(_, _, _) => { mnemonic: "mulmod", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::And(_, _) => { mnemonic: "and", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Or(_, _) => { mnemonic: "or", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Xor(_, _) => { mnemonic: "xor", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Not(_) => { mnemonic: "not", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Clz(_) => { mnemonic: "clz", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shl(_, _) => { mnemonic: "shl", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Shr(_, _) => { mnemonic: "shr", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Sar(_, _) => { mnemonic: "sar", result: SignedWord, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Byte(_, _) => { mnemonic: "byte", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Lt(_, _) => { mnemonic: "lt", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Gt(_, _) => { mnemonic: "gt", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SLt(_, _) => { mnemonic: "slt", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::SGt(_, _) => { mnemonic: "sgt", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::Eq(_, _) => { mnemonic: "eq", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::REORDERABLE, side_effects: false, category: None },
    Self::IsZero(_) => { mnemonic: "iszero", result: Bool, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::MLoad(_) => { mnemonic: "mload", result: Word, phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::MStore(_, _) => { mnemonic: "mstore", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MStore8(_, _) => { mnemonic: "mstore8", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::MemoryZero(_, _) => { mnemonic: "memory_zero", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("memory zero") },
    Self::MSize => { mnemonic: "msize", result: Word, phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Fmp => { mnemonic: "fmp", result: MemPtr, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("abstract allocation") },
    Self::SetFmp(_) => { mnemonic: "set_fmp", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::Alloc { .. } => { mnemonic: "alloc", result: Custom, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("abstract allocation") },
    Self::MemoryObjectLen(_, _) => { mnemonic: "memory_object_len", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::SetMemoryObjectLen(_, _, _) => { mnemonic: "set_memory_object_len", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectData(_, _) => { mnemonic: "memory_object_data", result: MemPtr, phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectFieldAddr { .. } => { mnemonic: "memory_object_field_addr", result: MemPtr, phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectElementAddr { .. } => { mnemonic: "memory_object_element_addr", result: MemPtr, phases: PhaseSet::THROUGH_DISPATCH, effect: Pure, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadField { .. } => { mnemonic: "memory_object_load_field", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreField { .. } => { mnemonic: "memory_object_store_field", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectLoadElement { .. } => { mnemonic: "memory_object_load_element", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectLoadByte { .. } => { mnemonic: "memory_object_load_byte", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectStoreElement { .. } => { mnemonic: "memory_object_store_element", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreByte { .. } => { mnemonic: "memory_object_store_byte", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectStoreWord { .. } => { mnemonic: "memory_object_store_word", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemorySliceLoadWord { .. } => { mnemonic: "memory_slice_load_word", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::CalldataSliceLoadWord { .. } => { mnemonic: "calldata_slice_load_word", result: Word, phases: PhaseSet::THROUGH_DISPATCH, effect: EnvironmentRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSlice { .. } => { mnemonic: "memory_object_copy_from_slice", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopyFromSliceAt { .. } => { mnemonic: "memory_object_copy_from_slice_at", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::MemoryObjectCopy { .. } => { mnemonic: "memory_object_copy", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::MEMORY_OBJECT, side_effects: true, category: Some("memory-object") },
    Self::AbiEncode { .. } => { mnemonic: "abi_encode", result: Custom, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI encoding") },
    Self::AbiDecode { .. } => { mnemonic: "abi_decode", result: Custom, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("ABI decoding") },
    Self::StorageToMemory { .. } => { mnemonic: "storage_to_memory", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MemoryToStorage { .. } => { mnemonic: "memory_to_storage", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::ClearStorage { .. } => { mnemonic: "clear_storage", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: Some("aggregate") },
    Self::MCopy(_, _, _) => { mnemonic: "mcopy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::SLoad(_) => { mnemonic: "sload", result: Word, phases: PhaseSet::ALL, effect: StorageRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SStore(_, _) => { mnemonic: "sstore", result: None, phases: PhaseSet::ALL, effect: StorageWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::TLoad(_) => { mnemonic: "tload", result: Word, phases: PhaseSet::ALL, effect: TransientRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::TStore(_, _) => { mnemonic: "tstore", result: None, phases: PhaseSet::ALL, effect: TransientWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::CalldataLoad(_) => { mnemonic: "calldataload", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::CalldataCopy(_, _, _) => { mnemonic: "calldatacopy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CalldataSize => { mnemonic: "calldatasize", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::MakeSlice { location: SliceLocation::Memory, .. } => { mnemonic: "make_memory_slice", result: Custom, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::MakeSlice { location: SliceLocation::Calldata, .. } => { mnemonic: "make_calldata_slice", result: Custom, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::MakeSlice { location: SliceLocation::Returndata, .. } => { mnemonic: "make_returndata_slice", result: Custom, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SlicePtr(_) => { mnemonic: "slice_ptr", result: Word, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::SliceLen(_) => { mnemonic: "slice_len", result: Word, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("slice") },
    Self::InternalFrameAddr(_) => { mnemonic: "internal_frame_addr", result: MemPtr, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::FrameLoad { .. } => { mnemonic: "frame_load", result: Custom, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("frame slot") },
    Self::FrameStore { .. } => { mnemonic: "frame_store", result: None, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: Some("frame slot") },
    Self::ConstructorArgsBase => { mnemonic: "constructor_args_base", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ConstructorArgsEnd => { mnemonic: "constructor_args_end", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::DataCopy(_, _, _) => { mnemonic: "data_copy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::REORDERABLE, side_effects: true, category: None },
    Self::CodeSize => { mnemonic: "codesize", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CodeCopy(_, _, _) => { mnemonic: "codecopy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeSize(_) => { mnemonic: "extcodesize", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ExtCodeCopy(_, _, _, _) => { mnemonic: "extcodecopy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCodeHash(_) => { mnemonic: "extcodehash", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::StoreImmutable(..) => { mnemonic: "storeimmutable", result: None, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: ImmutableWrite, traits: OpTraits::NONE, side_effects: true, category: Some("immutable assignment") },
    Self::LoadImmutable(_) => { mnemonic: "loadimmutable", result: Custom, phases: PhaseSet::ALL, effect: ImmutableRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataSize => { mnemonic: "returndatasize", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::ReturnDataCopy(_, _, _) => { mnemonic: "returndatacopy", result: None, phases: PhaseSet::ALL, effect: MemoryWrite, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Caller => { mnemonic: "caller", result: Address, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::CallValue => { mnemonic: "callvalue", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Origin => { mnemonic: "origin", result: Address, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasPrice => { mnemonic: "gasprice", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockHash(_) => { mnemonic: "blockhash", result: Bytes32, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Coinbase => { mnemonic: "coinbase", result: Address, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Timestamp => { mnemonic: "timestamp", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlockNumber => { mnemonic: "number", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::PrevRandao => { mnemonic: "prevrandao", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::GasLimit => { mnemonic: "gaslimit", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::SlotNum => { mnemonic: "slotnum", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::ChainId => { mnemonic: "chainid", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Address => { mnemonic: "address", result: Address, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::Balance(_) => { mnemonic: "balance", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SelfBalance => { mnemonic: "selfbalance", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Gas => { mnemonic: "gas", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::BaseFee => { mnemonic: "basefee", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobBaseFee => { mnemonic: "blobbasefee", result: Word, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::REMATERIALIZABLE, side_effects: false, category: None },
    Self::BlobHash(_) => { mnemonic: "blobhash", result: Bytes32, phases: PhaseSet::ALL, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: None },

    Self::Keccak256(_, _) => { mnemonic: "keccak256", result: Bytes32, phases: PhaseSet::ALL, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Keccak256Bytes(_) => { mnemonic: "keccak256_bytes", result: Bytes32, phases: PhaseSet::THROUGH_DISPATCH, effect: MemoryRead, traits: OpTraits::MEMORY_OBJECT, side_effects: false, category: Some("memory-object") },
    Self::MappingSlot(_, _) => { mnemonic: "mapping_slot", result: Bytes32, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotMemory(_, _) => { mnemonic: "mapping_slot_memory", result: Bytes32, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: MemoryRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::MappingSlotCalldata(_, _) => { mnemonic: "mapping_slot_calldata", result: Bytes32, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: EnvironmentRead, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayDataSlot(_) => { mnemonic: "storage_array_data_slot", result: Bytes32, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },
    Self::StorageArrayElementSlot { .. } => { mnemonic: "storage_array_element_slot", result: Bytes32, phases: PhaseSet::THROUGH_MEMORY_LOWERED, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: Some("storage slot") },

    Self::Call { .. } => { mnemonic: "call", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::CallCode { .. } => { mnemonic: "callcode", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::StaticCall { .. } => { mnemonic: "staticcall", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::DelegateCall { .. } => { mnemonic: "delegatecall", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtCall { .. } => { mnemonic: "extcall", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtDelegateCall { .. } => { mnemonic: "extdelegatecall", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::ExtStaticCall { .. } => { mnemonic: "extstaticcall", result: Word, phases: PhaseSet::ALL, effect: ExternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::InternalCall { .. } => { mnemonic: "internal_call", result: Custom, phases: PhaseSet::ALL, effect: InternalCall, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create(_, _, _) => { mnemonic: "create", result: Address, phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Create2(_, _, _, _) => { mnemonic: "create2", result: Address, phases: PhaseSet::ALL, effect: Create, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log0(_, _) => { mnemonic: "log0", result: None, phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log1(_, _, _) => { mnemonic: "log1", result: None, phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log2(_, _, _, _) => { mnemonic: "log2", result: None, phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log3(_, _, _, _, _) => { mnemonic: "log3", result: None, phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },
    Self::Log4(_, _, _, _, _, _) => { mnemonic: "log4", result: None, phases: PhaseSet::ALL, effect: Log, traits: OpTraits::NONE, side_effects: true, category: None },

    Self::Phi(_) => { mnemonic: "phi", result: Custom, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::Select(_, _, _) => { mnemonic: "select", result: Word, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    Self::SignExtend(_, _) => { mnemonic: "signextend", result: SignedWord, phases: PhaseSet::ALL, effect: Pure, traits: OpTraits::NONE, side_effects: false, category: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{AllocationKind, AllocationSemantics, ValueId};

    #[test]
    fn descriptors_drive_operation_properties() {
        let add = InstKind::Add(ValueId::new(0), ValueId::new(1));
        assert_eq!(add.mnemonic(), "add");
        assert_eq!(add.effect_kind(), EffectKind::Pure);
        assert!(add.op_def().traits.contains(OpTraits::REORDERABLE));
        assert!(!add.has_side_effects());

        let calldata_size = InstKind::CalldataSize;
        assert!(calldata_size.is_always_rematerializable());
        assert!(calldata_size.op_def().phases.contains(MirPhase::EvmShaped));

        assert_eq!(add.op_def().result.default_type(), Some(MirType::uint256()));
        assert!(
            !InstKind::MStore(ValueId::new(0), ValueId::new(1)).op_def().result.produces_value()
        );

        let slice = InstKind::MakeSlice {
            ptr: ValueId::new(0),
            len: ValueId::new(1),
            location: SliceLocation::Calldata,
        };
        assert_eq!(slice.mnemonic(), "make_calldata_slice");
    }

    #[test]
    fn views_project_operands() {
        let add = InstKind::Add(ValueId::new(0), ValueId::new(1));
        assert_eq!(add.op(), Op::Add { a: ValueId::new(0), b: ValueId::new(1) });
        let mapped = add.op().map_values(|value| ValueId::new(value.index() + 10));
        assert_eq!(mapped, Op::Add { a: ValueId::new(10), b: ValueId::new(11) });
        assert_eq!(InstKind::MSize.op(), Op::MSize);
        assert_eq!(add.op().into_kind().as_ref(), Some(&add));
        assert_eq!(InstKind::Phi(Vec::new()).op().into_kind(), None);
    }

    #[test]
    fn isle_prelude_matches_schema() {
        snapbox::assert_data_eq!(Op::isle_prelude(), snapbox::file!["../../isle/prelude.isle"]);
    }

    #[test]
    fn descriptors_enforce_phase_boundaries() {
        let metadata = InstructionMetadata::EMPTY;
        let fmp = InstKind::Fmp;
        assert_eq!(
            fmp.phase_violation(MirPhase::EvmShaped, &metadata),
            Some("abstract allocation")
        );

        let object_load =
            InstKind::MemoryObjectLoadByte { object: ValueId::new(0), index: ValueId::new(1) };
        assert_eq!(
            object_load.phase_violation(MirPhase::MemoryLowered, &metadata),
            Some("memory-object")
        );

        let raw_alloc = InstKind::Alloc {
            size: ValueId::new(0),
            kind: AllocationKind::Raw,
            semantics: AllocationSemantics::INTERNAL,
        };
        assert_eq!(
            raw_alloc.phase_violation(MirPhase::EvmShaped, &metadata),
            Some("abstract allocation")
        );
    }
}
