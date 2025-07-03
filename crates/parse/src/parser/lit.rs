use crate::{PResult, Parser, unescape};
use num_bigint::{BigInt, BigUint};
use num_rational::BigRational;
use num_traits::{Num, Signed, Zero};
use solar_ast::{token::*, *};
use solar_interface::{Symbol, diagnostics::ErrorGuaranteed, kw};
use std::{borrow::Cow, fmt};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a literal.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_lit(&mut self) -> PResult<'sess, &'ast mut Lit> {
        self.parse_spanned(Self::parse_lit_inner)
            .map(|(span, (symbol, kind))| self.arena.literals.alloc(Lit { span, symbol, kind }))
    }

    /// Parses a literal with an optional subdenomination.
    ///
    /// Note that the subdenomination gets applied to the literal directly, and is returned just for
    /// display reasons.
    ///
    /// Returns None if no subdenomination was parsed or if the literal is not a number or rational.
    pub fn parse_lit_with_subdenomination(
        &mut self,
    ) -> PResult<'sess, (&'ast mut Lit, Option<SubDenomination>)> {
        let lit = self.parse_lit()?;
        let mut sub = self.parse_subdenomination();
        if let opt @ Some(_) = &mut sub {
            let Some(sub) = opt else { unreachable!() };
            match &mut lit.kind {
                LitKind::Number(n) => *n *= sub.value(),
                l @ LitKind::Rational(_) => {
                    let LitKind::Rational(n) = l else { unreachable!() };
                    *n *= BigInt::from(sub.value());
                    if n.is_integer() {
                        *l = LitKind::Number(n.to_integer());
                    }
                }
                _ => {
                    *opt = None;
                    let msg = "sub-denominations are only allowed on number and rational literals";
                    self.dcx().err(msg).span(lit.span.to(self.prev_token.span)).emit();
                }
            }
        }
        Ok((lit, sub))
    }

    /// Parses a subdenomination.
    pub fn parse_subdenomination(&mut self) -> Option<SubDenomination> {
        let sub = self.subdenomination();
        if sub.is_some() {
            self.bump();
        }
        sub
    }

    fn subdenomination(&self) -> Option<SubDenomination> {
        match self.token.ident()?.name {
            kw::Wei => Some(SubDenomination::Ether(EtherSubDenomination::Wei)),
            kw::Gwei => Some(SubDenomination::Ether(EtherSubDenomination::Gwei)),
            kw::Ether => Some(SubDenomination::Ether(EtherSubDenomination::Ether)),

            kw::Seconds => Some(SubDenomination::Time(TimeSubDenomination::Seconds)),
            kw::Minutes => Some(SubDenomination::Time(TimeSubDenomination::Minutes)),
            kw::Hours => Some(SubDenomination::Time(TimeSubDenomination::Hours)),
            kw::Days => Some(SubDenomination::Time(TimeSubDenomination::Days)),
            kw::Weeks => Some(SubDenomination::Time(TimeSubDenomination::Weeks)),
            kw::Years => Some(SubDenomination::Time(TimeSubDenomination::Years)),

            _ => None,
        }
    }

    /// Emits an error if a subdenomination was parsed.
    pub(super) fn expect_no_subdenomination(&mut self) {
        if let Some(_sub) = self.parse_subdenomination() {
            let span = self.prev_token.span;
            self.dcx().err("subdenominations aren't allowed here").span(span).emit();
        }
    }

    fn parse_lit_inner(&mut self) -> PResult<'sess, (Symbol, LitKind)> {
        let lo = self.token.span;
        if let TokenKind::Ident(symbol @ (kw::True | kw::False)) = self.token.kind {
            self.bump();
            return Ok((symbol, LitKind::Bool(symbol != kw::False)));
        }

        if !self.check_lit() {
            return self.unexpected();
        }

        let Some(lit) = self.token.lit() else {
            unreachable!("check_lit() returned true for non-literal token");
        };
        self.bump();
        let result = match lit.kind {
            TokenLitKind::Integer => self.parse_lit_int(lit.symbol),
            TokenLitKind::Rational => self.parse_lit_rational(lit.symbol),
            TokenLitKind::Str | TokenLitKind::UnicodeStr | TokenLitKind::HexStr => {
                self.parse_lit_str(lit)
            }
            TokenLitKind::Err(guar) => Ok(LitKind::Err(guar)),
        };
        let kind =
            result.unwrap_or_else(|e| LitKind::Err(e.span(lo.to(self.prev_token.span)).emit()));
        Ok((lit.symbol, kind))
    }

    /// Parses an integer literal.
    fn parse_lit_int(&mut self, symbol: Symbol) -> PResult<'sess, LitKind> {
        use LitError::*;
        match parse_integer(symbol) {
            Ok(l) => Ok(l),
            // User error.
            Err(e @ (IntegerLeadingZeros | IntegerTooLarge)) => Err(self.dcx().err(e.to_string())),
            // User error, but already emitted.
            Err(EmptyInteger) => Ok(LitKind::Err(ErrorGuaranteed::new_unchecked())),
            // Lexer internal error.
            Err(e @ ParseInteger(_)) => panic!("failed to parse integer literal {symbol:?}: {e}"),
            // Should never happen.
            Err(
                e @ (InvalidRational | EmptyRational | EmptyExponent | ParseRational(_)
                | ParseExponent(_) | RationalTooLarge | ExponentTooLarge),
            ) => panic!("this error shouldn't happen for normal integer literals: {e}"),
        }
    }

    /// Parses a rational literal.
    fn parse_lit_rational(&mut self, symbol: Symbol) -> PResult<'sess, LitKind> {
        use LitError::*;
        match parse_rational(symbol) {
            Ok(l) => Ok(l),
            // User error.
            Err(
                e @ (EmptyRational | IntegerTooLarge | RationalTooLarge | ExponentTooLarge
                | IntegerLeadingZeros),
            ) => Err(self.dcx().err(e.to_string())),
            // User error, but already emitted.
            Err(InvalidRational | EmptyExponent) => {
                Ok(LitKind::Err(ErrorGuaranteed::new_unchecked()))
            }
            // Lexer internal error.
            Err(e @ (ParseExponent(_) | ParseInteger(_) | ParseRational(_) | EmptyInteger)) => {
                panic!("failed to parse rational literal {symbol:?}: {e}")
            }
        }
    }

    /// Parses a string literal.
    fn parse_lit_str(&mut self, lit: TokenLit) -> PResult<'sess, LitKind> {
        let mode = match lit.kind {
            TokenLitKind::Str => StrKind::Str,
            TokenLitKind::UnicodeStr => StrKind::Unicode,
            TokenLitKind::HexStr => StrKind::Hex,
            _ => unreachable!(),
        };

        let span = self.prev_token.span;
        let (mut value, _) =
            unescape::parse_string_literal(lit.symbol.as_str(), mode, span, self.sess);
        let mut extra = vec![];
        while let Some(TokenLit { symbol, kind }) = self.token.lit() {
            if kind != lit.kind {
                break;
            }
            extra.push((self.token.span, symbol));
            let (parsed, _) =
                unescape::parse_string_literal(symbol.as_str(), mode, self.token.span, self.sess);
            value.to_mut().extend_from_slice(&parsed);
            self.bump();
        }

        let kind = match lit.kind {
            TokenLitKind::Str => StrKind::Str,
            TokenLitKind::UnicodeStr => StrKind::Unicode,
            TokenLitKind::HexStr => StrKind::Hex,
            _ => unreachable!(),
        };
        Ok(LitKind::Str(kind, value.into(), extra))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LitError {
    InvalidRational,

    EmptyInteger,
    EmptyRational,
    EmptyExponent,

    ParseInteger(num_bigint::ParseBigIntError),
    ParseRational(num_bigint::ParseBigIntError),
    ParseExponent(std::num::ParseIntError),

    IntegerTooLarge,
    RationalTooLarge,
    ExponentTooLarge,
    IntegerLeadingZeros,
}

