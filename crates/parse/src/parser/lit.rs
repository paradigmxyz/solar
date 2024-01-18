use crate::{unescape, PErr, PResult, Parser};
use alloy_primitives::Address;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Num;
use std::fmt;
use sulk_ast::{ast::*, token::*};
use sulk_interface::{kw, Symbol};

impl<'a> Parser<'a> {
    /// Parses a literal.
    pub fn parse_lit(&mut self) -> PResult<'a, Lit> {
        self.parse_spanned(Self::parse_lit_inner).map(|(span, (symbol, kind))| Lit {
            span,
            symbol,
            kind,
        })
    }

    /// Parses a literal with an optional subdenomination.
    ///
    /// Note that the subdenomination gets applied to the literal directly, and is returned just for
    /// display reasons.
    ///
    /// Returns None if no subdenomination was parsed or if the literal is not a number or rational.
    pub fn parse_lit_with_subdenomination(
        &mut self,
    ) -> PResult<'a, (Lit, Option<SubDenomination>)> {
        let mut lit = self.parse_lit()?;
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
        let sub = match self.token.ident()?.name {
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
        };
        if sub.is_some() {
            self.bump();
        }
        sub
    }

    /// Emits an error if a subdenomination was parsed.
    pub(super) fn expect_no_subdenomination(&mut self) {
        if let Some(_sub) = self.parse_subdenomination() {
            self.no_subdenomination_error().emit();
        }
    }

    pub(super) fn no_subdenomination_error(&mut self) -> PErr<'a> {
        let span = self.prev_token.span;
        self.dcx().err("subdenominations aren't allowed here").span(span)
    }

    fn parse_lit_inner(&mut self) -> PResult<'a, (Symbol, LitKind)> {
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
        let kind = match lit.kind {
            TokenLitKind::Integer => self.parse_lit_int(lit.symbol),
            TokenLitKind::Rational => self.parse_lit_rational(lit.symbol),
            TokenLitKind::Str | TokenLitKind::UnicodeStr | TokenLitKind::HexStr => {
                self.parse_lit_str(lit)
            }
            TokenLitKind::Err => Ok(LitKind::Err),
        };
        kind.map(|kind| (lit.symbol, kind))
    }

    /// Parses an integer literal.
    fn parse_lit_int(&mut self, symbol: Symbol) -> PResult<'a, LitKind> {
        use LitError::*;
        match parse_integer(symbol) {
            Ok(l) => Ok(l),
            // User error.
            Err(e @ IntegerLeadingZeros) => Err(self.dcx().err(e.to_string())),
            // User error, but already emitted.
            Err(EmptyInteger) => Ok(LitKind::Err),
            // Lexer internal error.
            Err(e @ ParseInteger(_)) => panic!("failed to parse integer literal {symbol:?}: {e}"),
            // Should never happen.
            Err(
                e @ (EmptyRational | EmptyExponent | ParseRational(_) | ParseExponent(_)
                | RationalTooLarge | ExponentTooLarge),
            ) => panic!("this error shouldn't happen for normal integer literals: {e}"),
        }
    }

    /// Parses a rational literal.
    fn parse_lit_rational(&mut self, symbol: Symbol) -> PResult<'a, LitKind> {
        use LitError::*;
        match parse_rational(symbol) {
            Ok(l) => Ok(l),
            // User error.
            Err(e @ (IntegerLeadingZeros | RationalTooLarge | ExponentTooLarge)) => {
                Err(self.dcx().err(e.to_string()))
            }
            // User error, but already emitted.
            Err(EmptyExponent | EmptyInteger | EmptyRational) => Ok(LitKind::Err),
            // Lexer internal error.
            Err(e @ (ParseExponent(_) | ParseInteger(_) | ParseRational(_))) => {
                panic!("failed to parse rational literal {symbol:?}: {e}")
            }
        }
    }

    /// Parses a string literal.
    fn parse_lit_str(&mut self, lit: TokenLit) -> PResult<'a, LitKind> {
        let mode = match lit.kind {
            TokenLitKind::Str => unescape::Mode::Str,
            TokenLitKind::UnicodeStr => unescape::Mode::UnicodeStr,
            TokenLitKind::HexStr => unescape::Mode::HexStr,
            _ => unreachable!(),
        };
        let unescape = |s: Symbol| unescape::parse_literal(s.as_str(), mode, |_, _| {});

        let mut value = unescape(lit.symbol);
        while let Some(TokenLit { symbol, kind }) = self.token.lit() {
            if kind != lit.kind {
                break;
            }
            value.append(&mut unescape(symbol));
            self.bump();
        }

        let kind = match lit.kind {
            TokenLitKind::Str => StrKind::Str,
            TokenLitKind::UnicodeStr => StrKind::Unicode,
            TokenLitKind::HexStr => StrKind::Hex,
            _ => unreachable!(),
        };
        Ok(LitKind::Str(kind, value.into()))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LitError {
    EmptyInteger,
    EmptyRational,
    EmptyExponent,

    ParseInteger(num_bigint::ParseBigIntError),
    ParseRational(num_bigint::ParseBigIntError),
    ParseExponent(num_bigint::ParseBigIntError),

    RationalTooLarge,
    ExponentTooLarge,
    IntegerLeadingZeros,
}

impl fmt::Display for LitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInteger => write!(f, "empty integer"),
            Self::EmptyRational => write!(f, "empty rational"),
            Self::EmptyExponent => write!(f, "empty exponent"),
            Self::ParseInteger(e) => write!(f, "failed to parse integer: {e}"),
            Self::ParseRational(e) => write!(f, "failed to parse rational: {e}"),
            Self::ParseExponent(e) => write!(f, "failed to parse exponent: {e}"),
            Self::RationalTooLarge => write!(f, "rational part too large"),
            Self::ExponentTooLarge => write!(f, "exponent too large"),
            Self::IntegerLeadingZeros => write!(f, "leading zeros are not allowed in integers"),
        }
    }
}

