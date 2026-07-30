//! Codegen symbol mangling.
//!
//! MIR and EVM IR store [`SymbolName`]s rather than formatting source symbols
//! on demand. Source names remain unchanged unless a disambiguator is needed,
//! in which case `$N` is appended.

use solar_data_structures::fmt;
use solar_interface::Symbol;
use std::fmt::Write as _;

/// A mangled symbol stored in codegen IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SymbolName(Symbol);

impl SymbolName {
    /// Wraps a stored symbol name.
    pub(crate) const fn new(symbol: Symbol) -> Self {
        Self(symbol)
    }

    /// Mangles a source symbol.
    pub(crate) const fn mangle(symbol: Symbol) -> Self {
        Self(symbol)
    }

    /// Mangles a source symbol with a declaration disambiguator.
    pub(crate) fn mangle_with_disambiguator(symbol: Symbol, disambiguator: Option<usize>) -> Self {
        let Some(disambiguator) = disambiguator else { return Self(symbol) };
        let mut mangled = symbol.to_string();
        write!(mangled, "${disambiguator}").unwrap();
        Self(Symbol::intern(&mangled))
    }

    /// Returns the interned mangled symbol.
    pub(crate) const fn as_symbol(self) -> Symbol {
        self.0
    }
}

impl fmt::Display for SymbolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{ColorChoice, Session};

    #[test]
    fn preserves_undisambiguated_symbols() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            for raw in ["", "name", "123", "a.b$c", "é"] {
                let source = Symbol::intern(raw);
                let name = SymbolName::mangle(source);
                assert_eq!(name.as_symbol(), source);
            }
        });
    }

    #[test]
    fn appends_disambiguator() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let source = ["over", ".load"].concat();
            let source = Symbol::intern(&source);
            let name = SymbolName::mangle_with_disambiguator(source, Some(42));
            assert_eq!(name.as_symbol().as_str(), "over.load$42");
        });
    }
}