impl fmt::Display for LitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRational => write!(f, "invalid rational literal"),
            Self::EmptyInteger => write!(f, "empty integer"),
            Self::EmptyRational => write!(f, "empty rational"),
            Self::EmptyExponent => write!(f, "empty exponent"),
            Self::ParseInteger(e) => write!(f, "failed to parse integer: {e}"),
            Self::ParseRational(e) => write!(f, "failed to parse rational: {e}"),
            Self::ParseExponent(e) => write!(f, "failed to parse exponent: {e}"),
            Self::IntegerTooLarge => write!(f, "integer part too large"),
            Self::RationalTooLarge => write!(f, "rational part too large"),
            Self::ExponentTooLarge => write!(f, "exponent too large"),
            Self::IntegerLeadingZeros => write!(f, "leading zeros are not allowed in integers"),
        }
    }
}

fn parse_integer(symbol: Symbol) -> Result<LitKind, LitError> {
    let s = &strip_underscores(&symbol)[..];
    let base = match s.as_bytes() {
        [b'0', b'x', ..] => Base::Hexadecimal,
        [b'0', b'o', ..] => Base::Octal,
        [b'0', b'b', ..] => Base::Binary,
        _ => Base::Decimal,
    };

    if base == Base::Decimal && s.starts_with('0') && s.len() > 1 {
        return Err(LitError::IntegerLeadingZeros);
    }

    // Address literal.
    if base == Base::Hexadecimal
        && s.len() == 42
        && let Ok(address) = s.parse()
    {
        return Ok(LitKind::Address(address));
    }

    let start = if base == Base::Decimal { 0 } else { 2 };
    let s = &s[start..];
    if s.is_empty() {
        return Err(LitError::EmptyInteger);
    }
    big_int_from_str_radix(s, base, false).map(LitKind::Number)
}

