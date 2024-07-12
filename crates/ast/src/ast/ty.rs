use super::{Expr, FunctionHeader, ParameterList, Path, StateMutability, Visibility};
use bumpalo::{boxed::Box, collections::Vec};
use std::fmt;
use sulk_interface::{kw, Ident, Span, Symbol};

/// A type name.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.typeName>
#[derive(Debug)]
pub struct Ty<'ast> {
    pub span: Span,
    pub kind: TyKind<'ast>,
}

impl Ty<'_> {
    /// Returns `true` if the type is an elementary type.
    #[inline]
    pub fn is_elementary(&self) -> bool {
        self.kind.is_elementary()
    }

    /// Returns `true` if the type is a custom type.
    #[inline]
    pub fn is_custom(&self) -> bool {
        self.kind.is_custom()
    }
}

/// The kind of a type.
#[derive(Debug)]
pub enum TyKind<'ast> {
    // `elementary-type-name`: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.elementaryTypeName>
    /// Ethereum address, 20-byte fixed-size byte array.
    /// `address $(payable)?`
    Address(/* payable: */ bool),
    /// Boolean.
    /// `bool`
    Bool,
    /// UTF-8 string.
    /// `string`
    String,
    /// Dynamic byte array.
    /// `bytes`
    Bytes,
    /// Signed fixed-point number.
    /// `fixedMxN where M @ 0..=32, N @ 0..=80`. M is the number of bytes, **not bits**.
    Fixed(TySize, TyFixedSize),
    /// Unsigned fixed-point number.
    /// `ufixedMxN where M @ 0..=32, N @ 0..=80`. M is the number of bytes, **not bits**.
    UFixed(TySize, TyFixedSize),
    /// Signed integer. The number is the number of bytes, **not bits**.
    /// `0 => int`
    /// `size @ 1..=32 => int{size*8}`
    /// `33.. => unreachable!()`
    Int(TySize),
    /// Unsigned integer. The number is the number of bytes, **not bits**.
    /// `0 => uint`
    /// `size @ 1..=32 => uint{size*8}`
    /// `33.. => unreachable!()`
    UInt(TySize),
    /// Fixed-size byte array.
    /// `size @ 1..=32 => bytes{size}`
    /// `0 | 33.. => unreachable!()`
    FixedBytes(TySize),

    /// `$element[$($size)?]`
    Array(Box<'ast, TypeArray<'ast>>),
    /// `function($($parameters),*) $($attributes)* $(returns ($($returns),+))?`
    Function(Box<'ast, FunctionHeader<'ast>>),
    /// `mapping($key $($key_name)? => $value $($value_name)?)`
    Mapping(Box<'ast, TypeMapping<'ast>>),

    /// A custom type.
    Custom(Path),
}

impl<'ast> TyKind<'ast> {
    /// Returns `true` if the type is an elementary type.
    ///
    /// Note that this does not include `Custom` types.
    pub fn is_elementary(&self) -> bool {
        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.elementaryTypeName
        matches!(
            self,
            Self::Address(_)
                | Self::Bool
                | Self::String
                | Self::Bytes
                | Self::Int(..)
                | Self::UInt(..)
                | Self::FixedBytes(..)
                | Self::Fixed(..)
                | Self::UFixed(..)
        )
    }

    /// Returns `true` if the type is a custom type.
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(..))
    }
}

/// Byte size of a fixed-bytes, integer, or fixed-point number (M) type. Valid values: 0..=32.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TySize(u8);

impl fmt::Debug for TySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TySize({})", self.0)
    }
}

impl TySize {
    /// The value zero. Note that this is not a valid size for a fixed-bytes type.
    pub const ZERO: Self = Self(0);

    /// The maximum byte value of a `TySize`.
    pub const MAX: u8 = 32;

    /// Creates a new `TySize` from a `u8` number of **bytes**.
    #[inline]
    pub const fn new(bytes: u8) -> Option<Self> {
        if bytes > Self::MAX {
            None
        } else {
            Some(Self(bytes))
        }
    }

    /// Returns the number of **bytes**.
    #[inline]
    pub const fn bytes(self) -> u8 {
        self.0
    }

    /// Returns the number of **bits**.
    #[inline]
    pub const fn bits(self) -> u8 {
        self.0 * 8
    }

    /// Returns the `int` symbol for the type name.
    #[inline]
    pub const fn int_keyword(self) -> Symbol {
        kw::int(self.0)
    }

    /// Returns the `uint` symbol for the type name.
    #[inline]
    pub const fn uint_keyword(self) -> Symbol {
        kw::uint(self.0)
    }

    /// Returns the `bytesN` symbol for the type name.
    ///
    /// # Panics
    ///
    /// Panics if `self` is 0.
    #[inline]
    #[track_caller]
    pub const fn bytes_keyword(self) -> Symbol {
        kw::fixed_bytes(self.0)
    }
}

/// Size of a fixed-point number (N) type. Valid values: 0..=80.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TyFixedSize(u8);

impl fmt::Debug for TyFixedSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TyFixedSize({})", self.0)
    }
}

impl TyFixedSize {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// The maximum value of a `TyFixedSize`.
    pub const MAX: u8 = 80;

    /// Creates a new `TyFixedSize` from a `u8`.
    #[inline]
    pub const fn new(value: u8) -> Option<Self> {
        if value > Self::MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the value.
    #[inline]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// An array type.
#[derive(Debug)]
pub struct TypeArray<'ast> {
    pub element: Ty<'ast>,
    pub size: Option<Box<'ast, Expr<'ast>>>,
}

/// A function type name.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionTypeName>
#[derive(Debug)]
pub struct TypeFunction<'ast> {
    pub parameters: ParameterList<'ast>,
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub returns: ParameterList<'ast>,
}

/// A mapping type.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.mappingType>
#[derive(Debug)]
pub struct TypeMapping<'ast> {
    pub key: Ty<'ast>,
    pub key_name: Option<Ident>,
    pub value: Ty<'ast>,
    pub value_name: Option<Ident>,
}
