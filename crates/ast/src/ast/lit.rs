use alloy_primitives::{Address, U256};
use solar_interface::{ByteSymbol, Span, Symbol, diagnostics::ErrorGuaranteed, kw};
use std::fmt;

/// A literal: `hex"1234"`, `5.6 ether`.
///
/// Note that multiple string literals of the same kind are concatenated together to form a single
/// `Lit` (see [`LitKind::Str`]), thus the `span` will be the span of the entire literal, and
/// the `symbol` will be the concatenated string.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.literal>
#[derive(Clone, Copy, Debug)]
pub struct Lit<'ast> {
    /// The concatenated span of the literal.
    pub span: Span,
    /// The original contents of the literal as written in the source code, excluding any quotes.
    ///
    /// If this is a concatenated string literal, this will contain only the **first string
    /// literal's contents**. For all the other string literals, see the `extra` field in
    /// [`LitKind::Str`].
    pub symbol: Symbol,
    /// The "semantic" representation of the literal lowered from the original tokens.
    /// Strings are unescaped and concatenated, hexadecimal forms are eliminated, etc.
    pub kind: LitKind<'ast>,
}

impl fmt::Display for Lit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { ref kind, symbol, span: _ } = *self;
        match kind {
            LitKind::Str(s, ..) => write!(f, "{}\"{symbol}\"", s.prefix()),
            LitKind::Number(_)
            | LitKind::BigNumber(_)
            | LitKind::Rational(..)
            | LitKind::Err(_)
            | LitKind::Address(_)
            | LitKind::Bool(_) => write!(f, "{symbol}"),
        }
    }
}

impl Lit<'_> {
    /// Returns the span of the first string literal in this literal.
    pub fn first_span(&self) -> Span {
        if let LitKind::Str(kind, _, extra) = &self.kind
            && !extra.is_empty()
        {
            let str_len = kind.prefix().len() + 1 + self.symbol.as_str().len() + 1;
            return self.span.with_hi(self.span.lo() + str_len as u32);
        }
        self.span
    }

    /// Returns an iterator over all the literals that were concatenated to form this literal.
    pub fn literals(&self) -> impl Iterator<Item = (Span, Symbol)> + '_ {
        let extra = if let LitKind::Str(_, _, extra) = self.kind { extra } else { &[] };
        std::iter::once((self.first_span(), self.symbol)).chain(extra.iter().copied())
    }

    /// Returns a copy of this literal with any allocated data discarded.
    pub fn copy_without_data<'a>(&self) -> Lit<'a> {
        if let LitKind::Str(str_kind, byte_symbol, items) = self.kind
            && !items.is_empty()
        {
            return Lit {
                span: self.span,
                symbol: self.symbol,
                kind: LitKind::Str(str_kind, byte_symbol, &[]),
            };
        }

        // SAFETY: We just handled the case with data, this is a POD copy.
        unsafe { std::mem::transmute::<Lit<'_>, Lit<'a>>(*self) }
    }
}

/// A kind of literal.
#[derive(Clone, Copy)]
pub enum LitKind<'ast> {
    /// A string, unicode string, or hex string literal. Contains the kind and the unescaped
    /// contents of the string.
    ///
    /// Note that even if this is a string or unicode string literal, invalid UTF-8 sequences
    /// are allowed, and as such this cannot be a `str` or `Symbol`.
    ///
    /// The `[(Span, Symbol)]` contains the extra string literals of the same kind that were
    /// concatenated together to form this literal.
    /// For example, `"foo" "bar"` would be parsed as:
    /// ```ignore (illustrative-debug-format)
    /// # #![rustfmt::skip]
    /// Lit {
    ///     span: 0..11,
    ///     symbol: "foo",
    ///     kind: Str("foobar", [(6..11, "bar")]),
    /// }
    /// ```
    ///
    /// This list is only present in the AST, and is discarded after.
    Str(StrKind, ByteSymbol, &'ast [(Span, Symbol)]),
    /// A decimal or hexadecimal number literal that fits in 256 bits.
    Number(U256),
    /// A number literal wider than 256 bits, such as `1e100`.
    BigNumber(LitUint),
    /// A fractional number literal, such as `0.5`, as its numerator and denominator in lowest
    /// terms.
    ///
    /// Number literals whose values are integers are [`Number`](Self::Number)s or
    /// [`BigNumber`](Self::BigNumber)s, so `1.2e3` is `Number(1200)`.
    Rational(LitUint, LitUint),
    /// An address literal. This is a special case of a 40 digit hexadecimal number literal.
    Address(Address),
    /// A boolean literal.
    Bool(bool),
    /// An error occurred while parsing the literal, which has been emitted.
    Err(ErrorGuaranteed),
}

