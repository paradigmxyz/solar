//! Semantic ABI layout descriptors used by MIR encoding operations.

use super::{FunctionBuilder, PtrLocation, ValueId, ValueLayout};
use alloy_primitives::U256;
use solar_data_structures::map::{FxBuildHasher, FxHashMap, FxHashSet};
use std::{
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};

/// An interned ABI tuple layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AbiLayout {
    /// Types encoded as one ABI tuple.
    pub types: Box<[AbiType]>,
}

impl AbiLayout {
    /// Creates a tuple layout from its element types.
    #[must_use]
    pub fn new(types: impl Into<Box<[AbiType]>>) -> Self {
        Self { types: types.into() }
    }

    /// Returns the tuple head size in bytes.
    #[must_use]
    pub fn head_size(&self) -> u64 {
        self.types.iter().map(AbiType::head_size).sum()
    }
}

/// Shared reference returned by the module ABI-layout interner.
pub type AbiLayoutRef = Arc<AbiLayout>;

/// ABI input shape retained until the ABI lowering phase.
///
/// Unlike [`AbiType`], scalar leaves keep their MIR type so ABI lowering can
/// validate narrow words before it stores them in an aggregate object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AbiParamLayout {
    /// Types encoded as one ABI tuple.
    pub types: Box<[AbiParamType]>,
}

impl AbiParamLayout {
    /// Creates an input tuple layout.
    #[must_use]
    pub fn new(types: impl Into<Box<[AbiParamType]>>) -> Self {
        Self { types: types.into() }
    }

    /// Returns the tuple head size, or `None` when it exceeds the ABI layout range.
    #[must_use]
    pub fn checked_head_size(&self) -> Option<u64> {
        self.types.iter().try_fold(0u64, |size, ty| size.checked_add(ty.checked_head_size()?))
    }
}

/// Shared reference returned by the module ABI input-layout interner.
pub type AbiParamLayoutRef = Arc<AbiParamLayout>;

/// Source data location of an ABI parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbiParamLocation {
    /// A memory-backed parameter is decoded before entering the body.
    Memory,
    /// A calldata-backed parameter remains lazy until the body uses it.
    Calldata,
}

impl fmt::Display for AbiParamLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut printer = ShapePrinter::new(&self.types);
        write!(f, "[")?;
        for (index, ty) in self.types.iter().enumerate() {
            if index != 0 {
                write!(f, ", ")?;
            }
            printer.write(f, ty)?;
        }
        write!(f, "]")
    }
}

/// ABI input shape with scalar type information for decoding.
///
/// Nested shapes are shared [`AbiParamTypeRef`] nodes, so a struct that holds several copies of
/// another struct refers to one node for it instead of owning a copy each.
#[allow(clippy::derived_hash_with_manual_eq, reason = "equality only memoizes the derived one")]
#[derive(Clone, Debug, Eq, Hash)]
pub enum AbiParamType {
    /// A scalar encoded as one word.
    Scalar(ValueLayout),
    /// An enum encoded as a bounded unsigned word.
    Enum {
        /// MIR representation of the enum value.
        ty: ValueLayout,
        /// Number of declared variants.
        variants: u64,
    },
    /// A dynamic byte string.
    Bytes,
    /// A dynamic array.
    DynamicArray(AbiParamTypeRef),
    /// A fixed-size array.
    FixedArray {
        /// Array element layout.
        element: AbiParamTypeRef,
        /// Number of elements.
        len: u64,
    },
    /// A struct or tuple.
    Tuple(Box<[AbiParamTypeRef]>),
}

/// Compares two shapes, remembering the pairs of distinct nodes already found equal so that a
/// node reached along many paths is compared once.
fn shapes_equal(a: &AbiParamType, b: &AbiParamType, equal: &mut NodePairs) -> bool {
    match (a, b) {
        (AbiParamType::Scalar(a), AbiParamType::Scalar(b)) => a == b,
        (
            AbiParamType::Enum { ty: a, variants: a_variants },
            AbiParamType::Enum { ty: b, variants: b_variants },
        ) => a == b && a_variants == b_variants,
        (AbiParamType::Bytes, AbiParamType::Bytes) => true,
        (AbiParamType::DynamicArray(a), AbiParamType::DynamicArray(b)) => nodes_equal(a, b, equal),
        (
            AbiParamType::FixedArray { element: a, len: a_len },
            AbiParamType::FixedArray { element: b, len: b_len },
        ) => a_len == b_len && nodes_equal(a, b, equal),
        (AbiParamType::Tuple(a), AbiParamType::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| nodes_equal(a, b, equal))
        }
        _ => false,
    }
}