fn parse_rational(symbol: Symbol) -> Result<LitKind, LitError> {
    let s = &strip_underscores(&symbol)[..];
    debug_assert!(!s.is_empty());

    if matches!(s.get(..2), Some("0b" | "0o" | "0x")) {
        return Err(LitError::InvalidRational);
    }

    // Split the string into integer, rational, and exponent parts.
    let (mut int, rat, exp) = match (s.find('.'), s.find(['e', 'E'])) {
        // X
        (None, None) => (s, None, None),
        // X.Y
        (Some(dot), None) => {
            let (int, rat) = split_at_exclusive(s, dot);
            (int, Some(rat), None)
        }
        // XeZ
        (None, Some(exp)) => {
            let (int, exp) = split_at_exclusive(s, exp);
            (int, None, Some(exp))
        }
        // X.YeZ
        (Some(dot), Some(exp)) => {
            if exp < dot {
                return Err(LitError::InvalidRational);
            }
            let (int, rest) = split_at_exclusive(s, dot);
            let (rat, exp) = split_at_exclusive(rest, exp - dot - 1);
            (int, Some(rat), Some(exp))
        }
    };

    if cfg!(debug_assertions) {
        let mut reconstructed = String::from(int);
        if let Some(rat) = rat {
            reconstructed.push('.');
            reconstructed.push_str(rat);
        }
        if let Some(exp) = exp {
            let e = if s.contains('E') { 'E' } else { 'e' };
            reconstructed.push(e);
            reconstructed.push_str(exp);
        }
        assert_eq!(reconstructed, s, "{int:?} + {rat:?} + {exp:?}");
    }

    // `int` is allowed to be empty: `.1e1` is the same as `0.1e1`.
    if int.is_empty() {
        int = "0";
    }
    if rat.is_some_and(str::is_empty) {
        return Err(LitError::EmptyRational);
    }
    if exp.is_some_and(str::is_empty) {
        return Err(LitError::EmptyExponent);
    }

    if int.starts_with('0') && int.len() > 1 {
        return Err(LitError::IntegerLeadingZeros);
    }
    // NOTE: leading zeros are allowed in the rational and exponent parts.

    let rat = rat.map(|rat| rat.trim_end_matches('0'));

    let int = match rat {
        Some(rat) => {
            let s = [int, rat].concat();
            big_int_from_str_radix(&s, Base::Decimal, true)
        }
        None => big_int_from_str_radix(int, Base::Decimal, false),
    }?;

    let fract_len = rat.map_or(0, str::len);
    let fract_len = u16::try_from(fract_len).map_err(|_| LitError::RationalTooLarge)?;
    let denominator = BigInt::from(10u64).pow(fract_len as u32);
    let mut number = BigRational::new(int, denominator);

    // 0E... is always zero.
    if number.is_zero() {
        return Ok(LitKind::Number(BigInt::ZERO));
    }

    if let Some(exp) = exp {
        let exp = exp.parse::<i32>().map_err(|e| match *e.kind() {
            std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => {
                LitError::ExponentTooLarge
            }
            _ => LitError::ParseExponent(e),
        })?;
        let exp_abs = exp.unsigned_abs();
        let power = || BigInt::from(10u64).pow(exp_abs);
        if exp.is_negative() {
            if !fits_precision_base_10(&number.denom().abs().into_parts().1, exp_abs) {
                return Err(LitError::ExponentTooLarge);
            }
            number /= power();
        } else if exp > 0 {
            if !fits_precision_base_10(&number.numer().abs().into_parts().1, exp_abs) {
                return Err(LitError::ExponentTooLarge);
            }
            number *= power();
        }
    }

    if number.is_integer() {
        Ok(LitKind::Number(number.to_integer()))
    } else {
        Ok(LitKind::Rational(number))
    }
}

