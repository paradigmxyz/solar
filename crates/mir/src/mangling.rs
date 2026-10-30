//! MIR symbol mangling.

use solar_data_structures::newtype_index;
use solar_interface::Symbol;
use std::fmt;

newtype_index! {
    /// A numeric symbol disambiguator.
    pub struct Disambiguator;
}

/// A symbol with an optional numeric disambiguator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MangledSymbol {
    pub symbol: Symbol,
    pub disambiguator: Option<Disambiguator>,
}

impl MangledSymbol {
    /// Creates an undisambiguated symbol.
    pub const fn new(symbol: Symbol) -> Self {
        Self { symbol, disambiguator: None }
    }

    /// Creates a symbol with a numeric disambiguator.
    pub(crate) const fn disambiguated(symbol: Symbol, disambiguator: Disambiguator) -> Self {
        Self { symbol, disambiguator: Some(disambiguator) }
    }
}

impl fmt::Display for MangledSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(disambiguator) = self.disambiguator {
            write!(f, "{}.{}", self.symbol, disambiguator.index())
        } else {
            self.symbol.fmt(f)
        }
    }
}
