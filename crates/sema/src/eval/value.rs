//! Typed constant values and the operations on them.

use super::EvalErrorKind as EE;
use crate::{
    hir::{self, BinOpKind},
    ty::Gcx,
};
use alloy_primitives::{Address, B256, U256};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Signed, Zero};
use solar_ast::{ElementaryType, TypeSize};
use solar_interface::ByteSymbol;
use std::cmp::Ordering;

// Literal arithmetic is exact like solc's rational numbers, which limit the
// precision of every intermediate value to 4096 bits.
const MAX_INTERMEDIATE_BITS: u64 = 4096;

/// A typed Solidity constant value.
#[derive(Clone, Debug)]
pub enum ConstValue {
    /// An integer literal, or a value of an integer type.
    Int(IntScalar),
    /// A `bool` value.
    Bool(bool),
    /// A `bytesN` value.
    FixedBytes(FixedBytes),
    /// An `address` or `address payable` value.
    Address(Address),
    /// A contract of the given type at an address.
    Contract(hir::ContractId, Address),
    /// A string literal, or a `bytes` or `string` value.
    Bytes(ByteSymbol, BytesKind),
    /// The variant of an enum with the given index.
    Enum(hir::EnumId, u32),
    /// A value of a user-defined value type, holding the underlying value.
    Udvt(hir::UdvtId, Box<Self>),
}

/// The type of a [`ConstValue::Bytes`] value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BytesKind {
    /// A string literal, which converts to `bytesN`, `bytes` and `string`.
    Literal,
    /// A `bytes` value.
    Bytes,
    /// A `string` value.
    String,
}

