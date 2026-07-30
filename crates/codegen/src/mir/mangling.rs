//! MIR symbol mangling.

use solar_data_structures::fmt;
use solar_interface::Symbol;
use std::num::NonZeroU32;

/// A symbol with an optional numeric disambiguator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MangledSymbol {
    pub(crate) symbol: Symbol,
    pub(crate) disambiguator: Option<NonZeroU32>,
}

impl MangledSymbol {
    /// Creates an undisambiguated symbol.
    pub(crate) const fn new(symbol: Symbol) -> Self {
        Self { symbol, disambiguator: None }
    }

    /// Creates a symbol with a numeric disambiguator.
    pub(crate) const fn disambiguated(symbol: Symbol, disambiguator: NonZeroU32) -> Self {
        Self { symbol, disambiguator: Some(disambiguator) }
    }
}

impl fmt::Display for MangledSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(disambiguator) = self.disambiguator {
            write!(f, "{}.{disambiguator}", self.symbol)
        } else {
            self.symbol.fmt(f)
        }
    }
}
