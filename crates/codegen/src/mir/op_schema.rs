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
//! `#[mnemonic(pattern => name)]` gives an attribute-dependent textual spelling,
//! such as a slice's address space, without duplicating the rest of its metadata.
//! `#[commutative(lhs, rhs)]` generates the trait and canonicalizes exactly that
//! operand pair, leaving attributes and any remaining operands untouched.

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

const _: () = assert!(
    (MirPhase::EvmShaped as u32 + 1) < u8::BITS,
    "PhaseSet storage must be widened before adding another MIR phase"
);

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
    /// The operation is cheap and stable enough to rematerialize when its
    /// operands can themselves be rebuilt from stable leaves.
    pub(crate) const REMATERIALIZABLE: Self = Self(1 << 1);
    /// The operation still carries a semantic memory-object representation.
    pub(crate) const MEMORY_OBJECT: Self = Self(1 << 2);
    /// The declared operand pair may be exchanged without changing the result.
    pub(crate) const COMMUTATIVE: Self = Self(1 << 3);
    /// The operation is a root of at least one local e-graph rewrite rule.
    pub(crate) const EGRAPH_REWRITE: Self = Self(1 << 4);

    /// Returns the union of two property sets.
    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

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

    /// Returns whether a result type is consistent with the operation's result kind.
    ///
    /// Word-producing operations carry the precise Solidity type of the value they
    /// compute, so any word type is admitted there. Boolean operations produce
    /// `bool`, or the 256-bit word when lowered from inline assembly, where every
    /// value is a word.
    pub(crate) fn admits_type(self, ty: MirType) -> bool {
        match self {
            Self::None | Self::Custom => true,
            Self::Word | Self::SignedWord => !matches!(ty, MirType::Void | MirType::Function),
            Self::Bool => matches!(ty, MirType::Bool) || ty == MirType::uint256(),
            Self::Address => matches!(ty, MirType::Address),
            Self::Bytes32 => matches!(ty, MirType::FixedBytes(_)),
            Self::MemPtr => matches!(ty, MirType::MemPtr),
        }
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
    /// Whether the field is exactly one value operand.
    const IS_OPERAND: bool;
    /// Builds the field from one value operand; only value operands support this.
    fn from_operand(value: ValueId) -> Self
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

    const IS_OPERAND: bool = true;

    #[inline]
    fn from_operand(value: ValueId) -> Self {
        value
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

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("an optional operand is not built from one value")
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

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("an operand list is not built from one value")
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

    const IS_OPERAND: bool = false;

    fn from_operand(_: ValueId) -> Self {
        unreachable!("phi incoming values are not built from one value")
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
        impl crate::mir::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub(crate) fn $name(&mut self) -> ValueId {
                let kind = $inst::$variant;
                let ty = kind.op_def().result.default_type();
                // %result = op()
                self.emit_inst(kind, ty)
            }
        }
    };
    ($inst:ident::$variant:ident; [$name:ident]; ($($operand:ident)+); {}) => {
        impl crate::mir::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub(crate) fn $name(&mut self $(, $operand: ValueId)+) -> ValueId {
                let kind = $inst::$variant($($operand),+);
                let ty = kind.op_def().result.default_type();
                // %result = op(operands)
                self.emit_inst(kind, ty)
            }
        }
    };
    ($inst:ident::$variant:ident; [$name:ident, void]; ($($operand:ident)+); {}) => {
        impl crate::mir::FunctionBuilder<'_> {
            #[doc = concat!("Emits `", stringify!($name), "`.")]
            pub(crate) fn $name(&mut self $(, $operand: ValueId)+) {
                // op(operands)
                self.emit_void_inst($inst::$variant($($operand),+));
            }
        }
    };
}