impl ConstValue {
    /// Returns the integer value, if this is one.
    pub fn as_int(&self) -> Option<&IntScalar> {
        match self {
            Self::Int(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the non-negative integer value as unsigned data.
    pub fn as_u256(&self) -> Option<U256> {
        self.as_int()?.as_u256()
    }

    /// Returns the boolean value, if this is a boolean constant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns whether this is an integer constant with value zero.
    pub fn is_zero(&self) -> bool {
        self.as_int().is_some_and(IntScalar::is_zero)
    }

    /// Returns the EVM word that holds this value on the stack: sign-extended integers,
    /// left-aligned `bytesN` and right-aligned addresses. Byte arrays and integer literals wider
    /// than a word have none.
    pub fn as_word(&self) -> Option<U256> {
        match self {
            Self::Int(value) => value.as_evm_word(),
            Self::Bool(value) => Some(U256::from(*value as u8)),
            Self::FixedBytes(value) => Some(value.word()),
            Self::Address(address) | Self::Contract(_, address) => {
                Some(U256::from_be_slice(address.as_slice()))
            }
            Self::Enum(_, index) => Some(U256::from(*index)),
            Self::Udvt(_, value) => value.as_word(),
            Self::Bytes(..) => None,
        }
    }

    /// Returns the type of this value, or `None` for literals.
    pub(super) fn ty(&self) -> Option<ValueTy> {
        Some(match self {
            Self::Int(value) => ValueTy::Int(value.ty?),
            Self::Bool(_) => ValueTy::Bool,
            Self::FixedBytes(value) => ValueTy::FixedBytes(value.size),
            Self::Address(_) => ValueTy::Address,
            Self::Contract(id, _) => ValueTy::Contract(*id),
            Self::Bytes(_, BytesKind::Literal) => return None,
            Self::Bytes(_, BytesKind::Bytes) => ValueTy::Bytes,
            Self::Bytes(_, BytesKind::String) => ValueTy::String,
            Self::Enum(id, _) => ValueTy::Enum(*id),
            Self::Udvt(id, _) => ValueTy::Udvt(*id),
        })
    }

    /// Gives a literal its mobile type: the narrowest integer type of its sign for a number, and
    /// `string` for a string literal.
    pub(super) fn mobile(self) -> Result<Self, EE> {
        Ok(match self {
            Self::Int(value) if value.ty.is_none() => {
                let ty = value.mobile_ty()?;
                Self::Int(value.retype(Some(ty))?)
            }
            Self::Bytes(data, BytesKind::Literal) => Self::Bytes(data, BytesKind::String),
            value => value,
        })
    }

    /// Implicitly converts this value to `ty`, like an assignment does.
    pub(super) fn implicit_to(self, gcx: Gcx<'_>, ty: ValueTy) -> Result<Self, EE> {
        Ok(match (self, ty) {
            (Self::Int(value), ValueTy::Int(ty)) => Self::Int(value.implicit_to(ty)?),
            (Self::Int(value), ValueTy::FixedBytes(size)) if value.ty.is_none() => {
                Self::FixedBytes(FixedBytes::from_literal(size, &value)?)
            }
            (Self::Bool(value), ValueTy::Bool) => Self::Bool(value),
            (Self::FixedBytes(value), ValueTy::FixedBytes(size)) if value.size <= size => {
                Self::FixedBytes(value.resize(size))
            }
            (Self::Address(address), ValueTy::Address) => Self::Address(address),
            (Self::Contract(id, address), ValueTy::Contract(base))
                if gcx.hir.contract(id).linearized_bases.contains(&base) || id == base =>
            {
                Self::Contract(base, address)
            }
            (Self::Bytes(data, BytesKind::Literal), ValueTy::FixedBytes(size))
                if data.as_byte_str().len() <= usize::from(size) =>
            {
                Self::FixedBytes(FixedBytes::new(size, data.as_byte_str()))
            }
            (Self::Bytes(data, BytesKind::Literal | BytesKind::Bytes), ValueTy::Bytes) => {
                Self::Bytes(data, BytesKind::Bytes)
            }
            (Self::Bytes(data, BytesKind::Literal | BytesKind::String), ValueTy::String) => {
                Self::Bytes(data, BytesKind::String)
            }
            (value @ Self::Enum(id, _), ValueTy::Enum(to)) if id == to => value,
            (value @ Self::Udvt(id, _), ValueTy::Udvt(to)) if id == to => value,
            _ => return Err(EE::UnsupportedExpr),
        })
    }

    /// Explicitly converts this value to `ty`, like `T(value)` does.
    pub(super) fn explicit_to(self, gcx: Gcx<'_>, ty: ValueTy) -> Result<Self, EE> {
        Ok(match (self, ty) {
            (Self::Int(value), ValueTy::Int(ty)) => Self::Int(value.explicit_to(ty)?),
            (Self::Int(value), ValueTy::FixedBytes(size)) if value.ty.is_some() => {
                let word = value.as_evm_word().ok_or(EE::OutOfRange)?;
                Self::FixedBytes(FixedBytes::from_uint(size, word))
            }
            (Self::Int(value), ValueTy::Address | ValueTy::Contract(_)) => {
                let word = if value.ty.is_some() {
                    value.as_evm_word().ok_or(EE::OutOfRange)?
                } else {
                    value.as_u256().filter(|value| value.bit_len() <= 160).ok_or(EE::OutOfRange)?
                };
                let address = Address::from_word(word.into());
                match ty {
                    ValueTy::Contract(id) => Self::Contract(id, address),
                    _ => Self::Address(address),
                }
            }
            (Self::Int(value), ValueTy::Enum(id)) => {
                let index = value.as_u256().and_then(|index| u32::try_from(index).ok());
                match index {
                    Some(index) if (index as usize) < gcx.hir.enumm(id).variants.len() => {
                        Self::Enum(id, index)
                    }
                    _ => return Err(EE::OutOfRange),
                }
            }
            (Self::FixedBytes(value), ValueTy::FixedBytes(size)) => {
                Self::FixedBytes(value.resize(size))
            }
            (Self::FixedBytes(value), ValueTy::Int(ty)) => {
                Self::Int(IntScalar::new(value.to_uint()).explicit_to(ty)?)
            }
            (Self::FixedBytes(value), ValueTy::Address) if value.size == 20 => {
                Self::Address(Address::from_slice(value.as_slice()))
            }
            (Self::Address(address), ValueTy::Int(ty)) => {
                Self::Int(IntScalar::new(U256::from_be_slice(address.as_slice())).explicit_to(ty)?)
            }
            (Self::Address(address), ValueTy::FixedBytes(20)) => {
                Self::FixedBytes(FixedBytes::new(20, address.as_slice()))
            }
            (Self::Address(address) | Self::Contract(_, address), ValueTy::Address) => {
                Self::Address(address)
            }
            (Self::Address(address) | Self::Contract(_, address), ValueTy::Contract(id)) => {
                Self::Contract(id, address)
            }
            (Self::Bytes(data, _), ValueTy::Bytes) => Self::Bytes(data, BytesKind::Bytes),
            (Self::Bytes(data, _), ValueTy::String) => Self::Bytes(data, BytesKind::String),
            (Self::Bytes(data, BytesKind::Bytes), ValueTy::FixedBytes(size)) => {
                let data = data.as_byte_str();
                Self::FixedBytes(FixedBytes::new(size, &data[..data.len().min(size.into())]))
            }
            (Self::Enum(_, index), ValueTy::Int(ty)) => {
                Self::Int(IntScalar::new(U256::from(index)).implicit_to(ty)?)
            }
            (value, ty) => value.implicit_to(gcx, ty)?,
        })
    }

    /// Converts both values to their common type, like the branches of a conditional.
    pub(super) fn unify(self, gcx: Gcx<'_>, other: Self) -> Result<(Self, Self), EE> {
        let (a, b) = (self.mobile()?, other.mobile()?);
        let (Some(a_ty), Some(b_ty)) = (a.ty(), b.ty()) else { return Err(EE::UnsupportedExpr) };
        let common = match (a_ty, b_ty) {
            (ValueTy::Int(a), ValueTy::Int(b)) => {
                ValueTy::Int(a.common(b).ok_or(EE::UnsupportedExpr)?)
            }
            (ValueTy::FixedBytes(a), ValueTy::FixedBytes(b)) => ValueTy::FixedBytes(a.max(b)),
            (a, b) if a == b => a,
            (ValueTy::Contract(a), ValueTy::Contract(b)) => {
                if gcx.hir.contract(a).linearized_bases.contains(&b) {
                    ValueTy::Contract(b)
                } else if gcx.hir.contract(b).linearized_bases.contains(&a) {
                    ValueTy::Contract(a)
                } else {
                    return Err(EE::UnsupportedExpr);
                }
            }
            _ => return Err(EE::UnsupportedExpr),
        };
        Ok((a.implicit_to(gcx, common)?, b.implicit_to(gcx, common)?))
    }

    /// Applies the given unary operation to this value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match (self, op) {
            (Self::Int(value), op) => Self::Int(value.unop(op)?),
            (Self::Bool(value), hir::UnOpKind::Not) => Self::Bool(!value),
            (Self::FixedBytes(value), hir::UnOpKind::BitNot) => {
                Self::FixedBytes(FixedBytes::from_word(value.size, !value.word()))
            }
            _ => return Err(EE::UnsupportedUnaryOp),
        })
    }

