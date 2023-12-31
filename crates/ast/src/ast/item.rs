use super::{Block, CallArgs, Expr, Path, Ty};
use crate::token::Token;
use std::fmt;
use sulk_interface::{Ident, Span, Symbol};

/// A list of variable declarations.
pub type ParameterList = Vec<VariableDeclaration>;

/// A top-level item in a Solidity source file.
#[derive(Clone, Debug)]
pub struct Item {
    pub span: Span,
    /// The item's kind.
    pub kind: ItemKind,
}

/// An AST item. A more expanded version of a [Solidity source unit][ref].
///
/// [ref]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.sourceUnit
#[derive(Clone, Debug)]
pub enum ItemKind {
    /// A pragma directive: `pragma solidity ^0.8.0;`
    Pragma(PragmaDirective),

    /// An import directive: `import "foo.sol";`
    Import(ImportDirective),

    /// A `using` directive: `using { A, B.add as + } for uint256 global;`
    Using(UsingDirective),

    /// A contract, abstract contract, interface, or library definition:
    /// `contract Foo is Bar, Baz { ... }`
    Contract(ItemContract),

    /// A function, constructor, fallback, receive, or modifier definition:
    /// `function helloWorld() external pure returns(string memory);`
    Function(ItemFunction),

    /// A state variable or constant definition: `uint256 constant FOO = 42;`
    Variable(VariableDefinition),

    /// A struct definition: `struct Foo { uint256 bar; }`
    Struct(ItemStruct),

    /// An enum definition: `enum Foo { A, B, C }`
    Enum(ItemEnum),

    /// A user-defined value type definition: `type Foo is uint256;`
    Udvt(ItemUdvt),

    /// An error definition: `error Foo(uint256 a, uint256 b);`
    Error(ItemError),

    /// An event definition:
    /// `event Transfer(address indexed from, address indexed to, uint256 value);`
    Event(ItemEvent),
}

/// A pragma directive: `pragma solidity ^0.8.0;`.
#[derive(Clone, Debug)]
pub struct PragmaDirective {
    /// The parsed or unparsed tokens of the pragma directive.
    pub tokens: PragmaTokens,
}

#[derive(Clone, Debug)]
pub enum PragmaTokens {
    /// A Solidity Semantic Versioning requirement: `pragma solidity <req>;`.
    ///
    /// Note that this is parsed differently from the [`semver`] crate, but we still make use of
    /// the [`VersionReq`](semver::VersionReq) type.
    Solidity(semver::VersionReq),
    /// `pragma abicoder <ident>;`
    Abicoder(Ident),
    /// `pragma experimental <ident>;`
    Experimental(Ident),
    /// Unparsed tokens: `pragma <tokens>...;`.
    Verbatim(Vec<Token>),
}

/// An import directive: `import "foo.sol";`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.importDirective>
#[derive(Clone, Debug)]
pub struct ImportDirective {
    /// The path string literal value.
    pub path: Symbol,
    pub path_span: Span,
    pub items: ImportItems,
}

/// The path of an import directive.
#[derive(Clone, Debug)]
pub enum ImportItems {
    /// A plain import directive: `import "foo.sol" as Foo;`.
    Plain(Option<Ident>),
    /// A list of import aliases: `import { Foo as Bar, Baz } from "foo.sol";`.
    Aliases(Vec<(Ident, Option<Ident>)>),
    /// A glob import directive: `import * as Foo from "foo.sol";`.
    Glob(Option<Ident>),
}

/// A `using` directive: `using { A, B.add as + } for uint256 global;`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.usingDirective>
#[derive(Clone, Debug)]
pub struct UsingDirective {
    /// The list of paths.
    pub list: UsingList,
    /// The type for which this `using` directive applies. This is `*` if the value is `None`.
    pub ty: Option<Ty>,
    pub global: bool,
}

/// The path list of a `using` directive.
#[derive(Clone, Debug)]
pub enum UsingList {
    /// `A.B`
    Single(Path),
    /// `{ A, B.add as + }`
    Multiple(Vec<(Path, Option<UserDefinableOperator>)>),
}

/// A user-definable operator: `+`, `*`, `|`, etc.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.userDefinableOperator>
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UserDefinableOperator {
    /// `&`
    BitAnd,
    /// `~`
    BitNot,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// `+`
    Add,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `*`
    Mul,
    /// `-`
    Sub,
    /// `==`
    Eq,
    /// `>=`
    Ge,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `<`
    Lt,
    /// `!=`
    Ne,
}

/// A contract, abstract contract, interface, or library definition:
/// `contract Foo is Bar("foo"), Baz { ... }`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.contractDefinition>
#[derive(Clone, Debug)]
pub struct ItemContract {
    pub kind: ContractKind,
    pub name: Ident,
    pub inheritance: Vec<Modifier>,
    pub body: Vec<Item>,
}

/// The kind of contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContractKind {
    /// `contract`
    Contract,
    /// `abstract contract`
    AbstractContract,
    /// `interface`
    Interface,
    /// `library`
    Library,
}

impl fmt::Display for ContractKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl ContractKind {
    /// Returns the string representation of the contract kind.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::AbstractContract => "abstract contract",
            Self::Interface => "interface",
            Self::Library => "library",
        }
    }
}

/// A function, constructor, fallback, receive, or modifier definition:
/// `function helloWorld() external pure returns(string memory);`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionDefinition>
#[derive(Clone, Debug)]
pub struct ItemFunction {
    pub kind: FunctionKind,
    /// The name of the function.
    /// Constructors, fallbacks, and receive functions do not have names.
    pub name: Option<Ident>,
    /// Parens are optional for modifiers:
    /// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.modifierDefinition>
    pub parameters: ParameterList,
    /// The Solidity attributes of the function.
    pub attributes: FunctionAttributes,
    /// The optional return types of the function.
    pub returns: ParameterList,
    /// The body of the function. This is `;` when the value is `None`.
    pub body: Option<Block>,
}

