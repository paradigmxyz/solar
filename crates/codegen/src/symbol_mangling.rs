//! Codegen symbol mangling.
//!
//! MIR and EVM IR store [`SymbolName`]s rather than formatting source symbols
//! on demand. The encoding stays within Solidity identifier syntax so the
//! ordinary lexer can parse textual IR: bytes that are not valid at their
//! position are written as `$xx`, literal `$` bytes use the same escape, and
//! `$e` represents an empty symbol. Duplicate MIR functions append `$$N`.

use solar_data_structures::fmt;
use solar_interface::{Symbol, sym};
use std::fmt::Write as _;

/// A mangled symbol stored in codegen IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SymbolName(Symbol);

impl SymbolName {
    /// Wraps a symbol that is already mangled.
    pub(crate) const fn from_mangled(symbol: Symbol) -> Self {
        Self(symbol)
    }

    /// Mangles a source symbol.
    pub(crate) fn mangle(symbol: Symbol) -> Self {
        Self::mangle_with_disambiguator(symbol, None)
    }

    /// Mangles a source symbol with a declaration disambiguator.
    pub(crate) fn mangle_with_disambiguator(symbol: Symbol, disambiguator: Option<usize>) -> Self {
        if symbol == Symbol::DUMMY && disambiguator.is_none() {
            return Self(sym::dollar_e);
        }

        let bytes = symbol.as_str().as_bytes();
        if !bytes.is_empty()
            && disambiguator.is_none()
            && bytes.iter().enumerate().all(|(index, &byte)| {
                byte.is_ascii_alphabetic() || byte == b'_' || index != 0 && byte.is_ascii_digit()
            })
        {
            return Self(symbol);
        }

        let mut mangled = String::with_capacity(bytes.len() + disambiguator.map_or(0, |_| 4));
        if bytes.is_empty() {
            mangled.push_str("$e");
        } else {
            for (index, &byte) in bytes.iter().enumerate() {
                let valid = byte.is_ascii_alphabetic()
                    || byte == b'_'
                    || index != 0 && byte.is_ascii_digit();
                if valid {
                    mangled.push(char::from(byte));
                } else {
                    write!(mangled, "${byte:02x}").unwrap();
                }
            }
        }
        if let Some(disambiguator) = disambiguator {
            write!(mangled, "$${disambiguator}").unwrap();
        }
        Self(Symbol::intern(&mangled))
    }

    /// Returns the interned mangled symbol.
    pub(crate) const fn as_symbol(self) -> Symbol {
        self.0
    }

    /// Decodes a mangled symbol into its source symbol and disambiguator.
    pub(crate) fn demangle(self) -> Result<(Symbol, Option<usize>), &'static str> {
        let (symbol, disambiguator) = decode(self.0.as_str())?;
        Ok((Symbol::intern(&symbol), disambiguator))
    }
}

impl fmt::Display for SymbolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn decode(mangled: &str) -> Result<(String, Option<usize>), &'static str> {
    let (mangled, disambiguator) = if let Some((mangled, suffix)) = mangled.split_once("$$") {
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("invalid symbol disambiguator");
        }
        let disambiguator = suffix.parse().map_err(|_| "symbol disambiguator is out of range")?;
        (mangled, Some(disambiguator))
    } else {
        (mangled, None)
    };

    if mangled == "$e" {
        return Ok((String::new(), disambiguator));
    }
    if mangled.is_empty() {
        return Err("empty symbols must be encoded as `$e`");
    }

    let mangled = mangled.as_bytes();
    let mut decoded = Vec::with_capacity(mangled.len());
    let mut index = 0;
    while index < mangled.len() {
        if mangled[index] != b'$' {
            decoded.push(mangled[index]);
            index += 1;
            continue;
        }
        if index + 2 >= mangled.len() {
            return Err("incomplete symbol escape");
        }
        let high = decode_hex(mangled[index + 1]).ok_or("invalid symbol escape")?;
        let low = decode_hex(mangled[index + 2]).ok_or("invalid symbol escape")?;
        decoded.push(high << 4 | low);
        index += 3;
    }

    String::from_utf8(decoded)
        .map(|decoded| (decoded, disambiguator))
        .map_err(|_| "symbol escape is not valid UTF-8")
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{ColorChoice, Session};

    #[test]
    fn symbols_round_trip() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            for (raw, mangled) in [
                ("", "$e"),
                ("name", "name"),
                ("123", "$3123"),
                ("a.b$c", "a$2eb$24c"),
                ("é", "$c3$a9"),
            ] {
                let source = Symbol::intern(raw);
                let name = SymbolName::mangle(source);
                assert_eq!(name.as_symbol().as_str(), mangled);
                assert_eq!(name.demangle(), Ok((source, None)));
            }
        });
    }

    #[test]
    fn disambiguated_symbols_round_trip() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let source = Symbol::intern("over.load");
            let name = SymbolName::mangle_with_disambiguator(source, Some(42));
            assert_eq!(name.as_symbol().as_str(), "over$2eload$$42");
            assert_eq!(name.demangle(), Ok((source, Some(42))));
        });
    }

    #[test]
    fn malformed_symbols_are_rejected() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            for mangled in ["$", "$x0", "name$$", "name$$x", "$e$$1$$2"] {
                let name = SymbolName::from_mangled(Symbol::intern(mangled));
                assert!(name.demangle().is_err(), "{mangled}");
            }
        });
    }
}
