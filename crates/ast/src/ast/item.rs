use super::{
    AstPath, BinOpKind, Block, Box, CallArgs, DocComments, Expr, SemverReq, StrLit, Type, UnOpKind,
};
use crate::token::Token;
use either::Either;
use solar_interface::{Ident, Span};
use std::fmt;
use strum::EnumIs;

/// A list of variable declarations.
pub type ParameterList<'ast> = Box<'ast, [VariableDefinition<'ast>]>;

/// A top-level item in a Solidity source file.
#[derive(Debug)]
pub struct Item<'ast> {
    pub docs: DocComments<'ast>,
    pub span: Span,
    /// The item's kind.
    pub kind: ItemKind<'ast>,
}

impl Item<'_> {
    /// Returns the name of the item, if any.
    pub fn name(&self) -> Option<Ident> {
        self.kind.name()
    }

    /// Returns the description of the item.
    pub fn description(&self) -> &'static str {
        self.kind.description()
    }

    /// Returns `true` if the item is allowed inside of contracts.
    pub fn is_allowed_in_contract(&self) -> bool {
        self.kind.is_allowed_in_contract()
    }
}

/// An AST item. A more expanded version of a [Solidity source unit][ref].
///
/// [ref]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.sourceUnit
pub enum ItemKind<'ast> {
    /// A pragma directive: `pragma solidity ^0.8.0;`
    Pragma(PragmaDirective<'ast>),

    /// An import directive: `import "foo.sol";`
    Import(ImportDirective<'ast>),

    /// A `using` directive: `using { A, B.add as + } for uint256 global;`
    Using(UsingDirective<'ast>),

    /// A contract, abstract contract, interface, or library definition:
    /// `contract Foo is Bar, Baz { ... }`
    Contract(ItemContract<'ast>),

    /// A function, constructor, fallback, receive, or modifier definition:
    /// `function helloWorld() external pure returns(string memory);`
    Function(ItemFunction<'ast>),

    /// A state variable or constant definition: `uint256 constant FOO = 42;`
    Variable(VariableDefinition<'ast>),

    /// A struct definition: `struct Foo { uint256 bar; }`
    Struct(ItemStruct<'ast>),

    /// An enum definition: `enum Foo { A, B, C }`
    Enum(ItemEnum<'ast>),

    /// A user-defined value type definition: `type Foo is uint256;`
    Udvt(ItemUdvt<'ast>),

    /// An error definition: `error Foo(uint256 a, uint256 b);`
    Error(ItemError<'ast>),

    /// An event definition:
    /// `event Transfer(address indexed from, address indexed to, uint256 value);`
    Event(ItemEvent<'ast>),
}

impl fmt::Debug for ItemKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ItemKind::")?;
        match self {
            ItemKind::Pragma(item) => item.fmt(f),
            ItemKind::Import(item) => item.fmt(f),
            ItemKind::Using(item) => item.fmt(f),
            ItemKind::Contract(item) => item.fmt(f),
            ItemKind::Function(item) => item.fmt(f),
            ItemKind::Variable(item) => item.fmt(f),
            ItemKind::Struct(item) => item.fmt(f),
            ItemKind::Enum(item) => item.fmt(f),
            ItemKind::Udvt(item) => item.fmt(f),
            ItemKind::Error(item) => item.fmt(f),
            ItemKind::Event(item) => item.fmt(f),
        }
    }
}

impl ItemKind<'_> {
    /// Returns the name of the item, if any.
    pub fn name(&self) -> Option<Ident> {
        match self {
            Self::Pragma(_) | Self::Import(_) | Self::Using(_) => None,
            Self::Contract(item) => Some(item.name),
            Self::Function(item) => item.header.name,
            Self::Variable(item) => item.name,
            Self::Struct(item) => Some(item.name),
            Self::Enum(item) => Some(item.name),
            Self::Udvt(item) => Some(item.name),
            Self::Error(item) => Some(item.name),
            Self::Event(item) => Some(item.name),
        }
    }

    /// Returns the description of the item.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Pragma(_) => "pragma directive",
            Self::Import(_) => "import directive",
            Self::Using(_) => "using directive",
            Self::Contract(_) => "contract definition",
            Self::Function(_) => "function definition",
            Self::Variable(_) => "variable definition",
            Self::Struct(_) => "struct definition",
            Self::Enum(_) => "enum definition",
            Self::Udvt(_) => "user-defined value type definition",
            Self::Error(_) => "error definition",
            Self::Event(_) => "event definition",
        }
    }

    /// Returns `true` if the item is allowed inside of contracts.
    pub fn is_allowed_in_contract(&self) -> bool {
        match self {
            Self::Pragma(_) => false,
            Self::Import(_) => false,
            Self::Using(_) => true,
            Self::Contract(_) => false,
            Self::Function(_) => true,
            Self::Variable(_) => true,
            Self::Struct(_) => true,
            Self::Enum(_) => true,
            Self::Udvt(_) => true,
            Self::Error(_) => true,
            Self::Event(_) => true,
        }
    }
}

/// A pragma directive: `pragma solidity ^0.8.0;`.
#[derive(Debug)]
pub struct PragmaDirective<'ast> {
    /// The parsed or unparsed tokens of the pragma directive.
    pub tokens: PragmaTokens<'ast>,
}

/// The parsed or unparsed tokens of a pragma directive.
#[derive(Debug)]
pub enum PragmaTokens<'ast> {
    /// A Semantic Versioning requirement: `pragma solidity <req>;`.
    ///
    /// Note that this is parsed differently from the [`semver`] crate.
    Version(Ident, SemverReq<'ast>),
    /// `pragma <name> [value];`.
    Custom(IdentOrStrLit, Option<IdentOrStrLit>),
    /// Unparsed tokens: `pragma <tokens...>;`.
    Verbatim(Box<'ast, [Token]>),
}

impl PragmaTokens<'_> {
    /// Returns the name and value of the pragma directive, if any.
    ///
    /// # Examples
    ///
    /// ```solidity
    /// pragma solidity ...;          // None
    /// pragma abicoder v2;           // Some((Ident("abicoder"), Some(Ident("v2"))))
    /// pragma experimental solidity; // Some((Ident("experimental"), Some(Ident("solidity"))))
    /// pragma hello;                 // Some((Ident("hello"), None))
    /// pragma hello world;           // Some((Ident("hello"), Some(Ident("world"))))
    /// pragma hello "world";         // Some((Ident("hello"), Some(StrLit("world"))))
    /// pragma "hello" world;         // Some((StrLit("hello"), Some(Ident("world"))))
    /// pragma ???;                   // None
    /// ```
    pub fn as_name_and_value(&self) -> Option<(&IdentOrStrLit, Option<&IdentOrStrLit>)> {
        match self {
            Self::Custom(name, value) => Some((name, value.as_ref())),
            _ => None,
        }
    }
}

/// An identifier or a string literal.
///
/// This is used in `pragma` declaration because Solc for some reason accepts and treats both as
/// identical.
///
/// Parsed in: <https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L235>
///
/// Syntax-checked in: <https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/analysis/SyntaxChecker.cpp#L77>
#[derive(Clone, Debug)]
pub enum IdentOrStrLit {
    /// An identifier.
    Ident(Ident),
    /// A string literal.
    StrLit(StrLit),
}

impl IdentOrStrLit {
    /// Returns the string value of the identifier or literal.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ident(ident) => ident.as_str(),
            Self::StrLit(str_lit) => str_lit.value.as_str(),
        }
    }

    /// Returns the span of the identifier or literal.
    pub fn span(&self) -> Span {
        match self {
            Self::Ident(ident) => ident.span,
            Self::StrLit(str_lit) => str_lit.span,
        }
    }
}

/// An import directive: `import "foo.sol";`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.importDirective>
#[derive(Debug)]
pub struct ImportDirective<'ast> {
    /// The path string literal value.
    pub path: StrLit,
    pub items: ImportItems<'ast>,
}

impl ImportDirective<'_> {
    /// Returns `true` if the import directive imports all items from the target.
    pub fn imports_all(&self) -> bool {
        matches!(self.items, ImportItems::Glob(None) | ImportItems::Plain(None))
    }
}