/// Pairs of distinct nodes found equal during one comparison.
type NodePairs = FxHashSet<(*const AbiParamTypeNode, *const AbiParamTypeNode)>;

/// Compares two nodes like [`shapes_equal`], deciding at once when they are the same node or
/// their hashes differ.
fn nodes_equal(a: &AbiParamTypeRef, b: &AbiParamTypeRef, equal: &mut NodePairs) -> bool {
    if Arc::ptr_eq(&a.0, &b.0) {
        return true;
    }
    if a.0.hash != b.0.hash {
        return false;
    }
    let key = (Arc::as_ptr(&a.0), Arc::as_ptr(&b.0));
    if equal.contains(&key) {
        return true;
    }
    let result = shapes_equal(&a.0.ty, &b.0.ty, equal);
    if result {
        equal.insert(key);
    }
    result
}

impl PartialEq for AbiParamType {
    fn eq(&self, other: &Self) -> bool {
        shapes_equal(self, other, &mut FxHashSet::default())
    }
}

/// A shared node of an [`AbiParamType`] shape.
///
/// Clones share the node, which caches its hash, whether it is dynamic, its head size, and its
/// depth, so building, hashing, and comparing a shape take time linear in its distinct nodes
/// rather than in the paths through them.
#[derive(Clone)]
pub struct AbiParamTypeRef(Arc<AbiParamTypeNode>);

struct AbiParamTypeNode {
    ty: AbiParamType,
    hash: u64,
    is_dynamic: bool,
    head_size: Option<u64>,
    depth: usize,
}

impl AbiParamTypeRef {
    /// Creates a shared node for `ty`.
    #[must_use]
    pub fn new(ty: AbiParamType) -> Self {
        let hash = FxBuildHasher.hash_one(&ty);
        let is_dynamic = ty.is_dynamic();
        let head_size = ty.checked_head_size();
        let depth = ty.depth();
        Self(Arc::new(AbiParamTypeNode { ty, hash, is_dynamic, head_size, depth }))
    }

    /// Returns whether the ABI value occupies an offset in its containing head.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.0.is_dynamic
    }

    /// Returns the static head size, or `None` when the shape exceeds the
    /// representable ABI layout range.
    #[must_use]
    pub fn checked_head_size(&self) -> Option<u64> {
        self.0.head_size
    }

    /// Returns the nesting depth: zero for a scalar or `bytes`, one more than the deepest child
    /// for an aggregate.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.depth
    }
}

impl std::ops::Deref for AbiParamTypeRef {
    type Target = AbiParamType;

    fn deref(&self) -> &AbiParamType {
        &self.0.ty
    }
}

impl PartialEq for AbiParamTypeRef {
    fn eq(&self, other: &Self) -> bool {
        nodes_equal(self, other, &mut FxHashSet::default())
    }
}

impl Eq for AbiParamTypeRef {}

impl Hash for AbiParamTypeRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.hash);
    }
}

impl fmt::Debug for AbiParamTypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0.ty, f)
    }
}

impl fmt::Display for AbiParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ShapePrinter::new(std::slice::from_ref(self)).write(f, self)
    }
}

/// Prints ABI input shapes in time linear in their distinct nodes.
///
/// A nested aggregate that occurs more than once and itself contains an aggregate is printed in
/// full once as `$N = shape`, where `N` counts such nodes in printing order, and as `$N` after
/// that. Other nodes print in full each time, which stays linear because only nodes whose
/// children are scalars, `bytes`, or empty tuples repeat.
struct ShapePrinter {
    /// Nodes that get a name, with the number they got when first printed.
    named: FxHashMap<AbiParamTypeRef, Option<usize>>,
    next: usize,
}