    /// Applies the given binary operation to this value.
    ///
    /// `&&` and `||` evaluate both operands here; the evaluator short-circuits them first.
    pub fn binop(self, rhs: Self, op: BinOpKind) -> Result<Self, EE> {
        Ok(match (self, rhs) {
            (Self::Int(lhs), Self::Int(rhs)) => match op {
                _ if op.is_cmp() => {
                    // Comparisons need a common type, like arithmetic.
                    IntScalar::binop_ty(&lhs, &rhs, op)?;
                    Self::Bool(compare(lhs.data.cmp(&rhs.data), op))
                }
                BinOpKind::Or | BinOpKind::And => return Err(EE::UnsupportedBinaryOp),
                _ => Self::Int(lhs.binop(rhs, op)?),
            },
            (Self::Bool(lhs), Self::Bool(rhs)) => match op {
                BinOpKind::And => Self::Bool(lhs && rhs),
                BinOpKind::Or => Self::Bool(lhs || rhs),
                BinOpKind::Eq | BinOpKind::Ne => Self::Bool(compare(lhs.cmp(&rhs), op)),
                _ => return Err(EE::UnsupportedBinaryOp),
            },
            (Self::FixedBytes(lhs), Self::Int(rhs))
                if matches!(op, BinOpKind::Shl | BinOpKind::Shr) =>
            {
                Self::FixedBytes(lhs.shift(&rhs, op)?)
            }
            (lhs @ Self::FixedBytes(_), rhs) | (lhs, rhs @ Self::FixedBytes(_)) => {
                let (lhs, rhs) = FixedBytes::unify(lhs, rhs)?;
                if op.is_cmp() {
                    return Ok(Self::Bool(compare(lhs.word().cmp(&rhs.word()), op)));
                }
                let word = match op {
                    BinOpKind::BitAnd => lhs.word() & rhs.word(),
                    BinOpKind::BitOr => lhs.word() | rhs.word(),
                    BinOpKind::BitXor => lhs.word() ^ rhs.word(),
                    _ => return Err(EE::UnsupportedBinaryOp),
                };
                Self::FixedBytes(FixedBytes::from_word(lhs.size, word))
            }
            (Self::Address(lhs), Self::Address(rhs)) if op.is_cmp() => {
                Self::Bool(compare(lhs.cmp(&rhs), op))
            }
            (Self::Enum(lhs_id, lhs), Self::Enum(rhs_id, rhs))
                if lhs_id == rhs_id && op.is_cmp() =>
            {
                Self::Bool(compare(lhs.cmp(&rhs), op))
            }
            _ => return Err(EE::UnsupportedBinaryOp),
        })
    }
}

/// Returns the result of the comparison `op` given how its operands order.
fn compare(ordering: Ordering, op: BinOpKind) -> bool {
    match op {
        BinOpKind::Lt => ordering.is_lt(),
        BinOpKind::Le => ordering.is_le(),
        BinOpKind::Gt => ordering.is_gt(),
        BinOpKind::Ge => ordering.is_ge(),
        BinOpKind::Eq => ordering.is_eq(),
        BinOpKind::Ne => ordering.is_ne(),
        _ => unreachable!("not a comparison: {op:?}"),
    }
}

/// The type of a typed constant value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueTy {
    Int(IntTy),
    Bool,
    /// `bytesN`, with the number of bytes.
    FixedBytes(u8),
    /// `address` and `address payable`.
    Address,
    Contract(hir::ContractId),
    Bytes,
    String,
    Enum(hir::EnumId),
    Udvt(hir::UdvtId),
}

impl ValueTy {
    /// Returns the value type denoted by the given type, if it has constant values.
    pub(super) fn from_hir(ty: &hir::Type<'_>) -> Option<Self> {
        Some(match ty.kind {
            hir::TypeKind::Elementary(ty) => Self::from_elementary(ty)?,
            hir::TypeKind::Custom(hir::ItemId::Contract(id)) => Self::Contract(id),
            hir::TypeKind::Custom(hir::ItemId::Enum(id)) => Self::Enum(id),
            hir::TypeKind::Custom(hir::ItemId::Udvt(id)) => Self::Udvt(id),
            _ => return None,
        })
    }

    /// Returns the value type denoted by the given elementary type, if it has constant values.
    pub(super) fn from_elementary(ty: ElementaryType) -> Option<Self> {
        Some(match ty {
            ElementaryType::Int(size) => Self::Int(IntTy { signed: true, size }),
            ElementaryType::UInt(size) => Self::Int(IntTy { signed: false, size }),
            ElementaryType::Bool => Self::Bool,
            ElementaryType::Address(_) => Self::Address,
            ElementaryType::FixedBytes(size) => Self::FixedBytes(size.bytes()),
            ElementaryType::Bytes => Self::Bytes,
            ElementaryType::String => Self::String,
            ElementaryType::Fixed(..) | ElementaryType::UFixed(..) => return None,
        })
    }
}

/// A `bytesN` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedBytes {
    /// The number of bytes, from 1 to 32.
    size: u8,
    /// The bytes, left-aligned and padded with zeros.
    data: B256,
}