impl fmt::Debug for LitKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(kind, value, extra) => {
                write!(f, "{kind:?}(")?;
                let value = value.as_byte_str();
                if let Ok(utf8) = std::str::from_utf8(value) {
                    write!(f, "{utf8:?}")?;
                } else {
                    f.write_str(&alloy_primitives::hex::encode_prefixed(value))?;
                }
                if !extra.is_empty() {
                    write!(f, ", {extra:?}")?;
                }
                f.write_str(")")
            }
            Self::Number(value) => write!(f, "Number({value:?})"),
            Self::BigNumber(value) => write!(f, "BigNumber({value:?})"),
            Self::Rational(numer, denom) => write!(f, "Rational({numer:?} / {denom:?})"),
            Self::Address(value) => write!(f, "Address({value:?})"),
            Self::Bool(value) => write!(f, "Bool({value:?})"),
            Self::Err(_) => write!(f, "Err"),
        }
    }
}

impl LitKind<'_> {
    /// Returns `true` if this is a number literal, whatever its value.
    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number(_) | Self::BigNumber(_) | Self::Rational(..))
    }

    /// Returns the description of this literal kind.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Str(kind, ..) => kind.description(),
            Self::Number(_) | Self::BigNumber(_) => "number",
            Self::Rational(..) => "rational",
            Self::Address(_) => "address",
            Self::Bool(_) => "boolean",
            Self::Err(_) => "<error>",
        }
    }
}

/// An unsigned integer of a number literal, interned as its big-endian bytes.
///
/// Number literals that fit in 256 bits are stored inline as [`U256`]s instead. Interning keeps
/// literals small and `Copy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LitUint(ByteSymbol);

impl fmt::Debug for LitUint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&alloy_primitives::hex::encode_prefixed(self.to_be_bytes()))
    }
}

impl LitUint {
    /// Interns an integer from its big-endian bytes, without leading zeros.
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        debug_assert!(bytes.first() != Some(&0), "leading zeros: {bytes:?}");
        Self(ByteSymbol::intern(bytes))
    }

    /// Returns the integer's big-endian bytes, without leading zeros.
    pub fn to_be_bytes(&self) -> &[u8] {
        self.0.as_byte_str()
    }

    /// Returns the number of bits needed to represent the integer.
    pub fn bit_len(&self) -> usize {
        let bytes = self.to_be_bytes();
        bytes.len() * 8 - bytes.first().map_or(0, |b| b.leading_zeros() as usize)
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

    /// Returns the prefix as a string. Empty if `Str`.
    #[doc(alias = "to_str")]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Str => "",
            Self::Unicode => "unicode",
            Self::Hex => "hex",
        }
    }

    /// Returns the prefix as a symbol. Empty if `Str`.
    #[doc(alias = "to_symbol")]
    pub fn prefix_symbol(self) -> Symbol {
        match self {
            Self::Str => kw::Empty,
            Self::Unicode => kw::Unicode,
            Self::Hex => kw::Hex,
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
        // https://github.com/argotorg/solidity/blob/2a2a9d37ee69ca77ef530fe18524a3dc8b053104/libsolidity/ast/Types.cpp#L973
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
        // https://github.com/argotorg/solidity/blob/2a2a9d37ee69ca77ef530fe18524a3dc8b053104/libsolidity/ast/Types.cpp#L973
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

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{BytePos, enter};

    fn bs(s: &[u8]) -> ByteSymbol {
        ByteSymbol::intern(s)
    }

    #[test]
    fn literal_fmt() {
        enter(|| {
            let lit = LitKind::Str(StrKind::Str, bs(b"hello world"), &[]);
            assert_eq!(lit.description(), "string");
            assert_eq!(format!("{lit:?}"), "Str(\"hello world\")");

            let lit = LitKind::Str(StrKind::Str, bs(b"hello\0world"), &[]);
            assert_eq!(lit.description(), "string");
            assert_eq!(format!("{lit:?}"), "Str(\"hello\\0world\")");

            let lit = LitKind::Str(StrKind::Str, bs(&[255u8][..]), &[]);
            assert_eq!(lit.description(), "string");
            assert_eq!(format!("{lit:?}"), "Str(0xff)");

            let extra = [(Span::new(BytePos(69), BytePos(420)), Symbol::intern("world"))];
            let lit = LitKind::Str(StrKind::Str, bs(b"hello world"), &extra);
            assert_eq!(lit.description(), "string");
            assert_eq!(format!("{lit:?}"), "Str(\"hello world\", [(Span(69..420), \"world\")])");
        })
    }
}