/// The path of an import directive.
#[derive(Debug)]
pub enum ImportItems<'ast> {
    /// A plain import directive: `import "foo.sol" as Foo;`.
    Plain(Option<Ident>),
    /// A list of import aliases: `import { Foo as Bar, Baz } from "foo.sol";`.
    Aliases(Box<'ast, [(Ident, Option<Ident>)]>),
    /// A glob import directive: `import * as Foo from "foo.sol";`.
    Glob(Option<Ident>),
}

/// A `using` directive: `using { A, B.add as + } for uint256 global;`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.usingDirective>
#[derive(Debug)]
pub struct UsingDirective<'ast> {
    /// The list of paths.
    pub list: UsingList<'ast>,
    /// The type for which this `using` directive applies. This is `*` if the value is `None`.
    pub ty: Option<Type<'ast>>,
    pub global: bool,
}

/// The path list of a `using` directive.
#[derive(Debug)]
pub enum UsingList<'ast> {
    /// `A.B`
    Single(AstPath<'ast>),
    /// `{ A, B.add as + }`
    Multiple(Box<'ast, [(AstPath<'ast>, Option<UserDefinableOperator>)]>),
}

/// A user-definable operator: `+`, `*`, `|`, etc.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.userDefinableOperator>
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

impl UserDefinableOperator {
    /// Returns this operator as a binary or unary operator.
    pub const fn to_op(self) -> Either<UnOpKind, BinOpKind> {
        match self {
            Self::BitAnd => Either::Right(BinOpKind::BitAnd),
            Self::BitNot => Either::Left(UnOpKind::BitNot),
            Self::BitOr => Either::Right(BinOpKind::BitOr),
            Self::BitXor => Either::Right(BinOpKind::BitXor),
            Self::Add => Either::Right(BinOpKind::Add),
            Self::Div => Either::Right(BinOpKind::Div),
            Self::Rem => Either::Right(BinOpKind::Rem),
            Self::Mul => Either::Right(BinOpKind::Mul),
            Self::Sub => Either::Right(BinOpKind::Sub),
            Self::Eq => Either::Right(BinOpKind::Eq),
            Self::Ge => Either::Right(BinOpKind::Ge),
            Self::Gt => Either::Right(BinOpKind::Gt),
            Self::Le => Either::Right(BinOpKind::Le),
            Self::Lt => Either::Right(BinOpKind::Lt),
            Self::Ne => Either::Right(BinOpKind::Ne),
        }
    }