/// Primitive type to use for fast-path parsing.
///
/// If changed, update `max_digits` as well.
type Primitive = u128;

/// Maximum number of bits for a big number.
const MAX_BITS: u32 = 4096;

/// Returns the maximum number of digits in `base` radix that can be represented in `BITS` bits.
///
/// ```python
/// import math
/// def max_digits(bits, base):
///     return math.floor(math.log(2**bits - 1, base)) + 1
/// ```
#[inline]
const fn max_digits<const BITS: u32>(base: Base) -> usize {
    if matches!(base, Base::Binary) {
        return BITS as usize;
    }
    match BITS {
        Primitive::BITS => match base {
            Base::Binary => BITS as usize,
            Base::Octal => 43,
            Base::Decimal => 39,
            Base::Hexadecimal => 33,
        },
        MAX_BITS => match base {
            Base::Binary => BITS as usize,
            Base::Octal => 1366,
            Base::Decimal => 1234,
            Base::Hexadecimal => 1025,
        },
        _ => panic!("unknown bits"),
    }
}

fn big_int_from_str_radix(s: &str, base: Base, rat: bool) -> Result<BigInt, LitError> {
    if s.len() > max_digits::<MAX_BITS>(base) {
        return Err(if rat { LitError::RationalTooLarge } else { LitError::IntegerTooLarge });
    }
    if s.len() <= max_digits::<{ Primitive::BITS }>(base)
        && let Ok(n) = Primitive::from_str_radix(s, base as u32)
    {
        return Ok(BigInt::from(n));
    }
    BigInt::from_str_radix(s, base as u32)
        .map_err(|e| if rat { LitError::ParseRational(e) } else { LitError::ParseInteger(e) })
}

/// Checks whether mantissa * (10 ** exp) fits into [`MAX_BITS`] bits.
fn fits_precision_base_10(mantissa: &BigUint, exp: u32) -> bool {
    // https://github.com/ethereum/solidity/blob/14232980e4b39dee72972f3e142db584f0848a16/libsolidity/ast/Types.cpp#L66
    fits_precision_base_x(mantissa, std::f64::consts::LOG2_10, exp)
}

/// Checks whether `mantissa * (X ** exp)` fits into [`MAX_BITS`] bits,
/// where `X` is given indirectly via `log_2_of_base = log2(X)`.
fn fits_precision_base_x(mantissa: &BigUint, log_2_of_base: f64, exp: u32) -> bool {
    // https://github.com/ethereum/solidity/blob/53c4facf4e01d603c21a8544fc3b016229628a16/libsolutil/Numeric.cpp#L25
    if mantissa.is_zero() {
        return true;
    }

    let max = MAX_BITS as u64;
    let bits = mantissa.bits();
    if bits > max {
        return false;
    }
    let bits_needed = bits + f64::floor(log_2_of_base * exp as f64) as u64;
    bits_needed <= max
}

