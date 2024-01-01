use super::{Expr, ParameterList, Path, StateMutability, Visibility};
use sulk_interface::{kw, Ident, Span, Symbol};

/// A type name.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.typeName>
#[derive(Clone, Debug)]
pub struct Ty {
    pub span: Span,
    pub kind: TyKind,
}

/// The kind of a type.
#[derive(Clone, Debug)]
pub enum TyKind {
    // `elementary-type-name`: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.elementaryTypeName>
    /// `address $(payable)?`
    Address(/* payable: */ bool),
    /// `bool`
    Bool,
    /// `string`
    String,
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
    /// `size @ 1..=32 => bytes{size}`
    /// `0 | 33.. => unreachable!()`
    FixedBytes(TySize),

    /// `$element[$($size)?]`
    Array(Box<TypeArray>),
    /// `function($($parameters),*) $($attributes)* $(returns ($($returns),+))?`
    Function(Box<TypeFunction>),
    /// `mapping($key $($key_name)? => $value $($value_name)?)`
    Mapping(Box<TypeMapping>),

    /// A custom type.
    Custom(Path),
}

/// Byte size of a fixed-bytes, integer, or fixed-point number (M) type. Valid values: 0..=32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TySize(u8);

impl TySize {
    /// The value zero. Note that this is not a valid size for a fixed-bytes type.
    pub const ZERO: Self = Self(0);

    /// The maximum value of a `TySize`.
    pub const MAX: u8 = 32;

    /// Creates a new `TySize` from a `u8`.
    pub const fn new(value: u8) -> Option<Self> {
        if value > Self::MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the value.
    pub const fn get(self) -> u8 {
        self.0
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TyFixedSize(u8);

impl TyFixedSize {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// The maximum value of a `TyFixedSize`.
    pub const MAX: u8 = 80;

    /// Creates a new `TyFixedSize` from a `u8`.
    pub const fn new(value: u8) -> Option<Self> {
        if value > Self::MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the value.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// An array type.
#[derive(Clone, Debug)]
pub struct TypeArray {
    pub element: Ty,
    pub size: Option<Expr>,
}

/// A function type name.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionTypeName>
#[derive(Clone, Debug)]
pub struct TypeFunction {
    pub parameters: ParameterList,
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub returns: ParameterList,
}

/// A mapping type.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.mappingType>
#[derive(Clone, Debug)]
pub struct TypeMapping {
    pub key: Ty,
    pub key_name: Option<Ident>,
    pub value: Ty,
    pub value_name: Option<Ident>,
}