impl FixedBytes {
    /// Creates a `bytesN` value from at most `size` bytes, padding them on the right.
    pub(super) fn new(size: u8, bytes: &[u8]) -> Self {
        debug_assert!((1..=32).contains(&size) && bytes.len() <= usize::from(size));
        let mut data = B256::ZERO;
        data[..bytes.len()].copy_from_slice(bytes);
        Self { size, data }
    }

    /// Creates a `bytesN` value from a left-aligned word, dropping the bytes past `size`.
    fn from_word(size: u8, word: U256) -> Self {
        let mask = U256::MAX << (256 - 8 * usize::from(size));
        Self { size, data: (word & mask).into() }
    }

    /// Creates a `bytesN` value from the low `size` bytes of an unsigned value.
    fn from_uint(size: u8, value: U256) -> Self {
        Self::from_word(size, value << (256 - 8 * usize::from(size)))
    }

    /// Converts an integer literal, like the implicit conversion of a hex number to `bytesN`.
    fn from_literal(size: u8, value: &IntScalar) -> Result<Self, EE> {
        let value = value.as_u256().filter(|value| value.bit_len() <= 8 * usize::from(size));
        Ok(Self::from_uint(size, value.ok_or(EE::OutOfRange)?))
    }

    /// Returns the number of bytes.
    pub fn size(&self) -> u8 {
        self.size
    }

    /// Returns the bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..usize::from(self.size)]
    }

    /// Returns the bytes as a left-aligned EVM word.
    pub fn word(&self) -> U256 {
        self.data.into()
    }

    /// Returns the bytes as an unsigned integer.
    fn to_uint(self) -> U256 {
        self.word() >> (256 - 8 * usize::from(self.size))
    }

    /// Truncates or pads the value on the right to `size` bytes.
    fn resize(self, size: u8) -> Self {
        Self::from_word(size, self.word())
    }

    /// Converts both operands of a `bytesN` operation to their common type.
    fn unify(lhs: ConstValue, rhs: ConstValue) -> Result<(Self, Self), EE> {
        let size = match (&lhs, &rhs) {
            (ConstValue::FixedBytes(lhs), ConstValue::FixedBytes(rhs)) => lhs.size.max(rhs.size),
            (ConstValue::FixedBytes(value), _) | (_, ConstValue::FixedBytes(value)) => value.size,
            _ => unreachable!(),
        };
        let convert = |value| match value {
            ConstValue::FixedBytes(value) if value.size <= size => Ok(value.resize(size)),
            ConstValue::Int(value) if value.ty.is_none() => Self::from_literal(size, &value),
            ConstValue::Bytes(data, BytesKind::Literal)
                if data.as_byte_str().len() <= usize::from(size) =>
            {
                Ok(Self::new(size, data.as_byte_str()))
            }
            _ => Err(EE::UnsupportedBinaryOp),
        };
        Ok((convert(lhs)?, convert(rhs)?))
    }

    /// Shifts the bytes left or right by a non-negative amount of bits.
    fn shift(self, amount: &IntScalar, op: BinOpKind) -> Result<Self, EE> {
        if amount.is_negative() {
            return Err(EE::UnsupportedBinaryOp);
        }
        let amount = amount.as_u256().and_then(|amount| usize::try_from(amount).ok());
        let word = match amount {
            Some(amount) if amount < 256 => match op {
                BinOpKind::Shl => self.word() << amount,
                _ => self.word() >> amount,
            },
            _ => U256::ZERO,
        };
        Ok(Self::from_word(self.size, word))
    }
}

/// The declared integer type of a constant value.
#[derive(Clone, Copy, Eq, Debug)]
pub(super) struct IntTy {
    signed: bool,
    size: TypeSize,
}

/// `int` and `int256` denote the same type with a different size, so compare the bit widths
/// instead of the sizes as written.
impl PartialEq for IntTy {
    fn eq(&self, other: &Self) -> bool {
        self.signed == other.signed && self.bits() == other.bits()
    }
}

impl IntTy {
    /// `uint8`.
    pub(super) const UINT8: Self = Self { signed: false, size: TypeSize::new_int_bits(8) };
    /// `uint256`.
    pub(super) const UINT256: Self = Self { signed: false, size: TypeSize::new_int_bits(256) };

    /// Returns the number of bits of the type.
    fn bits(self) -> u16 {
        self.size.bits()
    }

    /// Returns the smallest value of the type.
    pub(super) fn min(self) -> BigInt {
        if self.signed { -(BigInt::one() << (self.bits() - 1)) } else { BigInt::zero() }
    }

    /// Returns the largest value of the type.
    pub(super) fn max(self) -> BigInt {
        let value_bits = if self.signed { self.bits() - 1 } else { self.bits() };
        (BigInt::one() << value_bits) - 1
    }

    /// Returns whether the given value is representable in this type.
    fn contains(self, value: &BigInt) -> bool {
        let value_bits = if self.signed { self.bits() - 1 } else { self.bits() };
        if value.is_negative() {
            self.signed && -value <= BigInt::one() << value_bits
        } else {
            value.bits() <= value_bits as u64
        }
    }

    /// Returns the widest integer type of the given signedness.
    fn full_width(signed: bool) -> Self {
        Self { signed, size: TypeSize::new_int_bits(256) }
    }

    /// Returns the narrowest integer type of the given signedness holding `bits` value bits.
    ///
    /// Integer types come in whole bytes, so the width is rounded up to the next multiple of
    /// eight; a value needing more than a word has no such type.
    fn narrowest(bits: u64, signed: bool) -> Option<Self> {
        let bits = bits.max(1).div_ceil(8) * 8;
        u16::try_from(bits)
            .ok()
            .and_then(TypeSize::try_new_int_bits)
            .map(|size| Self { signed, size })
    }