/// Returns the ISLE term name of an operation: the lower-cased variant name,
/// with the bitwise operations named as in Cranelift because `and` is an ISLE
/// keyword.
#[cfg(test)]
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
                $(#[mnemonic($mnemonic_pattern:pat => $alternate_mnemonic:literal)])*
                $(#[commutative($lhs:ident, $rhs:ident)])?
                $(#[builder($builder:ident $(, $void:ident)?)])?
                $variant:ident
                $( ( $( $operand:ident : $operand_ty:ty ),+ $(,)? ) )?
                $( { $( $(#[$field_meta:meta])* $field:ident : $field_ty:ty ),+ $(,)? } )?
            ),+ $(,)?
        }
    ) => {
        /// The kind of a MIR instruction.
        #[derive(Clone, Debug, PartialEq)]
        pub(crate) enum $inst_name {
            $(
                $(#[doc = $doc])*
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
            /// All declared textual names, including attribute-dependent spellings.
            #[cfg(test)]
            pub(crate) const MNEMONICS: &[&str] = &[
                $( $mnemonic, $( $alternate_mnemonic, )* )+
            ];

            /// Returns the declarative definition for this operation.
            #[inline]
            #[must_use]
            pub(crate) const fn op_def(&self) -> &'static OpDef {
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
                                $( $mnemonic_pattern => &OpDef { mnemonic: $alternate_mnemonic, ..DEF }, )*
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

            /// Returns the arity and constructor of an operation that is built
            /// from value operands alone, by textual mnemonic.
            #[must_use]
            pub(crate) fn operand_only(
                mnemonic: &str,
            ) -> Option<(usize, fn(&[ValueId]) -> Self)> {
                match mnemonic {
                    $(
                        $mnemonic $( | $alternate_mnemonic )* => build::$variant::operand_only(),
                    )+
                    _ => None,
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
        pub(crate) enum Op {
            $(
                $variant
                $( { $( $operand: <$operand_ty as Operands>::View ),+ } )?
                $( { $( $field: <$field_ty as Operands>::View ),+ } )?,
            )+
        }

        impl Op {
            /// Orders each declared commutative pair for value-numbering keys.
            /// Other operands, including a modular operation's modulus, stay in place.
            pub(crate) fn canonicalize_commutative(self) -> Self {
                match self {
                    $(
                        Self::$variant $( { $( $operand ),+ } )? $( { $( $field ),+ } )? => {
                            $(
                                // op(lhs, rhs, rest) -> op(min(lhs, rhs), max(lhs, rhs), rest)
                                let ($lhs, $rhs) = if $rhs.index() < $lhs.index() {
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
            #[cfg(test)]
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
        out.push_str("))\n");
        out
    }

    /// Returns value-definition extractors for rules that inspect MIR operands.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn isle_extractors() -> String {
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
    // Arithmetic operations
    /// Addition: `a + b`
    #[mir_op(
        mnemonic = "add",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(add)]
    Add(a: ValueId, b: ValueId),
    /// Subtraction: `a - b`
    #[mir_op(
        mnemonic = "sub",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(sub)]
    Sub(a: ValueId, b: ValueId),
    /// Multiplication: `a * b`
    #[mir_op(
        mnemonic = "mul",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(mul)]
    Mul(a: ValueId, b: ValueId),
    /// Unsigned division: `a / b`
    #[mir_op(
        mnemonic = "div",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(div)]
    Div(a: ValueId, b: ValueId),
    /// Signed division: `a / b`
    #[mir_op(
        mnemonic = "sdiv",
        result = SignedWord,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(sdiv)]
    SDiv(a: ValueId, b: ValueId),
    /// Unsigned modulo: `a % b`
    #[mir_op(
        mnemonic = "mod",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(mod_)]
    Mod(a: ValueId, b: ValueId),
    /// Signed modulo: `a % b`
    #[mir_op(
        mnemonic = "smod",
        result = SignedWord,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(smod)]
    SMod(a: ValueId, b: ValueId),
    /// Exponentiation: `a ** b`
    #[mir_op(
        mnemonic = "exp",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(exp)]
    Exp(a: ValueId, b: ValueId),
    /// Add modulo: `(a + b) % n`
    #[mir_op(
        mnemonic = "addmod",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(addmod)]
    AddMod(a: ValueId, b: ValueId, n: ValueId),
    /// Multiply modulo: `(a * b) % n`
    #[mir_op(
        mnemonic = "mulmod",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(mulmod)]
    MulMod(a: ValueId, b: ValueId, n: ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    #[mir_op(
        mnemonic = "and",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(and)]
    And(a: ValueId, b: ValueId),
    /// Bitwise OR: `a | b`
    #[mir_op(
        mnemonic = "or",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(or)]
    Or(a: ValueId, b: ValueId),
    /// Bitwise XOR: `a ^ b`
    #[mir_op(
        mnemonic = "xor",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::REMATERIALIZABLE).union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(xor)]
    Xor(a: ValueId, b: ValueId),
    /// Bitwise NOT: `~a`
    #[mir_op(
        mnemonic = "not",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(not)]
    Not(a: ValueId),
    /// Count leading zero bits.
    #[mir_op(
        mnemonic = "clz",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(clz)]
    Clz(a: ValueId),
    /// Left shift: `a << b`
    #[mir_op(
        mnemonic = "shl",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(shl)]
    Shl(shift: ValueId, value: ValueId),
    /// Logical right shift: `a >> b`
    #[mir_op(
        mnemonic = "shr",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(shr)]
    Shr(shift: ValueId, value: ValueId),
    /// Arithmetic right shift: `a >> b` (signed)
    #[mir_op(
        mnemonic = "sar",
        result = SignedWord,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REMATERIALIZABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(sar)]
    Sar(shift: ValueId, value: ValueId),
    /// Extract a byte: `byte(i, x)`
    #[mir_op(
        mnemonic = "byte",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(byte)]
    Byte(index: ValueId, value: ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    #[mir_op(
        mnemonic = "lt",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(lt)]
    Lt(a: ValueId, b: ValueId),
    /// Greater than (unsigned): `a > b`
    #[mir_op(
        mnemonic = "gt",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(gt)]
    Gt(a: ValueId, b: ValueId),
    /// Less than (signed): `a < b`
    #[mir_op(
        mnemonic = "slt",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(slt)]
    SLt(a: ValueId, b: ValueId),
    /// Greater than (signed): `a > b`
    #[mir_op(
        mnemonic = "sgt",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[builder(sgt)]
    SGt(a: ValueId, b: ValueId),
    /// Equality: `a == b`
    #[mir_op(
        mnemonic = "eq",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::REORDERABLE.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = None
    )]
    #[commutative(a, b)]
    #[builder(eq)]
    Eq(a: ValueId, b: ValueId),
    /// Check if zero: `a == 0`
    #[mir_op(
        mnemonic = "iszero",
        result = Bool,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(iszero)]
    IsZero(a: ValueId),

    // Memory operations
    /// Load from memory: `mload(offset)`
    #[mir_op(
        mnemonic = "mload",
        result = Word,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(mload)]
    MLoad(offset: ValueId),
    /// Store to memory: `mstore(offset, value)`
    #[mir_op(
        mnemonic = "mstore",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(mstore, void)]
    MStore(offset: ValueId, value: ValueId),
    /// Store a single byte: `mstore8(offset, value)`
    #[mir_op(
        mnemonic = "mstore8",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(mstore8, void)]
    MStore8(offset: ValueId, value: ValueId),
    /// Set a contiguous memory range to zero: `memory_zero(offset, size)`
    #[mir_op(
        mnemonic = "memory_zero",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("memory zero")
    )]
    #[builder(memory_zero, void)]
    MemoryZero(offset: ValueId, size: ValueId),
    /// Get memory size: `msize()`
    #[mir_op(
        mnemonic = "msize",
        result = Word,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(msize)]
    MSize,
    /// Read the free-memory pointer.
    #[mir_op(
        mnemonic = "fmp",
        result = MemPtr,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("abstract allocation")
    )]
    #[builder(fmp)]
    Fmp,
    /// Set the free-memory pointer.
    #[mir_op(
        mnemonic = "set_fmp",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("abstract allocation")
    )]
    SetFmp(value: ValueId),
    /// Reserve memory and return the previous free-memory pointer.
    #[mir_op(
        mnemonic = "alloc",
        result = Custom,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("abstract allocation")
    )]
    Alloc {
        /// Requested byte count.
        size: ValueId,
        /// Semantic shape of the returned reference.
        kind: AllocationKind,
        /// Alignment, initialization, and failure behavior.
        semantics: AllocationSemantics,
    },
    /// Read the logical length of a dynamic memory object.
    #[mir_op(
        mnemonic = "memory_object_len",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectLen(object: ValueId, kind: MemoryObjectKind),
    /// Set the logical length of a dynamic memory object.
    #[mir_op(
        mnemonic = "set_memory_object_len",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
    SetMemoryObjectLen(object: ValueId, len: ValueId, kind: MemoryObjectKind),
    /// Project the address of the first payload byte from an object.
    #[mir_op(
        mnemonic = "memory_object_data",
        result = MemPtr,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = Pure,
        traits = OpTraits::MEMORY_OBJECT.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectData(object: ValueId, kind: MemoryObjectKind),
    /// Address a direct field of a struct object.
    #[mir_op(
        mnemonic = "memory_object_field_addr",
        result = MemPtr,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = Pure,
        traits = OpTraits::MEMORY_OBJECT.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectFieldAddr {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Address an array element under the semantic object layout.
    #[mir_op(
        mnemonic = "memory_object_element_addr",
        result = MemPtr,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = Pure,
        traits = OpTraits::MEMORY_OBJECT.union(OpTraits::EGRAPH_REWRITE),
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectElementAddr {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one direct struct field without exposing its physical address.
    #[mir_op(
        mnemonic = "memory_object_load_field",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectLoadField {
        /// Struct object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Zero-based direct field index.
        field: u64,
    },
    /// Store one direct struct field without exposing its physical address.
    #[mir_op(
        mnemonic = "memory_object_store_field",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "memory_object_load_element",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectLoadElement {
        /// Array object reference.
        object: ValueId,
        /// Complete direct-object layout.
        layout: MemoryObjectLayout,
        /// Runtime element index.
        index: ValueId,
    },
    /// Load one byte from a bytes object without exposing its physical address.
    #[mir_op(
        mnemonic = "memory_object_load_byte",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    MemoryObjectLoadByte {
        /// Bytes object reference.
        object: ValueId,
        /// Runtime byte index.
        index: ValueId,
    },
    /// Store one array element without exposing its physical address.
    #[mir_op(
        mnemonic = "memory_object_store_element",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "memory_object_store_byte",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "memory_object_store_word",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "memory_slice_load_word",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    MemorySliceLoadWord {
        /// Memory slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Load one word from a calldata slice at a byte offset without exposing
    /// the physical calldata address.
    #[mir_op(
        mnemonic = "calldata_slice_load_word",
        result = Word,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = EnvironmentRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    CalldataSliceLoadWord {
        /// Calldata slice reference.
        slice: ValueId,
        /// Runtime byte offset from the slice start.
        offset: ValueId,
    },
    /// Copy a typed slice into the payload of a dynamic memory object.
    #[mir_op(
        mnemonic = "memory_object_copy_from_slice",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
    MemoryObjectCopyFromSlice {
        /// Destination memory object reference.
        object: ValueId,
        /// Dynamic memory object kind.
        kind: MemoryObjectKind,
        /// Source logical slice.
        source: ValueId,
    },
    /// Copy a typed slice into a byte offset in a dynamic memory object.
    #[mir_op(
        mnemonic = "memory_object_copy_from_slice_at",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "memory_object_copy",
        result = None,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = true,
        category = Some("memory-object")
    )]
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
    #[mir_op(
        mnemonic = "abi_encode",
        result = Custom,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("ABI encoding")
    )]
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
    #[mir_op(
        mnemonic = "abi_decode",
        result = Custom,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("ABI decoding")
    )]
    AbiDecode {
        /// ABI-encoded bytes object.
        data: ValueId,
        /// Interned ABI input layout, including scalar validation types.
        layout: AbiParamLayoutRef,
    },
    /// Copy a statically shaped aggregate from storage into an existing memory allocation.
    #[mir_op(
        mnemonic = "storage_to_memory",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("aggregate")
    )]
    StorageToMemory {
        /// Base storage slot.
        storage: ValueId,
        /// Destination memory pointer.
        memory: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy a statically shaped aggregate from memory into storage.
    #[mir_op(
        mnemonic = "memory_to_storage",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = StorageWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("aggregate")
    )]
    MemoryToStorage {
        /// Source memory pointer.
        memory: ValueId,
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Clear every storage slot occupied by a statically shaped aggregate.
    #[mir_op(
        mnemonic = "clear_storage",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = StorageWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("aggregate")
    )]
    ClearStorage {
        /// Base storage slot.
        storage: ValueId,
        /// Aggregate layout.
        layout: StorageLayoutRef,
    },
    /// Copy memory: `mcopy(dest, src, len)`
    #[mir_op(
        mnemonic = "mcopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(mcopy, void)]
    MCopy(dest: ValueId, src: ValueId, len: ValueId),

    // Storage operations
    /// Load from storage: `sload(slot)`
    #[mir_op(
        mnemonic = "sload",
        result = Word,
        phases = PhaseSet::ALL,
        effect = StorageRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(sload)]
    SLoad(slot: ValueId),
    /// Store to storage: `sstore(slot, value)`
    #[mir_op(
        mnemonic = "sstore",
        result = None,
        phases = PhaseSet::ALL,
        effect = StorageWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(sstore, void)]
    SStore(slot: ValueId, value: ValueId),
    /// Transient load: `tload(slot)`
    #[mir_op(
        mnemonic = "tload",
        result = Word,
        phases = PhaseSet::ALL,
        effect = TransientRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(tload)]
    TLoad(slot: ValueId),
    /// Transient store: `tstore(slot, value)`
    #[mir_op(
        mnemonic = "tstore",
        result = None,
        phases = PhaseSet::ALL,
        effect = TransientWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(tstore, void)]
    TStore(slot: ValueId, value: ValueId),

    // Calldata operations
    /// Load from calldata: `calldataload(offset)`
    #[mir_op(
        mnemonic = "calldataload",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(calldataload)]
    CalldataLoad(offset: ValueId),
    /// Copy calldata to memory: `calldatacopy(destOffset, offset, size)`
    #[mir_op(
        mnemonic = "calldatacopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(calldatacopy, void)]
    CalldataCopy(dest: ValueId, offset: ValueId, size: ValueId),
    /// Get calldata size: `calldatasize()`
    #[mir_op(
        mnemonic = "calldatasize",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(calldatasize)]
    CalldataSize,
    /// Construct a logical `(pointer, length, location)` slice.
    #[mir_op(
        mnemonic = "make_memory_slice",
        result = Custom,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("slice")
    )]
    #[mnemonic(InstKind::MakeSlice { location: SliceLocation::Calldata, .. } => "make_calldata_slice")]
    #[mnemonic(InstKind::MakeSlice { location: SliceLocation::Returndata, .. } => "make_returndata_slice")]
    MakeSlice {
        /// Address of the first element or byte.
        ptr: ValueId,
        /// Logical element or byte length.
        len: ValueId,
        /// Address space containing the slice data.
        location: SliceLocation,
    },
    /// Project the data pointer from a slice.
    #[mir_op(
        mnemonic = "slice_ptr",
        result = Word,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("slice")
    )]
    #[builder(slice_ptr)]
    SlicePtr(slice: ValueId),
    /// Project the logical length from a slice.
    #[mir_op(
        mnemonic = "slice_len",
        result = Word,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("slice")
    )]
    #[builder(slice_len)]
    SliceLen(slice: ValueId),
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
    InternalFrameAddr(offset: u64),
    /// Load a mutable local through its logical frame slot.
    ///
    /// A plain memory read: deletable when its result is dead. Ordering
    /// against frame stores, calls, and other frame traffic is carried by
    /// effect kinds and the alias model's `frame_location`.
    #[mir_op(
        mnemonic = "frame_load",
        result = Custom,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("frame slot")
    )]
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
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("frame slot")
    )]
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
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(constructor_args_base)]
    ConstructorArgsBase,
    /// End address of the constructor's copied ABI argument blob.
    #[mir_op(
        mnemonic = "constructor_args_end",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(constructor_args_end)]
    ConstructorArgsEnd,

    // Code operations
    /// Copy constant module data to memory.
    #[mir_op(
        mnemonic = "data_copy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::REORDERABLE,
        side_effects = true,
        category = None
    )]
    DataCopy(data: DataRef, dest: ValueId, size: ValueId),
    /// Get code size: `codesize()`
    #[mir_op(
        mnemonic = "codesize",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(codesize)]
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
    CodeCopy(dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code size: `extcodesize(addr)`
    #[mir_op(
        mnemonic = "extcodesize",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(extcodesize)]
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
    ExtCodeCopy(addr: ValueId, dest: ValueId, offset: ValueId, size: ValueId),
    /// Get external code hash: `extcodehash(addr)`
    #[mir_op(
        mnemonic = "extcodehash",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(extcodehash)]
    ExtCodeHash(addr: ValueId),
    /// Assign an immutable during construction: `storeimmutable <name>, value`.
    /// Lowered to constructor staging memory after MIR optimization.
    #[mir_op(
        mnemonic = "storeimmutable",
        result = None,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = ImmutableWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = Some("immutable assignment")
    )]
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
    LoadImmutable(id: ImmutableId),

    // Return data operations
    /// Get the current call's return data size: `returndatasize()`.
    ///
    /// Raw volatile query used by Yul and high-level call lowering.
    #[mir_op(
        mnemonic = "returndatasize",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(returndatasize)]
    ReturnDataSize,
    /// Copy return data to memory: `returndatacopy(destOffset, offset, size)`
    #[mir_op(
        mnemonic = "returndatacopy",
        result = None,
        phases = PhaseSet::ALL,
        effect = MemoryWrite,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(returndatacopy, void)]
    ReturnDataCopy(dest: ValueId, offset: ValueId, size: ValueId),

    // Environment operations
    /// Get caller address: `caller()`
    #[mir_op(
        mnemonic = "caller",
        result = Address,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(caller)]
    Caller,
    /// Get call value: `callvalue()`
    #[mir_op(
        mnemonic = "callvalue",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(callvalue)]
    CallValue,
    /// Get origin address: `origin()`
    #[mir_op(
        mnemonic = "origin",
        result = Address,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(origin)]
    Origin,
    /// Get gas price: `gasprice()`
    #[mir_op(
        mnemonic = "gasprice",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(gasprice)]
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    #[mir_op(
        mnemonic = "blockhash",
        result = Bytes32,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(blockhash)]
    BlockHash(number: ValueId),
    /// Get coinbase address: `coinbase()`
    #[mir_op(
        mnemonic = "coinbase",
        result = Address,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(coinbase)]
    Coinbase,
    /// Get block timestamp: `timestamp()`
    #[mir_op(
        mnemonic = "timestamp",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(timestamp)]
    Timestamp,
    /// Get block number: `number()`
    #[mir_op(
        mnemonic = "number",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(number)]
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    #[mir_op(
        mnemonic = "prevrandao",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(prevrandao)]
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    #[mir_op(
        mnemonic = "gaslimit",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(gaslimit)]
    GasLimit,
    /// Get beacon chain slot number: `slotnum()`
    #[mir_op(
        mnemonic = "slotnum",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    SlotNum,
    /// Get chain ID: `chainid()`
    #[mir_op(
        mnemonic = "chainid",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(chainid)]
    ChainId,
    /// Get this contract's address: `address()`
    #[mir_op(
        mnemonic = "address",
        result = Address,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(address)]
    Address,
    /// Get balance: `balance(addr)`
    #[mir_op(
        mnemonic = "balance",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(balance)]
    Balance(addr: ValueId),
    /// Get self balance: `selfbalance()`
    #[mir_op(
        mnemonic = "selfbalance",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(selfbalance)]
    SelfBalance,
    /// Get remaining gas: `gas()`
    #[mir_op(
        mnemonic = "gas",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(gas)]
    Gas,
    /// Get base fee: `basefee()`
    #[mir_op(
        mnemonic = "basefee",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(basefee)]
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    #[mir_op(
        mnemonic = "blobbasefee",
        result = Word,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::REMATERIALIZABLE,
        side_effects = false,
        category = None
    )]
    #[builder(blobbasefee)]
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    #[mir_op(
        mnemonic = "blobhash",
        result = Bytes32,
        phases = PhaseSet::ALL,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(blobhash)]
    BlobHash(index: ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    #[mir_op(
        mnemonic = "keccak256",
        result = Bytes32,
        phases = PhaseSet::ALL,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = None
    )]
    #[builder(keccak256)]
    Keccak256(offset: ValueId, size: ValueId),
    /// Keccak256 hash of a `memorybytes` object's contents:
    /// `keccak256_bytes(object)`.
    ///
    /// Consumes the object reference directly, so the optimizer sees one
    /// whole-object read instead of separate length and data-pointer
    /// projections. `lower-memory-objects` expands it into those projections
    /// and a physical `keccak256`.
    #[mir_op(
        mnemonic = "keccak256_bytes",
        result = Bytes32,
        phases = PhaseSet::THROUGH_DISPATCH,
        effect = MemoryRead,
        traits = OpTraits::MEMORY_OBJECT,
        side_effects = false,
        category = Some("memory-object")
    )]
    #[builder(keccak256_bytes)]
    Keccak256Bytes(object: ValueId),
    /// Hash a fixed-width mapping key and its parent slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    #[mir_op(
        mnemonic = "mapping_slot",
        result = Bytes32,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("storage slot")
    )]
    #[builder(mapping_slot)]
    MappingSlot(key: ValueId, slot: ValueId),
    /// Hash a `[length][data...]` memory value and its parent mapping slot.
    #[mir_op(
        mnemonic = "mapping_slot_memory",
        result = Bytes32,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = MemoryRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("storage slot")
    )]
    #[builder(mapping_slot_memory)]
    MappingSlotMemory(key: ValueId, slot: ValueId),
    /// Hash a dynamically-sized calldata value and its parent mapping slot.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    #[mir_op(
        mnemonic = "mapping_slot_calldata",
        result = Bytes32,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = EnvironmentRead,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("storage slot")
    )]
    #[builder(mapping_slot_calldata)]
    MappingSlotCalldata(key: ValueId, slot: ValueId),
    /// Hash the slot of a dynamically-sized storage array to find its data.
    ///
    /// The temporary scratch memory used by its late lowering is not an
    /// observable part of this instruction's MIR semantics.
    #[mir_op(
        mnemonic = "storage_array_data_slot",
        result = Bytes32,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("storage slot")
    )]
    #[builder(storage_array_data_slot)]
    StorageArrayDataSlot(slot: ValueId),
    /// Resolve one element slot in a dynamic storage array.
    ///
    /// The array's base slot, element index, and logical slot stride stay
    /// semantic until the mapping-slot lowering pass expands the hash and
    /// offset calculation.
    #[mir_op(
        mnemonic = "storage_array_element_slot",
        result = Bytes32,
        phases = PhaseSet::THROUGH_MEMORY_LOWERED,
        effect = Pure,
        traits = OpTraits::NONE,
        side_effects = false,
        category = Some("storage slot")
    )]
    StorageArrayElementSlot { slot: ValueId, index: ValueId, element_slots: u64 },

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    #[mir_op(
        mnemonic = "call",
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
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
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
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
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
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
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// EOF external call: `extcall(addr, argsOffset, argsSize, value)`.
    #[mir_op(
        mnemonic = "extcall",
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    ExtCall { addr: ValueId, args_offset: ValueId, args_size: ValueId, value: ValueId },
    /// EOF external delegate call: `extdelegatecall(addr, argsOffset, argsSize)`.
    #[mir_op(
        mnemonic = "extdelegatecall",
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    ExtDelegateCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
    /// EOF external static call: `extstaticcall(addr, argsOffset, argsSize)`.
    #[mir_op(
        mnemonic = "extstaticcall",
        result = Word,
        phases = PhaseSet::ALL,
        effect = ExternalCall,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    ExtStaticCall { addr: ValueId, args_offset: ValueId, args_size: ValueId },
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
    ICall { function: FunctionId, args: Box<[ValueId]>, returns: u32 },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    #[mir_op(
        mnemonic = "create",
        result = Address,
        phases = PhaseSet::ALL,
        effect = Create,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    #[builder(create)]
    Create(value: ValueId, offset: ValueId, size: ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    #[mir_op(
        mnemonic = "create2",
        result = Address,
        phases = PhaseSet::ALL,
        effect = Create,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Create2(value: ValueId, offset: ValueId, size: ValueId, salt: ValueId),

    // Log operations
    // TODO(codegen): Consider unifying log0..log4 as one instruction with a topic list.
    /// Log with no topics: `log0(offset, size)`
    #[mir_op(
        mnemonic = "log0",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Log0(offset: ValueId, size: ValueId),
    /// Log with 1 topic: `log1(offset, size, topic1)`
    #[mir_op(
        mnemonic = "log1",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Log1(offset: ValueId, size: ValueId, topic1: ValueId),
    /// Log with 2 topics: `log2(offset, size, topic1, topic2)`
    #[mir_op(
        mnemonic = "log2",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Log2(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId),
    /// Log with 3 topics: `log3(offset, size, topic1, topic2, topic3)`
    #[mir_op(
        mnemonic = "log3",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Log3(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId, topic3: ValueId),
    /// Log with 4 topics: `log4(offset, size, topic1, topic2, topic3, topic4)`
    #[mir_op(
        mnemonic = "log4",
        result = None,
        phases = PhaseSet::ALL,
        effect = Log,
        traits = OpTraits::NONE,
        side_effects = true,
        category = None
    )]
    Log4(offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId, topic3: ValueId, topic4: ValueId),

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
    Phi(incoming: Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    #[mir_op(
        mnemonic = "select",
        result = Word,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    Select(cond: ValueId, true_val: ValueId, false_val: ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    #[mir_op(
        mnemonic = "signextend",
        result = SignedWord,
        phases = PhaseSet::ALL,
        effect = Pure,
        traits = OpTraits::EGRAPH_REWRITE,
        side_effects = false,
        category = None
    )]
    #[builder(signextend)]
    SignExtend(byte: ValueId, value: ValueId),
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
        assert!(calldata_size.op_def().traits.contains(OpTraits::REMATERIALIZABLE));
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
    fn commutativity_applies_only_to_the_declared_pair() {
        let a = ValueId::new(2);
        let b = ValueId::new(1);
        let addmod = InstKind::AddMod(a, b, a);
        assert!(addmod.op_def().traits.contains(OpTraits::COMMUTATIVE));
        assert_eq!(addmod.op().canonicalize_commutative(), Op::AddMod { a: b, b: a, n: a });

        let gt = InstKind::Gt(a, b);
        assert!(gt.op_def().traits.contains(OpTraits::REORDERABLE));
        assert!(!gt.op_def().traits.contains(OpTraits::COMMUTATIVE));
        assert_eq!(gt.op().canonicalize_commutative(), gt.op());
    }

    #[test]
    fn isle_prelude_matches_schema() {
        snapbox::assert_data_eq!(Op::isle_prelude(), snapbox::file!["../../isle/prelude.isle"]);
        snapbox::assert_data_eq!(
            Op::isle_extractors(),
            snapbox::file!["../../isle/extractors.isle"]
        );
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