#[track_caller]
fn split_at_exclusive(s: &str, idx: usize) -> (&str, &str) {
    (&s[..idx], &s[idx + 1..])
}

#[inline]
fn strip_underscores(symbol: &Symbol) -> Cow<'_, str> {
    // Do not allocate a new string unless necessary.
    let s = symbol.as_str();
    if s.contains('_') {
        let mut s = s.to_string();
        s.retain(|c| c != '_');
        return Cow::Owned(s);
    }
    Cow::Borrowed(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lexer;
    use alloy_primitives::{Address, address};
    use solar_interface::Session;

    // String literal parsing is tested in ../lexer/mod.rs.

    // Run through the lexer to get the same input that the parser gets.
    #[track_caller]
    fn lex_literal(src: &str, should_fail: bool) -> Symbol {
        let sess = Session::builder().with_silent_emitter(None).build();
        let tokens = Lexer::new(&sess, src).into_tokens();
        assert_eq!(tokens.len(), 1, "expected exactly 1 token: {tokens:?}");
        assert_eq!(sess.dcx.has_errors().is_err(), should_fail, "{src:?} -> {tokens:?}");
        tokens[0].lit().expect("not a literal").symbol
    }

    #[test]
    fn integer() {
        use LitError::*;

        #[track_caller]
        fn check_int(src: &str, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src, false);
            let res = match parse_integer(symbol) {
                Ok(LitKind::Number(n)) => Ok(n),
                Ok(x) => panic!("not a number: {x:?} ({src:?})"),
                Err(e) => Err(e),
            };
            let expected = match expected {
                Ok(s) => Ok(BigInt::from_str_radix(s, 10).unwrap()),
                Err(e) => Err(e),
            };
            assert_eq!(res, expected, "{src:?}");
        }

        #[track_caller]
        fn check_address(src: &str, expected: Result<Address, &str>) {
            let symbol = lex_literal(src, false);
            match expected {
                Ok(address) => match parse_integer(symbol) {
                    Ok(LitKind::Address(a)) => assert_eq!(a, address, "{src:?}"),
                    e => panic!("not an address: {e:?} ({src:?})"),
                },
                Err(int) => match parse_integer(symbol) {
                    Ok(LitKind::Number(n)) => {
                        assert_eq!(n, BigInt::from_str_radix(int, 10).unwrap(), "{src:?}")
                    }
                    e => panic!("not an integer: {e:?} ({src:?})"),
                },
            }
        }

        solar_interface::enter(|| {
            check_int("00", Err(IntegerLeadingZeros));
            check_int("01", Err(IntegerLeadingZeros));
            check_int("00", Err(IntegerLeadingZeros));
            check_int("001", Err(IntegerLeadingZeros));
            check_int("000", Err(IntegerLeadingZeros));
            check_int("0001", Err(IntegerLeadingZeros));

            check_int("0", Ok("0"));
            check_int("1", Ok("1"));

            // check("0b10", Ok("2"));
            // check("0o10", Ok("8"));
            check_int("10", Ok("10"));
            check_int("0x10", Ok("16"));

            check_address("0x00000000000000000000000000000000000000", Err("0"));
            check_address("0x000000000000000000000000000000000000000", Err("0"));
            check_address("0x0000000000000000000000000000000000000000", Ok(Address::ZERO));
            check_address("0x00000000000000000000000000000000000000000", Err("0"));
            check_address("0x000000000000000000000000000000000000000000", Err("0"));
            check_address(
                "0x0000000000000000000000000000000000000001",
                Ok(Address::with_last_byte(1)),
            );

            check_address(
                "0x52908400098527886E0F7030069857D2E4169EE7",
                Ok(address!("52908400098527886E0F7030069857D2E4169EE7")),
            );
            check_address(
                "0x52908400098527886E0F7030069857D2E4169Ee7",
                Ok(address!("52908400098527886E0F7030069857D2E4169EE7")),
            );

            check_address(
                "0x8617E340B3D01FA5F11F306F4090FD50E238070D",
                Ok(address!("8617E340B3D01FA5F11F306F4090FD50E238070D")),
            );
            check_address(
                "0xde709f2102306220921060314715629080e2fb77",
                Ok(address!("de709f2102306220921060314715629080e2fb77")),
            );
            check_address(
                "0x27b1fdb04752bbc536007a920d24acb045561c26",
                Ok(address!("27b1fdb04752bbc536007a920d24acb045561c26")),
            );
            check_address(
                "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
                Ok(address!("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed")),
            );
            check_address(
                "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
                Ok(address!("fB6916095ca1df60bB79Ce92cE3Ea74c37c5d359")),
            );
            check_address(
                "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
                Ok(address!("dbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB")),
            );
            check_address(
                "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
                Ok(address!("D1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb")),
            );
        });
    }

    #[test]
    fn rational() {
        use LitError::*;

        #[track_caller]
        fn check_int_full(src: &str, should_fail_lexing: bool, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src, should_fail_lexing);
            let res = match parse_rational(symbol) {
                Ok(LitKind::Number(r)) => Ok(r),
                Ok(x) => panic!("not a number: {x:?} ({src:?})"),
                Err(e) => Err(e),
            };
            let expected = match expected {
                Ok(s) => Ok(BigInt::from_str_radix(s, 10).unwrap()),
                Err(e) => Err(e),
            };
            assert_eq!(res, expected, "{src:?}");
        }

        #[track_caller]
        fn check_int(src: &str, expected: Result<&str, LitError>) {
            check_int_full(src, false, expected);
        }

        #[track_caller]
        fn check_rat(src: &str, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src, false);
            let res = match parse_rational(symbol) {
                Ok(LitKind::Rational(r)) => Ok(r),
                Ok(x) => panic!("not a number: {x:?} ({src:?})"),
                Err(e) => Err(e),
            };
            let expected = match expected {
                Ok(s) => Ok(BigRational::from_str_radix(s, 10).unwrap()),
                Err(e) => Err(e),
            };
            assert_eq!(res, expected, "{src:?}");
        }

        #[track_caller]
        fn zeros(before: &str, zeros: usize) -> String {
            before.to_string() + &"0".repeat(zeros)
        }

        solar_interface::enter(|| {
            check_int("00", Err(IntegerLeadingZeros));
            check_int("0_0", Err(IntegerLeadingZeros));
            check_int("01", Err(IntegerLeadingZeros));
            check_int("0_1", Err(IntegerLeadingZeros));
            check_int("00", Err(IntegerLeadingZeros));
            check_int("001", Err(IntegerLeadingZeros));
            check_int("000", Err(IntegerLeadingZeros));
            check_int("0001", Err(IntegerLeadingZeros));

            check_int("0.", Err(EmptyRational));

            check_int("0", Ok("0"));
            check_int("0e0", Ok("0"));
            check_int("0.0", Ok("0"));
            check_int("0.00", Ok("0"));
            check_int("0.0e0", Ok("0"));
            check_int("0.00e0", Ok("0"));
            check_int("0.0e00", Ok("0"));
            check_int("0.00e00", Ok("0"));
            check_int("0.0e-0", Ok("0"));
            check_int("0.00e-0", Ok("0"));
            check_int("0.0e-00", Ok("0"));
            check_int("0.00e-00", Ok("0"));
            check_int("0.0e1", Ok("0"));
            check_int("0.00e1", Ok("0"));
            check_int("0.00e01", Ok("0"));
            check_int("0e999999", Ok("0"));
            check_int("0E123456789", Ok("0"));

            check_int(".0", Ok("0"));
            check_int(".00", Ok("0"));
            check_int(".0e0", Ok("0"));
            check_int(".00e0", Ok("0"));
            check_int(".0e00", Ok("0"));
            check_int(".00e00", Ok("0"));
            check_int(".0e-0", Ok("0"));
            check_int(".00e-0", Ok("0"));
            check_int(".0e-00", Ok("0"));
            check_int(".00e-00", Ok("0"));
            check_int(".0e1", Ok("0"));
            check_int(".00e1", Ok("0"));
            check_int(".00e01", Ok("0"));

            check_int("1", Ok("1"));
            check_int("1e0", Ok("1"));
            check_int("1.0", Ok("1"));
            check_int("1.00", Ok("1"));
            check_int("1.0e0", Ok("1"));
            check_int("1.00e0", Ok("1"));
            check_int("1.0e00", Ok("1"));
            check_int("1.00e00", Ok("1"));
            check_int("1.0e-0", Ok("1"));
            check_int("1.00e-0", Ok("1"));
            check_int("1.0e-00", Ok("1"));
            check_int("1.00e-00", Ok("1"));

            check_int_full("0b", true, Err(InvalidRational));
            check_int_full("0b0", true, Err(InvalidRational));
            check_int_full("0b01", true, Err(InvalidRational));
            check_int_full("0b01.0", true, Err(InvalidRational));
            check_int_full("0b01.0e1", true, Err(InvalidRational));
            check_int_full("0b0e", true, Err(InvalidRational));
            // check_int_full("0b0e.0", true, Err(InvalidRational));
            // check_int_full("0b0e.0e1", true, Err(InvalidRational));

            check_int_full("0o", true, Err(InvalidRational));
            check_int_full("0o0", true, Err(InvalidRational));
            check_int_full("0o01", true, Err(InvalidRational));
            check_int_full("0o01.0", true, Err(InvalidRational));
            check_int_full("0o01.0e1", true, Err(InvalidRational));
            check_int_full("0o0e", true, Err(InvalidRational));
            // check_int_full("0o0e.0", true, Err(InvalidRational));
            // check_int_full("0o0e.0e1", true, Err(InvalidRational));

            check_int_full("0x", true, Err(InvalidRational));
            check_int_full("0x0", false, Err(InvalidRational));
            check_int_full("0x01", false, Err(InvalidRational));
            check_int_full("0x01.0", true, Err(InvalidRational));
            check_int_full("0x01.0e1", true, Err(InvalidRational));
            check_int_full("0x0e", false, Err(InvalidRational));
            check_int_full("0x0e.0", true, Err(InvalidRational));
            check_int_full("0x0e.0e1", true, Err(InvalidRational));

            check_int("1e1", Ok("10"));
            check_int("1.0e1", Ok("10"));
            check_int("1.00e1", Ok("10"));
            check_int("1.00e01", Ok("10"));

            check_int("1.1e1", Ok("11"));
            check_int("1.10e1", Ok("11"));
            check_int("1.100e1", Ok("11"));
            check_int("1.2e1", Ok("12"));
            check_int("1.200e1", Ok("12"));

            check_int("1e10", Ok(&zeros("1", 10)));
            check_int("1.0e10", Ok(&zeros("1", 10)));
            check_int("1.1e10", Ok(&zeros("11", 9)));
            check_int("10e-1", Ok("1"));
            check_int("1E1233", Ok(&zeros("1", 1233)));
            check_int("1E1234", Err(LitError::ExponentTooLarge));

            check_rat("1e-1", Ok("1/10"));
            check_rat("1e-2", Ok("1/100"));
            check_rat("1e-3", Ok("1/1000"));
            check_rat("1.0e-1", Ok("1/10"));
            check_rat("1.0e-2", Ok("1/100"));
            check_rat("1.0e-3", Ok("1/1000"));
            check_rat("1.1e-1", Ok("11/100"));
            check_rat("1.1e-2", Ok("11/1000"));
            check_rat("1.1e-3", Ok("11/10000"));

            check_rat("1.1", Ok("11/10"));
            check_rat("1.10", Ok("11/10"));
            check_rat("1.100", Ok("11/10"));
            check_rat("1.2", Ok("12/10"));
            check_rat("1.20", Ok("12/10"));
        });
    }
}
