use super::{Block, CallArgs, DocComment, Expr, Path, SemverReq, StrLit, Ty};
use crate::token::Token;
use std::fmt;
use sulk_interface::{Ident, Span};

/// A list of variable declarations.
pub type ParameterList = Vec<VariableDefinition>;

/// A top-level item in a Solidity source file.
#[derive(Clone, Debug)]
pub struct Item {
    pub docs: Vec<DocComment>,
    pub span: Span,
    /// The item's kind.
    pub kind: ItemKind,
}

impl Item {
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

impl ItemKind {
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
#[derive(Clone, Debug)]
pub struct PragmaDirective {
    /// The parsed or unparsed tokens of the pragma directive.
    pub tokens: PragmaTokens,
}

/// The parsed or unparsed tokens of a pragma directive.
#[derive(Clone, Debug)]
pub enum PragmaTokens {
    /// A Semantic Versioning requirement: `pragma solidity <req>;`.
    ///
    /// Note that this is parsed differently from the [`semver`] crate.
    Version(Ident, SemverReq),
    /// `pragma <name> [value];`.
    Custom(IdentOrStrLit, Option<IdentOrStrLit>),
    /// Unparsed tokens: `pragma <tokens...>;`.
    Verbatim(Vec<Token>),
}

impl PragmaTokens {
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
#[derive(Clone, Debug)]
pub struct ImportDirective {
    /// The path string literal value.
    pub path: StrLit,
    pub items: ImportItems,
}

impl ImportDirective {
    /// Returns `true` if the import directive imports all items from the target.
    pub fn imports_all(&self) -> bool {
        matches!(self.items, ImportItems::Glob(None) | ImportItems::Plain(None))
    }
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
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.usingDirective>
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

/// A contract, abstract contract, interface, or library definition:
/// `contract Foo is Bar("foo"), Baz { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.contractDefinition>
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
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.functionDefinition>
#[derive(Clone, Debug)]
pub struct ItemFunction {
    /// What kind of function this is.
    pub kind: FunctionKind,
    /// The function header.
    pub header: FunctionHeader,
    /// The body of the function. This is `;` when the value is `None`.
    pub body: Option<Block>,
}

/// A function header: `function helloWorld() external pure returns(string memory)`.
///
/// Used by all [function items](ItemFunction) and the [function type](super::TyKind::Function).
#[derive(Clone, Debug, Default)]
pub struct FunctionHeader {
    /// The name of the function.
    pub name: Option<Ident>,
    /// The parameters of the function.
    pub parameters: ParameterList,

    pub visibility: Option<Visibility>,
    pub state_mutability: Option<StateMutability>,
    pub modifiers: Vec<Modifier>,
    pub virtual_: bool,
    pub override_: Option<Override>,

    /// The returns parameter list.
    pub returns: ParameterList,
}

/// A kind of function.
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

    /// Returns `true` if the function is allowed in global scope.
    pub fn allowed_in_global(&self) -> bool {
        matches!(self, Self::Function)
    }
}

/// A [modifier invocation][m], or an [inheritance specifier][i].
///
/// [m]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.modifierInvocation
/// [i]: https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.inheritanceSpecifier
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

/// A storage location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataLocation {
    /// `memory`
    Memory,
    /// `storage`
    Storage,
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
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.stateVariableDeclaration>
#[derive(Clone, Debug)]
pub struct VariableDefinition {
    pub ty: Ty,
    pub visibility: Option<Visibility>,
    pub mutability: Option<VarMut>,
    pub data_location: Option<DataLocation>,
    pub override_: Option<Override>,
    pub indexed: bool,
    pub name: Option<Ident>,
    pub initializer: Option<Box<Expr>>,
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
#[derive(Clone, Debug)]
pub struct ItemStruct {
    pub name: Ident,
    pub fields: Vec<VariableDefinition>,
}

/// An enum definition: `enum Foo { A, B, C }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.enumDefinition>
#[derive(Clone, Debug)]
pub struct ItemEnum {
    pub name: Ident,
    pub variants: Vec<Ident>,
}

/// A user-defined value type definition: `type Foo is uint256;`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.userDefinedValueTypeDefinition>
#[derive(Clone, Debug)]
pub struct ItemUdvt {
    pub name: Ident,
    pub ty: Ty,
}

/// An error definition: `error Foo(uint256 a, uint256 b);`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.errorDefinition>
#[derive(Clone, Debug)]
pub struct ItemError {
    pub name: Ident,
    pub parameters: ParameterList,
}

/// An event definition:
/// `event Transfer(address indexed from, address indexed to, uint256 value);`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.eventDefinition>
#[derive(Clone, Debug)]
pub struct ItemEvent {
    pub name: Ident,
    pub parameters: ParameterList,
    pub anonymous: bool,
}