    /// Returns whether values of this type implicitly convert to `other`.
    ///
    /// Integer types only convert implicitly to wider types of the same signedness.
    fn converts_to(self, other: Self) -> bool {
        self.signed == other.signed && other.bits() >= self.bits()
    }

    /// Returns the type both `self` and `other` implicitly convert to, if any.
    fn common(self, other: Self) -> Option<Self> {
        if other.converts_to(self) {
            Some(self)
        } else if self.converts_to(other) {
            Some(other)
        } else {
            None
        }
    }
}

/// Represents an integer value for constant evaluation.
#[derive(Clone, Debug)]
pub struct IntScalar {
    data: BigInt,
    /// The type the value was computed in, or `None` for a literal.
    ///
    /// Values built only from literals are unbounded and carry no type; as soon as a typed value
    /// takes part in the expression, arithmetic is checked against that type instead of yielding
    /// the mathematical result.
    ty: Option<IntTy>,
}

impl IntScalar {
    /// Creates a new non-negative integer literal.
    pub fn new(data: U256) -> Self {
        Self { data: Self::bigint_from_u256(data), ty: None }
    }

    /// Creates a new `uint256` value.
    pub(super) fn uint256(data: U256) -> Self {
        Self::typed(Self::bigint_from_u256(data), IntTy::UINT256)
    }

    /// Creates a new integer value of the given type, which must hold it.
    pub(super) fn typed(data: BigInt, ty: IntTy) -> Self {
        debug_assert!(ty.contains(&data));
        Self { data, ty: Some(ty) }
    }

    /// Returns the bit length of the integer value.
    ///
    /// This is the number of bits needed for the literal type.
    pub fn bit_len(&self) -> u64 {
        Self::bits(&self.data)
    }

    /// Returns whether the value is negative.
    pub fn is_negative(&self) -> bool {
        self.data.is_negative()
    }

    /// Returns whether the value is an integer literal rather than a value of an integer type.
    pub fn is_literal(&self) -> bool {
        self.ty.is_none()
    }

    /// Returns whether the integer value is zero.
    pub fn is_zero(&self) -> bool {
        self.data.is_zero()
    }

    /// Returns the non-negative integer value as unsigned data.
    pub fn as_u256(&self) -> Option<U256> {
        if self.is_negative() { None } else { Self::magnitude_u256(&self.data) }
    }

    /// Returns the magnitude of `data` if it fits in 256 bits.
    fn magnitude_u256(data: &BigInt) -> Option<U256> {
        let mut limbs = [0; 4];
        for (limb, digit) in std::iter::zip(&mut limbs, data.magnitude().iter_u64_digits()) {
            *limb = digit;
        }
        (data.magnitude().bits() <= 256).then(|| U256::from_limbs(limbs))
    }

    /// Returns the 256-bit two's-complement EVM word for this integer value, or `None` if the
    /// value does not fit in 256 bits.
    pub fn as_evm_word(&self) -> Option<U256> {
        if self.bit_len() > 256 {
            return None;
        }
        if let Some(value) = self.as_u256() {
            return Some(value);
        }
        Some(U256::ZERO.wrapping_sub(Self::magnitude_u256(&self.data)?))
    }

    /// Returns the bytes of the value in its type, `bytes` wide and in two's complement, for
    /// packed ABI encoding.
    pub(super) fn to_be_bytes(&self) -> Option<Vec<u8>> {
        let bytes = usize::from(self.ty?.size.bytes());
        let word = self.as_evm_word()?.to_be_bytes::<32>();
        Some(word[32 - bytes..].to_vec())
    }

    fn bigint_from_u256(data: U256) -> BigInt {
        BigInt::from_bytes_be(Sign::Plus, &data.to_be_bytes::<32>())
    }

    fn checked(data: BigInt) -> Result<Self, EE> {
        // Like solc, allow a most significant bit up to index `MAX_INTERMEDIATE_BITS`.
        if data.magnitude().bits() > MAX_INTERMEDIATE_BITS + 1 {
            return Err(EE::ArithmeticOverflow);
        }
        Ok(Self { data, ty: None })
    }

    /// Converts the value implicitly to `ty`: a literal must fit in it, and a typed value must
    /// have a narrower type of the same signedness.
    fn implicit_to(self, ty: IntTy) -> Result<Self, EE> {
        match self.ty {
            None if ty.contains(&self.data) => Ok(Self { ty: Some(ty), ..self }),
            None => Err(EE::OutOfRange),
            Some(from) if from.converts_to(ty) => Ok(Self { ty: Some(ty), ..self }),
            Some(_) => Err(EE::UnsupportedExpr),
        }
    }

    /// Converts the value explicitly to `ty`: a literal must fit in it, and a typed value keeps
    /// its low bits in two's complement.
    fn explicit_to(self, ty: IntTy) -> Result<Self, EE> {
        if self.ty.is_none() {
            return self.implicit_to(ty);
        }
        let modulus = BigInt::one() << ty.bits();
        let mut data = ((self.data % &modulus) + &modulus) % &modulus;
        if ty.signed && data > ty.max() {
            data -= modulus;
        }
        Ok(Self::typed(data, ty))
    }