    /// Returns the string representation of the operator.
    pub const fn to_str(&self) -> &'static str {
        either::for_both!(self.to_op(), op => op.to_str())
    }
}

/// A contract, abstract contract, interface, or library definition:
/// `contract Foo is Bar("foo"), Baz { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.contractDefinition>
#[derive(Debug)]
pub struct ItemContract<'ast> {
    pub kind: ContractKind,
    pub name: Ident,
    pub bases: Box<'ast, [Modifier<'ast>]>,
    pub body: Box<'ast, [Item<'ast>]>,
}

/// The kind of contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIs)]
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
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionDefinition>
#[derive(Debug)]
pub struct ItemFunction<'ast> {
    /// What kind of function this is.
    pub kind: FunctionKind,
    /// The function header.
    pub header: FunctionHeader<'ast>,
    /// The body of the function. This is `;` when the value is `None`.
    pub body: Option<Block<'ast>>,
}

/// A function header: `function helloWorld() external pure returns(string memory)`.
#[derive(Debug, Default)]
pub struct FunctionHeader<'ast> {
    /// The name of the function.
    /// Only `None` if this is a constructor, fallback, or receive function.
    pub name: Option<Ident>,
    /// The parameters of the function.
    pub parameters: ParameterList<'ast>,

    pub visibility: Option<Visibility>,
    pub state_mutability: StateMutability,
    pub modifiers: Box<'ast, [Modifier<'ast>]>,
    pub virtual_: bool,
    pub override_: Option<Override<'ast>>,

    /// The returns parameter list.
    pub returns: ParameterList<'ast>,
}

