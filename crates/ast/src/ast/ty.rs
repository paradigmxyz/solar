use super::{AstPath, Box, Expr, ParameterList, StateMutability, Visibility};
use solar_interface::{kw, Ident, Span, Symbol};
use std::{borrow::Cow, fmt};

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

    /// Returns `true` if the type is a function.
    #[inline]
    pub fn is_function(&self) -> bool {
        matches!(self.kind, TypeKind::Function(_))
    }
}

/// The kind of a type.
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

impl fmt::Debug for TypeKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elementary(ty) => ty.fmt(f),
            Self::Array(ty) => ty.fmt(f),
            Self::Function(ty) => ty.fmt(f),
            Self::Mapping(ty) => ty.fmt(f),
            Self::Custom(path) => write!(f, "Custom({path:?})"),
        }
    }
}

impl TypeKind<'_> {
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

impl fmt::Debug for ElementaryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(false) => f.write_str("Address"),
            Self::Address(true) => f.write_str("AddressPayable"),
            Self::Bool => f.write_str("Bool"),
            Self::String => f.write_str("String"),
            Self::Bytes => f.write_str("Bytes"),
            Self::Fixed(size, fixed) => write!(f, "Fixed({}, {})", size.bytes_raw(), fixed.get()),
            Self::UFixed(size, fixed) => write!(f, "UFixed({}, {})", size.bytes_raw(), fixed.get()),
            Self::Int(size) => write!(f, "Int({})", size.bits_raw()),
            Self::UInt(size) => write!(f, "UInt({})", size.bits_raw()),
            Self::FixedBytes(size) => write!(f, "FixedBytes({})", size.bytes_raw()),
        }
    }
}

impl ElementaryType {
    /// Returns the Solidity ABI representation of the type as a string.
    pub fn to_abi_str(self) -> Cow<'static, str> {
        match self {
            Self::Address(_) => "address".into(),
            Self::Bool => "bool".into(),
            Self::String => "string".into(),
            Self::Bytes => "bytes".into(),
            Self::Fixed(_size, _fixed) => "fixed".into(),
            Self::UFixed(_size, _fixed) => "ufixed".into(),
            Self::Int(size) => format!("int{}", size.bits()).into(),
            Self::UInt(size) => format!("uint{}", size.bits()).into(),
            Self::FixedBytes(size) => format!("bytes{}", size.bytes()).into(),
        }
    }

    /// Writes the Solidity ABI representation of the type to a formatter.
    pub fn write_abi_str(self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(match self {
            Self::Address(_) => "address",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Bytes => "bytes",
            Self::Fixed(_size, _fixed) => "fixed",
            Self::UFixed(_size, _fixed) => "ufixed",
            Self::Int(size) => return write!(f, "int{}", size.bits()),
            Self::UInt(size) => return write!(f, "uint{}", size.bits()),
            Self::FixedBytes(size) => return write!(f, "bytes{}", size.bytes()),
        })
    }

    /// Returns `true` if the type is a value type.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/types.html#value-types>
    #[inline]
    pub const fn is_value_type(self) -> bool {
        matches!(
            self,
            Self::Address(_)
                | Self::Bool
                | Self::Fixed(..)
                | Self::UFixed(..)
                | Self::Int(..)
                | Self::UInt(..)
                | Self::FixedBytes(..)
        )
    }
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

    /// Creates a new `TypeSize` from a `u8` number of **bits**.
    ///
    /// Panics if `bits` is not a multiple of 8 or greater than 256.
    #[inline]
    #[track_caller]
    pub fn new_int_bits(bits: u16) -> Self {
        Self::try_new_int_bits(bits).unwrap_or_else(|| panic!("invalid integer size: {bits}"))
    }

    /// Creates a new `TypeSize` for an integer type.
    ///
    /// Returns None if `bits` is not a multiple of 8 or greater than 256.
    #[inline]
    pub fn try_new_int_bits(bits: u16) -> Option<Self> {
        if bits % 8 == 0 {
            Self::new((bits / 8).try_into().ok()?)
        } else {
            None
        }
    }

    /// Creates a new `TypeSize` for a fixed-bytes type.
    ///
    /// Panics if `bytes` is not in the range 1..=32.
    #[inline]
    #[track_caller]
    pub fn new_fb_bytes(bytes: u8) -> Self {
        Self::try_new_fb_bytes(bytes).unwrap_or_else(|| panic!("invalid fixed-bytes size: {bytes}"))
    }

    /// Creates a new `TypeSize` for a fixed-bytes type.
    ///
    /// Returns None if `bytes` is not in the range 1..=32.
    #[inline]
    pub fn try_new_fb_bytes(bytes: u8) -> Option<Self> {
        if bytes == 0 {
            return None;
        }
        Self::new(bytes)
    }

    /// Returns the number of **bytes**, with `0` defaulting to `MAX`.
    #[inline]
    pub const fn bytes(self) -> u8 {
        if self.0 == 0 {
            Self::MAX
        } else {
            self.0
        }
    }

    /// Returns the number of **bytes**.
    #[inline]
    pub const fn bytes_raw(self) -> u8 {
        self.0
    }

    /// Returns the number of **bits**, with `0` defaulting to `MAX*8`.
    #[inline]
    pub const fn bits(self) -> u16 {
        self.bytes() as u16 * 8
    }

    /// Returns the number of **bits**.
    #[inline]
    pub const fn bits_raw(self) -> u16 {
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
    pub state_mutability: StateMutability,
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
