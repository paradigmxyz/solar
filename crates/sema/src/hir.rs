use sulk_ast::ast;
use sulk_data_structures::{index::IndexVec, newtype_index};

pub struct Hir {
    /// All contracts.
    pub contracts: IndexVec<Contract, ContractData>,
    /// All functions.
    pub functions: IndexVec<Function, FunctionData>,
    /// All structs.
    pub structs: IndexVec<Struct, StructData>,
    /// All enums.
    pub enums: IndexVec<Enum, EnumData>,
}

newtype_index! {
    /// A conctract index.
    pub struct Contract
}

/// A contract.
pub struct ContractData {
    // TODO
}

pub enum ContractItem {
    Function(Function),
    VarDecl(VarDecl),
    Struct(Struct),
    Enum(Enum),
}

newtype_index! {
    /// A conctract index.
    pub struct Contract
}

/// A contract.
pub struct ContractData {
    // TODO
}
