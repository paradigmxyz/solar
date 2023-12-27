use std::fmt;
use sulk_interface::{Span, Symbol};

/// A literal: `hex"1234"`, `5.6 ether`.
#[derive(Clone, Debug)]
pub enum LitKind {
    /// A string literal. Contains the unescaped contents of the string.
    Str(Symbol, StrKind),
    /// A number literal. Contains the parsed contents of the number in base 10.
    Number(Symbol, Base, /* is_sign_negative: */ bool, Option<SubDenomination>),
    /// A boolean literal.
    Bool(bool),
}

/// A single UTF-8 string literal. Only used in import paths and statements, not expressions.
#[derive(Clone, Debug)]
pub struct LitStr {
    pub span: Span,
    pub value: Symbol,
}

/// A string literal kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StrKind {
    /// A regular string literal.
    Str,
    /// A unicode string literal.
    Unicode,
    /// A hex string literal.
    Hex,
}

/// A number sub-denomination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    /// Returns the value of this sub-denomination.
    pub const fn value(self) -> u64 {
        match self {
            Self::Ether(sub_denomination) => sub_denomination.wei(),
            Self::Time(sub_denomination) => sub_denomination.seconds(),
        }
    }
}

/// An ether [`SubDenomination`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimeSubDenomination {
    /// `second`
    Second,
    /// `minute`
    Minute,
    /// `hour`
    Hour,
    /// `day`
    Day,
    /// `week`
    Week,
    /// `year`
    Year,
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
            Self::Second => "second",
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Year => "year",
        }
    }

    /// Returns the number of seconds in this sub-denomination.
    pub const fn seconds(self) -> u64 {
        // https://github.com/ethereum/solidity/blob/2a2a9d37ee69ca77ef530fe18524a3dc8b053104/libsolidity/ast/Types.cpp#L973
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::Hour => 3_600,
            Self::Day => 86_400,
            Self::Week => 604_800,
            Self::Year => 31_536_000,
        }
    }
}

/// Base of numeric literal encoding according to its prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