/// The kind of function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionKind {
    /// `constructor`
    Constructor,
    /// `function`
    Function,
    /// `fallback`
    Fallback,
    /// `receive`
    Receive,
    /// `modifier`
    Modifier,
}

impl fmt::Display for FunctionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl FunctionKind {
    /// Returns the string representation of the function kind.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Constructor => "constructor",
            Self::Function => "function",
            Self::Fallback => "fallback",
            Self::Receive => "receive",
            Self::Modifier => "modifier",
        }
    }

    /// Returns `true` if the function item requires a name.
    #[inline]
    pub const fn requires_name(self) -> bool {
        matches!(self, Self::Function | Self::Modifier)
    }

    /// Returns `true` if the function item can omit parentheses for the parameter list.
    #[inline]
    pub const fn can_omit_parens(self) -> bool {
        matches!(self, Self::Modifier)
    }

    /// Returns `true` if the function item can have attributes.
    #[inline]
    pub const fn can_have_attributes(&self) -> bool {
        matches!(self, Self::Function | Self::Modifier)
    }

    /// Returns `true` if the function item can have a return type.
    #[inline]
    pub fn can_have_returns(&self) -> bool {
        matches!(self, Self::Function | Self::Modifier)
    }
}

/// The attributes of a function.
#[derive(Clone, Debug)]
pub struct FunctionAttributes {
    pub span: Span,
    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub modifiers: Vec<Modifier>,
    pub virtual_: bool,
    pub overrides: Vec<Override>,
}

impl FunctionAttributes {
    /// Returns `true` if the attributes are empty.
    pub fn is_empty(&self) -> bool {
        self.visibility.is_none()
            && self.state_mutability.is_none()
            && self.modifiers.is_empty()
            && !self.virtual_
            && self.overrides.is_empty()
    }
}

/// A modifier invocation, or an inheritance specifier.
#[derive(Clone, Debug)]
pub struct Modifier {
    pub name: Path,
    pub arguments: CallArgs,
}

/// An override specifier: `override(a, b.c)`.
#[derive(Clone, Debug)]
pub struct Override {
    pub span: Span,
    pub paths: Vec<Path>,
}

/// A variable declaration: `string memory hello`.
#[derive(Clone, Debug)]
pub struct VariableDeclaration {
    /// The type of the variable.
    pub ty: Ty,
    /// The storage location of the variable, if any.
    pub storage: Option<Storage>,
    /// Whether the variable is indexed.
    pub indexed: bool,
    /// The name of the variable.
    /// This is always `Some` if parsed as part of [`ParameterList`] or a [`Stmt`](super::Stmt).
    pub name: Option<Ident>,
}

/// A storage location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Storage {
    /// `memory`
    Memory,
    /// `storage`
    Storage,
    /// `calldata`
    Calldata,
}

impl fmt::Display for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl Storage {
    /// Returns the string representation of the storage location.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Storage => "storage",
            Self::Calldata => "calldata",
        }
    }
}

// How a function can mutate the EVM state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateMutability {
    /// `pure`
    Pure,
    /// `view`
    View,
    /// `payable`
    Payable,
}

impl fmt::Display for StateMutability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl StateMutability {
    /// Returns the string representation of the state mutability.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::View => "view",
            Self::Payable => "payable",
        }
    }
}

/// Visibility ordered from restricted to unrestricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// `private`
    Private,
    /// `internal`
    Internal,
    /// `public`
    Public,
    /// `external`
    External,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl Visibility {
    /// Returns the string representation of the visibility.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Internal => "internal",
            Self::Public => "public",
            Self::External => "external",
        }
    }
}

/// A state variable or constant definition: `uint256 constant FOO = 42;`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.stateVariableDeclaration>
#[derive(Clone, Debug)]
pub struct VariableDefinition {
    pub ty: Ty,
    pub visibility: Option<Visibility>,
    pub mutability: Option<VarMut>,
    pub storage: Option<Storage>,
    pub name: Ident,
    pub initializer: Option<Expr>,
}

/// The mutability of a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarMut {
    /// `immutable`
    Immutable,
    /// `constant`
    Constant,
}

impl fmt::Display for VarMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl VarMut {
    /// Returns the string representation of the variable mutability.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Immutable => "immutable",
            Self::Constant => "constant",
        }
    }
}

/// A struct definition: `struct Foo { uint256 bar; }`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.structDefinition>
#[derive(Clone, Debug)]
pub struct ItemStruct {
    pub name: Ident,
    pub fields: Vec<VariableDeclaration>,
}

/// An enum definition: `enum Foo { A, B, C }`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.enumDefinition>
#[derive(Clone, Debug)]
pub struct ItemEnum {
    pub name: Ident,
    pub variants: Vec<Ident>,
}

/// A user-defined value type definition: `type Foo is uint256;`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.userDefinedValueTypeDefinition>
#[derive(Clone, Debug)]
pub struct ItemUdvt {
    pub name: Ident,
    pub ty: Ty,
}

/// An error definition: `error Foo(uint256 a, uint256 b);`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.errorDefinition>
#[derive(Clone, Debug)]
pub struct ItemError {
    pub name: Ident,
    pub parameters: ParameterList,
}

/// An event definition:
/// `event Transfer(address indexed from, address indexed to, uint256 value);`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.eventDefinition>
#[derive(Clone, Debug)]
pub struct ItemEvent {
    pub name: Ident,
    pub parameters: ParameterList,
}