    /// Sets the type the value was computed in, rejecting values outside of its range.
    fn retype(mut self, ty: Option<IntTy>) -> Result<Self, EE> {
        if let Some(ty) = ty
            && !ty.contains(&self.data)
        {
            return Err(EE::ArithmeticOverflow);
        }
        self.ty = ty;
        Ok(self)
    }

    fn bits(data: &BigInt) -> u64 {
        if data.is_zero() {
            return 1;
        }
        if data.is_positive() {
            return data.bits();
        }
        let abs = data.magnitude();
        // Signed N-bit two's-complement values cover [-2^(N - 1), 2^(N - 1) - 1].
        // Negative powers of two therefore fit in one fewer value bit than other negatives.
        if Self::is_power_of_two(abs) { abs.bits() } else { abs.bits() + 1 }
    }

    fn is_power_of_two(value: &BigUint) -> bool {
        !value.is_zero() && (value & (value - BigUint::one())).is_zero()
    }

    fn negate(self) -> Result<Self, EE> {
        Self::checked(-self.data)
    }

    fn shift_amount(r: Self) -> Option<usize> {
        r.as_u256()?.try_into().ok()
    }

    fn bitop(self, r: Self, f: impl FnOnce(BigInt, BigInt) -> BigInt) -> Result<Self, EE> {
        Self::checked(f(self.data, r.data))
    }

