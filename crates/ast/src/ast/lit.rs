use alloy_primitives::Address;
use solar_interface::{diagnostics::ErrorGuaranteed, kw, Span, Symbol};
use std::{fmt, sync::Arc};

/// A literal: `hex"1234"`, `5.6 ether`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.literal>
#[derive(Clone, Debug)]
pub struct Lit {
    /// The span of the literal.
    pub span: Span,
    /// The original literal as written in the source code.
    pub symbol: Symbol,
    /// The "semantic" representation of the literal lowered from the original tokens.
    /// Strings are unescaped, hexadecimal forms are eliminated, etc.
    pub kind: LitKind,
}

/// A kind of literal.
#[derive(Clone, Debug)]
pub enum LitKind {
    /// A string, unicode string, or hex string literal. Contains the kind and the unescaped
    /// contents of the string.
    ///
    /// Note that even if this is a string or unicode string literal, invalid UTF-8 sequences
    /// are allowed, and as such this cannot be a `str` or `Symbol`.
    Str(StrKind, Arc<[u8]>),
    /// A decimal or hexadecimal number literal.
    Number(num_bigint::BigInt),
    /// A rational number literal.
    ///
    /// Note that rational literals that evaluate to integers are represented as
    /// [`Number`](Self::Number) (e.g. `1.2e3` is represented as `Number(1200)`).
    Rational(num_rational::BigRational),
    /// An address literal. This is a special case of a 40 digit hexadecimal number literal.
    Address(Address),
    /// A boolean literal.
    Bool(bool),
    /// An error occurred while parsing the literal, which has been emitted.
    Err(ErrorGuaranteed),
}

impl LitKind {
    /// Returns the description of this literal kind.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Str(kind, _) => kind.description(),
            Self::Number(_) => "number",
            Self::Rational(_) => "rational",
            Self::Address(_) => "address",
            Self::Bool(_) => "boolean",
            Self::Err(_) => "<error>",
        }
    }
}

/// A single UTF-8 string literal. Only used in import paths and statements, not expressions.
#[derive(Clone, Debug)]
pub struct StrLit {
    /// The span of the literal.
    pub span: Span,
    /// The contents of the string. Not unescaped.
    pub value: Symbol,
}

/// A string literal kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StrKind {
    /// A regular string literal.
    Str,
    /// A unicode string literal.
    Unicode,
    /// A hex string literal.
    Hex,
}

impl StrKind {
    /// Returns the description of this string kind.
    pub fn description(self) -> &'static str {
        match self {
            Self::Str => "string",
            Self::Unicode => "unicode string",
            Self::Hex => "hex string",
        }
    }
}

/// A number sub-denomination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubDenomination {
    /// An ether sub-denomination.
    Ether(EtherSubDenomination),
    /// A time sub-denomination.
    Time(TimeSubDenomination),
}

impl fmt::Display for SubDenomination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ether(sub_denomination) => sub_denomination.fmt(f),
            Self::Time(sub_denomination) => sub_denomination.fmt(f),
        }
    }
}

impl SubDenomination {
    /// Returns the name of this sub-denomination as a string.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Ether(sub_denomination) => sub_denomination.to_str(),
            Self::Time(sub_denomination) => sub_denomination.to_str(),
        }
    }

    /// Returns the symbol of this sub-denomination.
    pub const fn to_symbol(self) -> Symbol {
        match self {
            Self::Ether(sub_denomination) => sub_denomination.to_symbol(),
            Self::Time(sub_denomination) => sub_denomination.to_symbol(),
        }
    }

    /// Returns the value of this sub-denomination.
    pub const fn value(self) -> u64 {
        match self {
            Self::Ether(sub_denomination) => sub_denomination.wei(),
            Self::Time(sub_denomination) => sub_denomination.seconds(),
        }
    }
}

/// An ether [`SubDenomination`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EtherSubDenomination {
    /// `wei`
    Wei,
    /// `gwei`
    Gwei,
    /// `ether`
    Ether,
}

impl fmt::Display for EtherSubDenomination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl EtherSubDenomination {
    /// Returns the name of this sub-denomination as a string.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Wei => "wei",
            Self::Gwei => "gwei",
            Self::Ether => "ether",
        }
    }

    /// Returns the symbol of this sub-denomination.
    pub const fn to_symbol(self) -> Symbol {
        match self {
            Self::Wei => kw::Wei,
            Self::Gwei => kw::Gwei,
            Self::Ether => kw::Ether,
        }
    }

    /// Returns the number of wei in this sub-denomination.
    pub const fn wei(self) -> u64 {
        // https://github.com/ethereum/solidity/blob/2a2a9d37ee69ca77ef530fe18524a3dc8b053104/libsolidity/ast/Types.cpp#L973
        match self {
            Self::Wei => 1,
            Self::Gwei => 1_000_000_000,
            Self::Ether => 1_000_000_000_000_000_000,
        }
    }
}

/// A time [`SubDenomination`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeSubDenomination {
    /// `seconds`
    Seconds,
    /// `minutes`
    Minutes,
    /// `hours`
    Hours,
    /// `days`
    Days,
    /// `weeks`
    Weeks,
    /// `years`
    Years,
}

impl fmt::Display for TimeSubDenomination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl TimeSubDenomination {
    /// Returns the name of this sub-denomination as a string.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
            Self::Weeks => "weeks",
            Self::Years => "years",
        }
    }

    /// Returns the symbol of this sub-denomination.
    pub const fn to_symbol(self) -> Symbol {
        match self {
            Self::Seconds => kw::Seconds,
            Self::Minutes => kw::Minutes,
            Self::Hours => kw::Hours,
            Self::Days => kw::Days,
            Self::Weeks => kw::Weeks,
            Self::Years => kw::Years,
        }
    }

    /// Returns the number of seconds in this sub-denomination.
    pub const fn seconds(self) -> u64 {
        // https://github.com/ethereum/solidity/blob/2a2a9d37ee69ca77ef530fe18524a3dc8b053104/libsolidity/ast/Types.cpp#L973
        match self {
            Self::Seconds => 1,
            Self::Minutes => 60,
            Self::Hours => 3_600,
            Self::Days => 86_400,
            Self::Weeks => 604_800,
            Self::Years => 31_536_000,
        }
    }
}

/// Base of numeric literal encoding according to its prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Base {
    /// Literal starts with "0b".
    Binary = 2,
    /// Literal starts with "0o".
    Octal = 8,
    /// Literal doesn't contain a prefix.
    Decimal = 10,
    /// Literal starts with "0x".
    Hexadecimal = 16,
}

impl fmt::Display for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_str().fmt(f)
    }
}

impl Base {
    /// Returns the name of the base as a string.
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Octal => "octal",
            Self::Decimal => "decimal",
            Self::Hexadecimal => "hexadecimal",
        }
    }
}