impl ShapePrinter {
    fn new(types: &[AbiParamType]) -> Self {
        fn count(ty: &AbiParamType, counts: &mut FxHashMap<AbiParamTypeRef, usize>) {
            for child in ty.children() {
                let seen = counts.entry(child.clone()).or_insert(0);
                *seen += 1;
                if *seen == 1 {
                    count(child, counts);
                }
            }
        }
        let mut counts = FxHashMap::default();
        for ty in types {
            count(ty, &mut counts);
        }
        let named = counts
            .into_iter()
            .filter(|(ty, count)| *count >= 2 && ty.depth() >= 2)
            .map(|(ty, _)| (ty, None))
            .collect();
        Self { named, next: 0 }
    }

    fn write(&mut self, f: &mut fmt::Formatter<'_>, ty: &AbiParamType) -> fmt::Result {
        match ty {
            AbiParamType::Scalar(ty) => write!(f, "{ty}"),
            AbiParamType::Enum { ty, variants } => write!(f, "enum<{variants}, {ty}>"),
            AbiParamType::Bytes => f.write_str("bytes"),
            AbiParamType::DynamicArray(element) => {
                f.write_str("array<_, ")?;
                self.write_node(f, element)?;
                f.write_str(">")
            }
            AbiParamType::FixedArray { element, len } => {
                write!(f, "array<{len}, ")?;
                self.write_node(f, element)?;
                f.write_str(">")
            }
            AbiParamType::Tuple(fields) => {
                f.write_str("tuple<")?;
                for (index, field) in fields.iter().enumerate() {
                    if index != 0 {
                        f.write_str(", ")?;
                    }
                    self.write_node(f, field)?;
                }
                f.write_str(">")
            }
        }
    }

    fn write_node(&mut self, f: &mut fmt::Formatter<'_>, node: &AbiParamTypeRef) -> fmt::Result {
        let Some(name) = self.named.get_mut(node) else { return self.write(f, node) };
        if let Some(name) = name {
            return write!(f, "${name}");
        }
        let index = self.next;
        *name = Some(index);
        self.next += 1;
        write!(f, "${index} = ")?;
        self.write(f, node)
    }
}

impl AbiParamType {
    /// Returns the nested shapes: an array's element or a tuple's fields.
    pub fn children(&self) -> impl Iterator<Item = &AbiParamTypeRef> {
        match self {
            Self::DynamicArray(element) | Self::FixedArray { element, .. } => {
                std::slice::from_ref(element)
            }
            Self::Tuple(fields) => fields,
            Self::Scalar(_) | Self::Enum { .. } | Self::Bytes => &[],
        }
        .iter()
    }

