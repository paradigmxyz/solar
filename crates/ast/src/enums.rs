use std::fmt;

/// Possible lookups for function resolving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VirtualLookup {
    Static,
    Virtual,
    Super,
}

// How a function can mutate the EVM state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StateMutability {
    Pure,
    View,
    NonPayable,
    Payable,
}

impl fmt::Display for StateMutability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl StateMutability {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::View => "view",
            Self::NonPayable => "nonpayable",
            Self::Payable => "payable",
        }
    }
}

/// Visibility ordered from restricted to unrestricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    Default,
    Private,
    Internal,
    Public,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arithmetic {
    Checked,
    Wrapping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractKind {
    Interface,
    Contract,
    Library,
}

impl fmt::Display for ContractKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ContractKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Interface => "interface",
            Self::Contract => "contract",
            Self::Library => "library",
        }
    }
}

// // Why is this here ??
// /// Container for function call parameter types & names
// struct FuncCallArguments {
//     /// Types of arguments
//     pub types: Vec<()>,
//     /// Names of the arguments if given, otherwise unset
//     pub names: Vec<String>,
// }

// impl FuncCallArguments {
//     pub fn arguments(&self) -> usize {
//         self.types.len()
//     }

//     pub fn names(&self) -> usize {
//         self.names.len()
//     }

//     pub fn has_named_arguments(&self) -> bool {
//         !self.names.is_empty()
//     }
// }