    /// Applies the given unary operation to this value.
    ///
    /// The operation is performed in the operand's type, so a result outside of that type's range
    /// is an error rather than the mathematical value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        let ty = self.ty;
        let value = match op {
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec
            | hir::UnOpKind::Not => return Err(EE::UnsupportedUnaryOp),
            hir::UnOpKind::BitNot => Self::checked(!self.data)?,
            // Negating an unsigned value is not arithmetic that overflows but an operator the
            // operand's type does not have, which is what solc reports for it.
            hir::UnOpKind::Neg if ty.is_some_and(|ty| !ty.signed) => {
                return Err(EE::NegateUnsigned);
            }
            hir::UnOpKind::Neg => self.negate()?,
        };
        value.retype(ty).map_err(|_| EE::UnaryOverflow)
    }

    /// Returns the mobile type of a literal operand.
    ///
    /// Next to a typed operand, a literal is no longer exact: the type checker gives it its
    /// mobile type, the narrowest integer type of its sign that holds it. A literal
    /// too large for a word has no mobile type at all, and the operator does not apply to it,
    /// which is what solc reports as "Literal too large".
    fn mobile_ty(&self) -> Result<IntTy, EE> {
        IntTy::narrowest(Self::bits(&self.data), self.is_negative()).ok_or(EE::LiteralTooLarge)
    }

    /// Returns the type a literal operand and a typed operand are computed in, if any.
    ///
    /// The literal takes its mobile type, and the operation is performed in the common type of
    /// that and the typed operand: the mobile type when the typed operand converts to it, and
    /// the typed operand's own type when the literal fits in it instead. Neither holds only when
    /// the two have different signedness, and the result stays untyped because the type checker
    /// already rejects such operands.
    fn literal_common_ty(literal: &Self, typed: IntTy) -> Result<Option<IntTy>, EE> {
        let mobile = literal.mobile_ty()?;
        Ok(if typed.converts_to(mobile) {
            Some(mobile)
        } else {
            typed.contains(&literal.data).then_some(typed)
        })
    }

    /// Returns the type the given binary operation is performed in, if any.
    ///
    /// Shifts and exponentiation are performed in the left operand's type, every other operation
    /// in the common type of both operands. Two literals stay untyped and keep their exact value up
    /// to `MAX_INTERMEDIATE_BITS`, and operands without a common type stay untyped because the
    /// type checker already rejects them. A shift or exponentiation
    /// of two literals must also pass [`Self::check_literal_bounds`].
    ///
    /// A literal paired with a typed operand must first have a mobile type, and the operation is
    /// rejected when it does not. This check comes before the operation because folding retypes
    /// only the result: `(1 << 256) >> ONE` with a typed `ONE` would otherwise shift the literal
    /// back into range and be accepted, where solc rejects the operands and the runtime
    /// expression yields `0`.
    ///
    /// A shift or exponentiation whose left operand is a literal is the further exception: it is
    /// always performed in `uint256`, or `int256` for a negative literal, rather than at full
    /// precision. Keeping it unbounded would fold `(1 << SHIFT) >> SHIFT` to `1` where the EVM
    /// shifts the bit out and yields `0`, and would let an exponentiation that reverts with
    /// `Panic(0x11)` at runtime evaluate to a value wider than a word.
    fn binop_ty(l: &Self, r: &Self, op: BinOpKind) -> Result<Option<IntTy>, EE> {
        Ok(match op {
            BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar | BinOpKind::Pow => match (l.ty, r.ty)
            {
                (Some(ty), Some(_)) => Some(ty),
                (Some(ty), None) => {
                    r.mobile_ty()?;
                    Some(ty)
                }
                (None, Some(_)) => {
                    l.mobile_ty()?;
                    Some(IntTy::full_width(l.is_negative()))
                }
                (None, None) => {
                    l.check_literal_bounds(r, op)?;
                    None
                }
            },
            _ => match (l.ty, r.ty) {
                (None, None) => None,
                (Some(ty), None) => Self::literal_common_ty(r, ty)?,
                (None, Some(ty)) => Self::literal_common_ty(l, ty)?,
                (Some(l), Some(r)) => l.common(r),
            },
        })
    }

    /// Rejects a literal shift or exponentiation that solc's
    /// `ConstantEvaluator::evaluateBinaryOperator` gives no value.
    ///
    /// The shift amount or exponent must fit in 32 bits, and a left shift or power must fit in
    /// `MAX_INTERMEDIATE_BITS`.
    fn check_literal_bounds(&self, r: &Self, op: BinOpKind) -> Result<(), EE> {
        match op {
            BinOpKind::Shl | BinOpKind::Shr if r.is_negative() => return Err(EE::NegativeShift),
            BinOpKind::Shl | BinOpKind::Shr => {
                let amount = u32::try_from(&r.data).map_err(|_| EE::ShiftTooLarge)?;
                if op == BinOpKind::Shl
                    && !self.is_zero()
                    && self.data.bits() + u64::from(amount) > MAX_INTERMEDIATE_BITS
                {
                    return Err(EE::ShiftTooLarge);
                }
            }
            // Powers of `0`, `1` and `-1` are exact for any exponent.
            BinOpKind::Pow if !self.is_zero() && !self.data.magnitude().is_one() => {
                let exp = u32::try_from(r.data.magnitude()).map_err(|_| EE::ExponentTooLarge)?;
                if self.data.bits() * u64::from(exp) > MAX_INTERMEDIATE_BITS {
                    return Err(EE::ExponentTooLarge);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns whether solc's constant evaluator gives the binary operation the same value, or
    /// reports the same error.
    ///
    /// solc gives no value to a shift by more than `u32::MAX` bits, and converts the shift amount
    /// or exponent of a typed operand to its type first.
    pub(super) fn solc_folds_binop(&self, r: &Self, op: BinOpKind) -> bool {
        let fits_u32 = u32::try_from(&r.data).is_ok();
        match (op, self.ty, r.ty) {
            // solc reports the failed operations of two literals.
            (_, None, None) => true,
            // solc converts the right operand of a typed shift or power to the left type.
            (BinOpKind::Pow, Some(ty), _) => ty.contains(&r.data),
            (BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar, Some(ty), _) => {
                fits_u32 && ty.contains(&r.data)
            }
            (BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar, None, Some(_)) => fits_u32,
            _ => true,
        }
    }

    /// Applies the given binary operation to this value.
    ///
    /// For literal arithmetic, this preserves the exact mathematical value. Typed arithmetic stays
    /// checked: a result outside of the operation's type is an error, like it is at runtime.
    pub fn binop(self, r: Self, op: BinOpKind) -> Result<Self, EE> {
        let ty = Self::binop_ty(&self, &r, op)?;
        let literal = ty.is_none() && self.ty.is_none() && r.ty.is_none();
        match self.binop_value(r, op, literal) {
            // solc gives literal arithmetic beyond its precision no type.
            Err(EE::ArithmeticOverflow) if literal => Err(EE::LiteralPrecision),
            value => value?.retype(ty),
        }
    }

    fn binop_value(self, r: Self, op: BinOpKind, literal: bool) -> Result<Self, EE> {
        Ok(match op {
            BinOpKind::Add => Self::checked(self.data + r.data)?,
            BinOpKind::Sub => Self::checked(self.data - r.data)?,
            BinOpKind::Mul => Self::checked(self.data * r.data)?,
            BinOpKind::Div => {
                if r.data.is_zero() {
                    return Err(EE::DivisionByZero { literal });
                }
                // A literal quotient is a rational number, which we do not model.
                if literal && !(&self.data % &r.data).is_zero() {
                    return Err(EE::Fractional);
                }
                Self::checked(self.data / r.data)?
            }
            BinOpKind::Rem => {
                if r.data.is_zero() {
                    return Err(EE::DivisionByZero { literal });
                }
                Self::checked(self.data % r.data)?
            }
            BinOpKind::Pow => {
                if r.is_negative() && !literal {
                    return Err(EE::ArithmeticOverflow);
                }
                self.checked_pow(r)?
            }
            BinOpKind::BitOr => self.bitop(r, |a, b| a | b)?,
            BinOpKind::BitAnd => self.bitop(r, |a, b| a & b)?,
            BinOpKind::BitXor => self.bitop(r, |a, b| a ^ b)?,
            BinOpKind::Shr => match Self::shift_amount(r) {
                Some(r) => Self::checked(self.data >> r)?,
                // Shifting by at least the width of any value leaves its sign.
                None if self.is_negative() => Self::checked(-BigInt::one())?,
                None => Self::checked(BigInt::zero())?,
            },
            BinOpKind::Shl => self.checked_shl(r)?,
            BinOpKind::Sar => return Err(EE::UnsupportedBinaryOp),
            BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge
            | BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Or
            | BinOpKind::And => return Err(EE::UnsupportedBinaryOp),
        })
    }

    fn checked_shl(self, r: Self) -> Result<Self, EE> {
        if self.data.is_zero() {
            return Ok(self);
        }
        let shift = u64::try_from(r.as_u256().ok_or(EE::ArithmeticOverflow)?)
            .map_err(|_| EE::ArithmeticOverflow)?;
        let bits = self.data.magnitude().bits();
        if shift > MAX_INTERMEDIATE_BITS.saturating_sub(bits) {
            return Err(EE::ArithmeticOverflow);
        }
        Self::checked(self.data << usize::try_from(shift).map_err(|_| EE::ArithmeticOverflow)?)
    }

    fn checked_pow(self, r: Self) -> Result<Self, EE> {
        if r.data.is_zero() {
            return Self::checked(BigInt::one());
        }
        if self.data.is_zero() {
            return Ok(self);
        }
        if self.data.is_one() {
            return Ok(self);
        }
        if self.data == BigInt::from(-1) {
            let is_odd = r.data.magnitude().bit(0);
            return Self::checked(if is_odd { self.data } else { BigInt::one() });
        }
        // Like solc, a negative literal exponent keeps `0`, `1` and `-1` above; other bases give
        // a fraction.
        if r.is_negative() {
            return Err(EE::Fractional);
        }
        let exp = r.as_u256().ok_or(EE::ArithmeticOverflow)?;
        let exp = u32::try_from(exp).map_err(|_| EE::ArithmeticOverflow)?;
        // Bound the result size before computing the power.
        if self.data.magnitude().bits().saturating_mul(exp.into()) > MAX_INTERMEDIATE_BITS {
            return Err(EE::ArithmeticOverflow);
        }
        Self::checked(self.data.pow(exp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::b256;

    fn int(signed: bool, bits: u16) -> IntTy {
        IntTy { signed, size: TypeSize::new_int_bits(bits) }
    }

    fn typed(value: i64, ty: IntTy) -> IntScalar {
        IntScalar::typed(BigInt::from(value), ty)
    }

    #[test]
    fn const_value_integer_accessors() {
        let zero = ConstValue::Int(IntScalar::new(U256::ZERO));
        assert_eq!(zero.as_u256(), Some(U256::ZERO));
        assert_eq!(zero.as_bool(), None);
        assert!(zero.is_zero());

        let one = ConstValue::Int(IntScalar::new(U256::from(1)));
        assert_eq!(one.as_u256(), Some(U256::from(1)));
        assert!(!one.is_zero());

        let negative =
            ConstValue::Int(IntScalar::new(U256::from(1)).unop(hir::UnOpKind::Neg).unwrap());
        assert_eq!(negative.as_u256(), None);
        assert_eq!(negative.as_word(), Some(U256::MAX));
        assert!(!negative.is_zero());
    }

    #[test]
    fn const_value_bool_accessors_preserve_type() {
        let value = ConstValue::Bool(false);
        assert_eq!(value.as_bool(), Some(false));
        assert_eq!(value.as_u256(), None);
        assert_eq!(value.as_word(), Some(U256::ZERO));
        assert!(!value.is_zero());
    }

    #[test]
    fn int_ty_contains_range_boundaries() {
        assert!(int(true, 8).contains(&BigInt::from(127)));
        assert!(!int(true, 8).contains(&BigInt::from(128)));
        assert!(int(true, 8).contains(&BigInt::from(-128)));
        assert!(!int(true, 8).contains(&BigInt::from(-129)));
        assert!(int(false, 8).contains(&BigInt::from(255)));
        assert!(!int(false, 8).contains(&BigInt::from(256)));
        assert!(!int(false, 8).contains(&BigInt::from(-1)));
        assert_eq!(int(true, 8).min(), BigInt::from(-128));
        assert_eq!(int(true, 8).max(), BigInt::from(127));
        assert_eq!(int(false, 8).max(), BigInt::from(255));
    }

    #[test]
    fn int_ty_common_widens_only_within_signedness() {
        assert_eq!(int(true, 8).common(int(true, 16)), Some(int(true, 16)));
        assert_eq!(int(true, 16).common(int(true, 8)), Some(int(true, 16)));
        assert_eq!(int(false, 8).common(int(false, 8)), Some(int(false, 8)));
        assert_eq!(int(true, 8).common(int(false, 16)), None);
        assert_eq!(int(false, 8).common(int(true, 16)), None);
    }

    #[test]
    fn int_ty_eq_ignores_how_the_size_is_written() {
        let plain = IntTy { signed: true, size: TypeSize::ZERO };
        assert_eq!(plain.bits(), int(true, 256).bits());
        assert_eq!(plain, int(true, 256));
        assert_eq!(plain.common(int(true, 256)), Some(int(true, 256)));
        assert_ne!(plain, int(false, 256));
        assert_ne!(plain, int(true, 128));
    }

    #[test]
    fn explicit_int_conversions_wrap() {
        let value = typed(-1, int(true, 8)).explicit_to(int(false, 8)).unwrap();
        assert_eq!(value.as_u256(), Some(U256::from(255)));
        let value = typed(300, int(false, 16)).explicit_to(int(false, 8)).unwrap();
        assert_eq!(value.as_u256(), Some(U256::from(44)));
        let value = typed(200, int(false, 8)).explicit_to(int(true, 8)).unwrap();
        assert_eq!(value.data, BigInt::from(-56));
        assert!(IntScalar::new(U256::from(300)).explicit_to(int(false, 8)).is_err());
    }

    #[test]
    fn fixed_bytes_shifts_drop_bits() {
        let value = FixedBytes::new(4, &[0x12, 0x34, 0x56, 0x78]);
        let eight = IntScalar::new(U256::from(8));
        let value = value.shift(&eight, BinOpKind::Shl).unwrap();
        assert_eq!(value.as_slice(), [0x34, 0x56, 0x78, 0x00]);
        let value = value.shift(&eight, BinOpKind::Shr).unwrap();
        assert_eq!(value.as_slice(), [0x00, 0x34, 0x56, 0x78]);
        assert_eq!(
            FixedBytes::new(1, &[0xff]).word(),
            U256::from_be_bytes(
                b256!("ff00000000000000000000000000000000000000000000000000000000000000").0
            )
        );
    }
}
