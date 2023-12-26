use std::fmt;

/// Possible lookups for function resolving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VirtualLookup {
    Static,
    Virtual,
    Super,
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
        f.write_str(self.to_str())
    }
}

impl ContractKind {
    /// Returns the string representation of the contract kind.
    pub const fn to_str(self) -> &'static str {
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