    /// Returns the nesting depth: zero for a scalar or `bytes`, one more than the deepest child
    /// for an aggregate.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Scalar(_) | Self::Enum { .. } | Self::Bytes => 0,
            _ => self.children().map(AbiParamTypeRef::depth).max().unwrap_or_default() + 1,
        }
    }

    /// Returns whether ABI return encoding must canonicalize a value in this shape.
    #[must_use]
    pub fn needs_return_cleanup(&self) -> bool {
        match self {
            Self::Scalar(ty) | Self::Enum { ty, .. } => match ty {
                ValueLayout::UInt(size) | ValueLayout::Int(size) => size.bits() < 256,
                ValueLayout::Address | ValueLayout::Bool | ValueLayout::Function => true,
                ValueLayout::FixedBytes(size) => size.bytes() < 32,
                _ => false,
            },
            Self::Bytes => false,
            Self::FixedArray { element, .. } | Self::DynamicArray(element) => {
                element.needs_return_cleanup()
            }
            Self::Tuple(fields) => fields.iter().any(|field| field.needs_return_cleanup()),
        }
    }

    /// Returns the memory representation used for an aggregate child.
    #[must_use]
    pub fn mir_type(&self) -> super::MirType {
        match self {
            Self::Scalar(ty) => ty.mir_type(),
            Self::Enum { ty, .. } => ty.mir_type(),
            Self::Bytes | Self::DynamicArray(_) | Self::FixedArray { .. } | Self::Tuple(_) => {
                super::MirType::MEMPTR
            }
        }
    }

    /// Returns whether this value occupies one ABI word.
    #[must_use]
    pub fn is_scalar_word(&self) -> bool {
        matches!(self, Self::Scalar(_) | Self::Enum { .. })
    }

    /// Returns the validator for a scalar or enum word, if it is not full-width.
    #[must_use]
    pub fn word_validator(&self) -> Option<AbiWordValidator> {
        match self {
            Self::Scalar(ty) => AbiWordValidator::from_layout(*ty),
            Self::Enum { variants, .. } => Some(AbiWordValidator::EnumRange(*variants)),
            _ => None,
        }
    }

    /// Returns whether the ABI value occupies an offset in its containing head.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Bytes | Self::DynamicArray(_)) || self.has_dynamic_child()
    }

    /// Returns whether an aggregate directly contains a dynamic value.
    #[must_use]
    pub fn has_dynamic_child(&self) -> bool {
        self.children().any(AbiParamTypeRef::is_dynamic)
    }

    /// Returns the size of this value's in-place ABI head.
    #[must_use]
    pub fn data_head_size(&self) -> u64 {
        match self {
            Self::FixedArray { element, len } => element
                .checked_head_size()
                .expect("ABI head size exceeds u64 range")
                .saturating_mul(*len),
            Self::Tuple(fields) => fields.iter().fold(0, |size, field| {
                size.saturating_add(
                    field.checked_head_size().expect("ABI head size exceeds u64 range"),
                )
            }),
            _ => 32,
        }
    }

    /// Returns the static head size, or `None` when the shape exceeds the
    /// representable ABI layout range.
    #[must_use]
    pub fn checked_head_size(&self) -> Option<u64> {
        if self.is_dynamic() {
            return Some(32);
        }
        match self {
            Self::FixedArray { element, len } => element.checked_head_size()?.checked_mul(*len),
            Self::Tuple(fields) => fields
                .iter()
                .try_fold(0u64, |size, field| size.checked_add(field.checked_head_size()?)),
            _ => Some(32),
        }
    }
}

/// The ABI-relevant shape and source representation of one value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AbiType {
    /// A scalar encoded as one word, with the cleanup its type requires when the word is read
    /// from memory: narrow integers are masked or sign-extended, booleans canonicalized, and
    /// enums range-checked during encoding. `None` is a full word.
    Word(Option<AbiWordValidator>),
    /// An external function pointer encoded as a left-aligned `bytes24` word.
    Function,
    /// A dynamic byte string represented in the given address space.
    Bytes(PtrLocation),
    /// A dynamic array represented in the given address space.
    DynamicArray {
        /// Array element layout.
        element: Box<Self>,
        /// Address space containing the array.
        location: PtrLocation,
    },
    /// A fixed-size array represented by a memory pointer.
    FixedArray {
        /// Array element layout.
        element: Box<Self>,
        /// Number of elements.
        len: u64,
        /// Whether solc copies the array from calldata as one block, because its elements are
        /// `uint256` or `bytes32`. It reads every other element type one word at a time, which
        /// differs when the array lies across 2^256.
        block_copy: bool,
    },
    /// A struct or tuple represented by a memory pointer.
    Tuple(Box<[Self]>),
}

impl AbiType {
    /// Returns the layout with every byte string and dynamic array placed in `location`.
    #[must_use]
    pub fn at_location(self, location: PtrLocation) -> Self {
        match self {
            Self::Word(_) | Self::Function => self,
            Self::Bytes(_) => Self::Bytes(location),
            Self::DynamicArray { element, .. } => {
                Self::DynamicArray { element: Box::new(element.at_location(location)), location }
            }
            Self::FixedArray { element, len, block_copy } => Self::FixedArray {
                element: Box::new(element.at_location(location)),
                len,
                block_copy,
            },
            Self::Tuple(fields) => Self::Tuple(
                fields.into_vec().into_iter().map(|f| f.at_location(location)).collect(),
            ),
        }
    }

