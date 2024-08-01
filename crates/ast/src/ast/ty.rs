use super::{AstPath, Box, Expr, ParameterList, StateMutability, Visibility};
use std::fmt;
use sulk_interface::{kw, Ident, Span, Symbol};

/// A type name.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.typeName>
#[derive(Debug)]
pub struct Type<'ast> {
    pub span: Span,
    pub kind: TypeKind<'ast>,
}

impl Type<'_> {
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
pub enum TypeKind<'ast> {
    /// An elementary/primitive type.
    Elementary(ElementaryType),

    /// `$element[$($size)?]`
    Array(Box<'ast, TypeArray<'ast>>),
    /// `function($($parameters),*) $($attributes)* $(returns ($($returns),+))?`
    Function(Box<'ast, TypeFunction<'ast>>),
    /// `mapping($key $($key_name)? => $value $($value_name)?)`
    Mapping(Box<'ast, TypeMapping<'ast>>),

    /// A custom type.
    Custom(AstPath<'ast>),
}

impl<'ast> TypeKind<'ast> {
    /// Returns `true` if the type is an elementary type.
    ///
    /// Note that this does not include `Custom` types.
    pub fn is_elementary(&self) -> bool {
        matches!(self, Self::Elementary(_))
    }

    /// Returns `true` if the type is a custom type.
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

/// Elementary/primitive type.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.elementaryTypeName>
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementaryType {
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
    Fixed(TypeSize, TypeFixedSize),
    /// Unsigned fixed-point number.
    /// `ufixedMxN where M @ 0..=32, N @ 0..=80`. M is the number of bytes, **not bits**.
    UFixed(TypeSize, TypeFixedSize),

    /// Signed integer. The number is the number of bytes, **not bits**.
    /// `0 => int`
    /// `size @ 1..=32 => int{size*8}`
    /// `33.. => unreachable!()`
    Int(TypeSize),
    /// Unsigned integer. The number is the number of bytes, **not bits**.
    /// `0 => uint`
    /// `size @ 1..=32 => uint{size*8}`
    /// `33.. => unreachable!()`
    UInt(TypeSize),
    /// Fixed-size byte array.
    /// `size @ 1..=32 => bytes{size}`
    /// `0 | 33.. => unreachable!()`
    FixedBytes(TypeSize),
}

/// Byte size of a fixed-bytes, integer, or fixed-point number (M) type. Valid values: 0..=32.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeSize(u8);

impl fmt::Debug for TypeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeSize({})", self.0)
    }
}

impl TypeSize {
    /// The value zero. Note that this is not a valid size for a fixed-bytes type.
    pub const ZERO: Self = Self(0);

    /// The maximum byte value of a `TypeSize`.
    pub const MAX: u8 = 32;

    /// Creates a new `TypeSize` from a `u8` number of **bytes**.
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
    pub const fn bits(self) -> u16 {
        self.0 as u16 * 8
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
pub struct TypeFixedSize(u8);

impl fmt::Debug for TypeFixedSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeFixedSize({})", self.0)
    }
}

impl TypeFixedSize {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// The maximum value of a `TypeFixedSize`.
    pub const MAX: u8 = 80;

    /// Creates a new `TypeFixedSize` from a `u8`.
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
    pub element: Type<'ast>,
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
    pub key: Type<'ast>,
    pub key_name: Option<Ident>,
    pub value: Type<'ast>,
    pub value_name: Option<Ident>,
}