fn parse_integer(symbol: Symbol) -> Result<LitKind, LitError> {
    let symbol = strip_underscores(symbol);
    let s = symbol.as_str();
    let base = match s.as_bytes() {
        [b'0', b'x', ..] => 16,
        [b'0', b'o', ..] => 8,
        [b'0', b'b', ..] => 2,
        _ => 10,
    };

    if base == 10 && s.starts_with('0') && s.len() > 1 {
        return Err(LitError::IntegerLeadingZeros);
    }

    // Address literal.
    if base == 16 && s.len() == 42 {
        match Address::parse_checksummed(s, None) {
            Ok(address) => return Ok(LitKind::Address(address)),
            // Continue parsing as a number to emit better errors.
            Err(alloy_primitives::AddressError::InvalidChecksum) => {}
            Err(alloy_primitives::AddressError::Hex(_)) => {}
        }
    }

    let start = if base == 10 { 0 } else { 2 };
    let s = &s[start..];
    if s.is_empty() {
        return Err(LitError::EmptyInteger);
    }
    BigInt::from_str_radix(s, base).map(LitKind::Number).map_err(LitError::ParseInteger)
}

fn parse_rational(symbol: Symbol) -> Result<LitKind, LitError> {
    let symbol = strip_underscores(symbol);
    let s = symbol.as_str();
    debug_assert!(!s.is_empty());

    let (int, rat, exp) = match (s.find('.'), s.find('e').or(s.find('E'))) {
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
            debug_assert!(exp > dot);
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

    if int.is_empty() {
        return Err(LitError::EmptyInteger);
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
            BigInt::from_str_radix(&s, 10).map_err(LitError::ParseRational)
        }
        None => BigInt::from_str_radix(int, 10).map_err(LitError::ParseInteger),
    }?;

    let fract_len = rat.map_or(0, str::len);
    let fract_len = u16::try_from(fract_len).map_err(|_| LitError::RationalTooLarge)?;
    let denominator = BigInt::from(10u64).pow(fract_len as u32);
    let mut number = BigRational::new(int, denominator);

    if let Some(exp) = exp {
        let exp = BigInt::from_str_radix(exp, 10).map_err(LitError::ParseExponent)?;
        let exp = i16::try_from(exp).map_err(|_| LitError::ExponentTooLarge)?;
        // NOTE: Calculating exponents greater than i16 might perform better with a manual loop.
        let ten = BigInt::from(10u64);
        if exp.is_negative() {
            number /= ten.pow((-exp) as u32);
        } else {
            number *= ten.pow(exp as u32);
        }
    }

    if number.is_integer() {
        Ok(LitKind::Number(number.to_integer()))
    } else {
        Ok(LitKind::Rational(number))
    }
}

#[track_caller]
fn split_at_exclusive(s: &str, idx: usize) -> (&str, &str) {
    if !s.is_char_boundary(idx) || !s.is_char_boundary(idx + 1) {
        panic!();
    }
    unsafe { (s.get_unchecked(..idx), s.get_unchecked(idx + 1..)) }
}

fn strip_underscores(symbol: Symbol) -> Symbol {
    // Do not allocate a new string unless necessary.
    let s = symbol.as_str();
    if s.contains('_') {
        let mut s = s.to_string();
        s.retain(|c| c != '_');
        return Symbol::intern(&s);
    }
    symbol
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lexer;
    use alloy_primitives::address;
    use num_rational::BigRational;
    use sulk_interface::Session;

    // String literal parsing is tested in ../lexer/mod.rs.

    // Run through the lexer to get the same input that the parser gets.
    #[track_caller]
    fn lex_literal(src: &str) -> Symbol {
        let sess = Session::with_test_emitter();
        let tokens = Lexer::new(&sess, src).into_tokens();
        sess.dcx.has_errors().unwrap();
        assert_eq!(tokens.len(), 1, "{tokens:?}");
        tokens[0].lit().expect("not a literal").symbol
    }

    #[test]
    fn integer() {
        use LitError::*;

        #[track_caller]
        fn check_int(src: &str, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src);
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
            let symbol = lex_literal(src);
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

        sulk_interface::enter(|| {
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
                Err("471360049350540672339372329809862569580528312039"),
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
        })
        .unwrap();
    }

    #[test]
    fn rational() {
        use LitError::*;

        #[track_caller]
        fn check_int(src: &str, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src);
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
        fn check_rat(src: &str, expected: Result<&str, LitError>) {
            let symbol = lex_literal(src);
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

        sulk_interface::enter(|| {
            check_int("00", Err(IntegerLeadingZeros));
            check_int("0_0", Err(IntegerLeadingZeros));
            check_int("01", Err(IntegerLeadingZeros));
            check_int("0_1", Err(IntegerLeadingZeros));
            check_int("00", Err(IntegerLeadingZeros));
            check_int("001", Err(IntegerLeadingZeros));
            check_int("000", Err(IntegerLeadingZeros));
            check_int("0001", Err(IntegerLeadingZeros));
            check_int("0e999999", Err(ExponentTooLarge));

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

            check_int("1e1", Ok("10"));
            check_int("1.0e1", Ok("10"));
            check_int("1.00e1", Ok("10"));
            check_int("1.00e01", Ok("10"));

            check_int("1.1e1", Ok("11"));
            check_int("1.10e1", Ok("11"));
            check_int("1.100e1", Ok("11"));
            check_int("1.2e1", Ok("12"));
            check_int("1.200e1", Ok("12"));

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
        })
        .unwrap();
    }
}