    /// Returns whether a storage byte string occurs in this layout, itself included. One reads
    /// its bytes from a slot operand, which only a top-level layout entry has.
    #[must_use]
    pub fn contains_storage_bytes(&self) -> bool {
        match self {
            Self::Bytes(location) => *location == PtrLocation::Storage,
            Self::Word(_) | Self::Function => false,
            Self::DynamicArray { element, .. } | Self::FixedArray { element, .. } => {
                element.contains_storage_bytes()
            }
            Self::Tuple(fields) => fields.iter().any(Self::contains_storage_bytes),
        }
    }

    /// Returns whether the ABI value occupies an offset in its containing head.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        match self {
            Self::Word(_) | Self::Function => false,
            Self::Bytes(_) | Self::DynamicArray { .. } => true,
            Self::FixedArray { element, .. } => element.is_dynamic(),
            Self::Tuple(fields) => fields.iter().any(Self::is_dynamic),
        }
    }

    /// Returns the size occupied by this value in its containing tuple head.
    #[must_use]
    pub fn head_size(&self) -> u64 {
        if self.is_dynamic() {
            return 32;
        }
        self.tail_size()
    }

    /// Returns the size of the value's own encoding where a tail offset points: the length
    /// word of a dynamically sized value, or the whole head area of a statically sized one.
    /// This is the length a calldata tail access requires.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/ast/Types.h#L263>
    #[must_use]
    pub fn tail_size(&self) -> u64 {
        match self {
            Self::Word(_) | Self::Function | Self::Bytes(_) | Self::DynamicArray { .. } => 32,
            Self::FixedArray { element, len, .. } => element.head_size() * len,
            Self::Tuple(fields) => fields.iter().map(Self::head_size).sum(),
        }
    }
}

impl fmt::Display for AbiLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (index, ty) in self.types.iter().enumerate() {
            if index != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{ty}")?;
        }
        write!(f, "]")
    }
}

impl fmt::Display for AbiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word(None) => write!(f, "word"),
            Self::Word(Some(cleanup)) => write!(f, "word<{cleanup}>"),
            Self::Function => write!(f, "function"),
            // ABI values live in calldata (inputs) or memory (outputs); the
            // location's own `Display` yields the `memory`/`calldata` prefix.
            Self::Bytes(location) => write!(f, "{location}_bytes"),
            Self::DynamicArray { element, location } => write!(f, "{location}_array<{element}>"),
            // A word array that solc does not copy as a block carries `wordwise`.
            Self::FixedArray { element, len, block_copy } => {
                let wordwise = !block_copy && matches!(element.as_ref(), Self::Word(None));
                write!(f, "array<{len}, {element}{}>", if wordwise { ", wordwise" } else { "" })
            }
            Self::Tuple(fields) => {
                write!(f, "tuple<")?;
                for (index, ty) in fields.iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ">")
            }
        }
    }
}

/// Validation and canonicalization of a narrow ABI word.
///
/// Shared by the ABI lowering phase (wrappers, constructors, returns) and the
/// calldata decoding helpers in the function lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AbiWordValidator {
    /// Word must fit in a low-bit range.
    Unsigned(u16),
    /// Word must fit in a high-bit range.
    LeftAligned(u16),
    /// Word must equal its sign-extension of `byte_index` bytes.
    SignExtend(u64),
    /// Word must be a canonical 0 or 1.
    Bool,
    /// Word must be less than the number of enum variants.
    EnumRange(u64),
}

/// Prints the validator as the MIR scalar type it canonicalizes, or `enum N` for a range.
impl fmt::Display for AbiWordValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsigned(bits) => write!(f, "uint{bits}"),
            Self::LeftAligned(bits) => write!(f, "bytes{}", bits / 8),
            Self::SignExtend(byte_index) => write!(f, "int{}", (byte_index + 1) * 8),
            Self::Bool => write!(f, "bool"),
            Self::EnumRange(variants) => write!(f, "enum {variants}"),
        }
    }
}