/// A kind of function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIs)]
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

    /// Returns `true` if the function is allowed in global scope.
    pub fn allowed_in_global(&self) -> bool {
        self.is_ordinary()
    }

    /// Returns `true` if the function is an ordinary function.
    pub fn is_ordinary(&self) -> bool {
        matches!(self, Self::Function)
    }
}

/// A [modifier invocation][m], or an [inheritance specifier][i].
///
/// [m]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.modifierInvocation
/// [i]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.inheritanceSpecifier
#[derive(Debug)]
pub struct Modifier<'ast> {
    pub name: AstPath<'ast>,
    pub arguments: CallArgs<'ast>,
}

/// An override specifier: `override`, `override(a, b.c)`.
#[derive(Debug)]
pub struct Override<'ast> {
    pub span: Span,
    pub paths: Box<'ast, [AstPath<'ast>]>,
}

/// A storage location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataLocation {
    /// `storage`
    Storage,
    /// `transient`
    Transient,
    /// `memory`
    Memory,
    /// `calldata`
    Calldata,
}

impl fmt::Display for DataLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl DataLocation {
    /// Returns the string representation of the storage location.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Transient => "transient",
            Self::Memory => "memory",
            Self::Calldata => "calldata",
        }
    }

    /// Returns the string representation of the storage location, or `"none"` if `None`.
    pub const fn opt_to_str(this: Option<Self>) -> &'static str {
        match this {
            Some(location) => location.to_str(),
            None => "none",
        }
    }
}

// How a function can mutate the EVM state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumIs)]
pub enum StateMutability {
    /// `pure`
    Pure,
    /// `view`
    View,
    /// `payable`
    Payable,
    /// Not specified.
    #[default]
    NonPayable,
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
            Self::NonPayable => "nonpayable",
        }
    }
}

/// Visibility ordered from restricted to unrestricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    /// `private`: visible only in the current contract.
    Private,
    /// `internal`: visible only in the current contract and contracts deriving from it.
    Internal,
    /// `public`: visible internally and externally.
    Public,
    /// `external`: visible only externally.
    External,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_str().fmt(f)
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
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.stateVariableDeclaration>
#[derive(Debug)]
pub struct VariableDefinition<'ast> {
    pub span: Span,
    pub ty: Type<'ast>,
    pub visibility: Option<Visibility>,
    pub mutability: Option<VarMut>,
    pub data_location: Option<DataLocation>,
    pub override_: Option<Override<'ast>>,
    pub indexed: bool,
    pub name: Option<Ident>,
    pub initializer: Option<Box<'ast, Expr<'ast>>>,
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

    /// Returns `true` if the variable is immutable.
    pub const fn is_immutable(self) -> bool {
        matches!(self, Self::Immutable)
    }

    /// Returns `true` if the variable is constant.
    pub const fn is_constant(self) -> bool {
        matches!(self, Self::Constant)
    }
}

/// A struct definition: `struct Foo { uint256 bar; }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.structDefinition>
#[derive(Debug)]
pub struct ItemStruct<'ast> {
    pub name: Ident,
    pub fields: Box<'ast, [VariableDefinition<'ast>]>,
}

/// An enum definition: `enum Foo { A, B, C }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.enumDefinition>
#[derive(Debug)]
pub struct ItemEnum<'ast> {
    pub name: Ident,
    pub variants: Box<'ast, [Ident]>,
}

/// A user-defined value type definition: `type Foo is uint256;`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.userDefinedValueTypeDefinition>
#[derive(Debug)]
pub struct ItemUdvt<'ast> {
    pub name: Ident,
    pub ty: Type<'ast>,
}

/// An error definition: `error Foo(uint256 a, uint256 b);`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.errorDefinition>
#[derive(Debug)]
pub struct ItemError<'ast> {
    pub name: Ident,
    pub parameters: ParameterList<'ast>,
}

/// An event definition:
/// `event Transfer(address indexed from, address indexed to, uint256 value);`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.eventDefinition>
#[derive(Debug)]
pub struct ItemEvent<'ast> {
    pub name: Ident,
    pub parameters: ParameterList<'ast>,
    pub anonymous: bool,
}
