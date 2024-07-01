use std::marker::PhantomData;
use sulk_data_structures::{index::IndexVec, newtype_index};
use sulk_interface::Ident;

pub use sulk_ast::ast::ContractKind;

/// The high-level intermediate representation (HIR).
///
/// This struct contains all the information about the ent
pub struct Hir<'hir> {
    /// All contracts.
    contracts: IndexVec<ContractId, Contract<'hir>>,
    /// All functions.
    functions: IndexVec<FunctionId, Function<'hir>>,
    /// All structs.
    structs: IndexVec<StructId, Struct<'hir>>,
    /// All enums.
    enums: IndexVec<EnumId, Enum<'hir>>,
    /// All events.
    events: IndexVec<EventId, Event<'hir>>,
    /// All custom errors.
    errors: IndexVec<ErrorId, Error<'hir>>,
    /// All constants and variables.
    vars: IndexVec<VarId, Var<'hir>>,
}

newtype_index! {
    /// A [`Contract`] ID.
    pub struct ContractId;

    /// A [`Function`] ID.
    pub struct FunctionId;

    /// A [`Struct`] ID.
    pub struct StructId;

    /// An [`Enum`] ID.
    pub struct EnumId;

    /// An [`Event`] ID.
    pub struct EventId;

    /// An [`Error`] ID.
    pub struct ErrorId;

    /// A [`Var`] ID.
    pub struct VarId;
}

/// A contract, interface, or library.
#[derive(Debug)]
pub struct Contract<'hir> {
    /// The function name.
    pub name: Ident,
    /// The contract kind.
    pub kind: ContractKind,
    /// The contract bases.
    pub bases: &'hir [ContractId],
    pub ctor: Option<FunctionId>,
    _tmp: PhantomData<&'hir ()>,
}

/// A function.
#[derive(Debug)]
pub struct Function<'hir> {
    /// The item name.
    pub name: Ident,
    _tmp: PhantomData<&'hir ()>,
}

/// A struct.
#[derive(Debug)]
pub struct Struct<'hir> {
    /// The item name.
    pub name: Ident,
    _tmp: PhantomData<&'hir ()>,
}

/// An enum.
#[derive(Debug)]
pub struct Enum<'hir> {
    /// The item name.
    pub name: Ident,
    /// The enum variants.
    pub variants: &'hir [Ident],
    _tmp: PhantomData<&'hir ()>,
}

/// An event.
#[derive(Debug)]
pub struct Event<'hir> {
    /// The item name.
    pub name: Ident,
    _tmp: PhantomData<&'hir ()>,
}

/// A custom error.
#[derive(Debug)]
pub struct Error<'hir> {
    /// The item name.
    pub name: Ident,
    _tmp: PhantomData<&'hir ()>,
}

/// A constant or variable declaration.
#[derive(Debug)]
pub struct Var<'hir> {
    /// The item name.
    pub name: Ident,
    _tmp: PhantomData<&'hir ()>,
}

/// A statement.
#[derive(Debug)]
pub struct Stmt<'hir> {
    _tmp: PhantomData<&'hir ()>,
}

/// An expression.
#[derive(Debug)]
pub struct Expr<'hir> {
    _tmp: PhantomData<&'hir ()>,
}