impl AbiWordValidator {
    /// Returns the validator for a scalar MIR type, or `None` when a full word
    /// carries no canonicality requirement.
    #[must_use]
    pub fn from_layout(ty: ValueLayout) -> Option<Self> {
        Some(match ty {
            ValueLayout::UInt(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::Unsigned(bits)
            }
            ValueLayout::Int(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::SignExtend(u64::from(bits / 8) - 1)
            }
            ValueLayout::Address => Self::Unsigned(160),
            ValueLayout::FixedBytes(size) => {
                let bytes = size.bytes();
                if bytes >= 32 {
                    return None;
                }
                Self::LeftAligned(u16::from(bytes) * 8)
            }
            ValueLayout::Function => Self::LeftAligned(192),
            ValueLayout::Bool => Self::Bool,
            _ => return None,
        })
    }

    /// Returns the validator for a scalar return type.
    #[must_use]
    pub fn from_return_layout(ty: ValueLayout) -> Option<Self> {
        if ty == ValueLayout::Function {
            return Some(Self::Unsigned(192));
        }
        Self::from_layout(ty)
    }

    /// Returns the bit mask for validators that accept a masked word.
    #[must_use]
    pub fn canonical_mask(self) -> Option<U256> {
        Some(match self {
            Self::Unsigned(bits) => U256::MAX >> (256 - usize::from(bits)),
            Self::LeftAligned(bits) => U256::MAX << (256 - usize::from(bits)),
            Self::SignExtend(_) | Self::Bool | Self::EnumRange(_) => return None,
        })
    }

    /// Builds the condition that is true when `word` is not canonical, as a check that aborts on
    /// it reads it: a width test is the comparison with its limit, `x > 2 ** bits - 1`.
    pub fn violation(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Unsigned(bits) => {
                // invalid = word > 2 ** bits - 1
                let limit = builder.imm(U256::MAX >> (256 - usize::from(bits)));
                builder.gt(word, limit)
            }
            Self::LeftAligned(bits) => {
                // shifted = word << bits
                // invalid = shifted != 0
                let shift = builder.imm(u64::from(bits));
                let shifted = builder.shl(word, shift);
                builder.ne_zero(shifted)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm(byte_index);
                let canonical = builder.signextend(byte_index, word);
                builder.ne(word, canonical)
            }
            Self::Bool => {
                let one = builder.imm(1);
                builder.gt(word, one)
            }
            // No word is valid without variants, and a lone variant is a zero test, as no width
            // test has a limit of `0`.
            Self::EnumRange(0) => builder.imm_bool(true),
            Self::EnumRange(1) => builder.ne_zero(word),
            Self::EnumRange(variants) => {
                let last = builder.imm(variants.wrapping_sub(1));
                builder.gt(word, last)
            }
        }
    }

    /// Builds the canonical form of `word`.
    pub fn cleanup(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Unsigned(_) | Self::LeftAligned(_) => {
                let mask = self.canonical_mask().expect("masked validator has a mask");
                let mask = builder.imm(mask);
                builder.and(word, mask)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm(byte_index);
                builder.signextend(byte_index, word)
            }
            Self::Bool => builder.ne_zero(word),
            Self::EnumRange(_) => word,
        }
    }

    /// Returns a validator that accepts a word exactly when its masked-off bits are zero, as a
    /// canonical boolean's are above the lowest, or `None` when canonicality is not a mask. Such a
    /// validator accepts every one of some words exactly when it accepts their bitwise OR.
    #[must_use]
    pub fn zero_bits(self) -> Option<Self> {
        match self {
            Self::Unsigned(_) | Self::LeftAligned(_) => Some(self),
            Self::Bool => Some(Self::Unsigned(1)),
            Self::SignExtend(_) | Self::EnumRange(_) => None,
        }
    }

    /// Whether this rule has a printable word representation and bounded shifts.
    /// Unsigned obligations also admit non-byte widths, as boolean mask proofs do.
    pub fn is_well_formed(self) -> bool {
        match self {
            Self::Unsigned(bits) => (1..=256).contains(&bits),
            Self::LeftAligned(bits) => (8..=256).contains(&bits) && bits.is_multiple_of(8),
            Self::SignExtend(byte_index) => byte_index < 32,
            Self::Bool | Self::EnumRange(_) => true,
        }
    }
}
