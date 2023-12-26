use super::{Expr, Path, StateMutability, Visibility};
use sulk_interface::{Ident, Span};

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
    /// `fixed`
    Fixed,
    /// `ufixed`
    Ufixed,
    /// Signed integer. The number is the number of bytes, **not bits**.
    /// `0 => int`
    /// `size @ 1..=32 => int{size*8}`
    /// `33.. => unreachable!()`
    Int(u8),
    /// Unsigned integer. The number is the number of bytes, **not bits**.
    /// `0 => uint`
    /// `size @ 1..=32 => uint{size*8}`
    /// `33.. => unreachable!()`
    Uint(u8),
    /// `size @ 1..=32 => bytes{size}`
    /// `0 | 33.. => unreachable!()`
    FixedBytes(u8),

    /// `$element[$($size)?]`
    Array(Box<TypeArray>),
    /// `function($($parameters),*) $($attributes)* $(returns ($($returns),+))?`
    Function(Box<TypeFunction>),
    /// `mapping($key $($key_name)? => $value $($value_name)?)`
    Mapping(Box<TypeMapping>),

    /// A custom type.
    Custom(Path),
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
    pub parameters: Vec<Ty>,
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub returns: Vec<Ty>,
}

/// A mapping type.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.mappingType>
#[derive(Clone, Debug)]
pub struct TypeMapping {
    pub key: Box<Ty>,
    pub key_name: Option<Ident>,
    pub value: Box<Ty>,
    pub value_name: Option<Ident>,
}
