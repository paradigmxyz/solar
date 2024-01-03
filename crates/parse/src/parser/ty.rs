use super::item::VarDeclMode;
use crate::{PResult, Parser};
use std::{fmt, ops::RangeInclusive};
use sulk_ast::{ast::*, token::*};
use sulk_interface::kw;

impl<'a> Parser<'a> {
    /// Parses a type.
    pub fn parse_type(&mut self) -> PResult<'a, Ty> {
        let mut ty =
            self.parse_spanned(Self::parse_basic_ty_kind).map(|(span, kind)| Ty { span, kind })?;

        // Parse suffixes.
        while self.eat(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            let size = if self.check_noexpect(&TokenKind::CloseDelim(Delimiter::Bracket)) {
                None
            } else {
                Some(self.parse_expr()?)
            };
            self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?;
            ty = Ty {
                span: ty.span.to(self.prev_token.span),
                kind: TyKind::Array(Box::new(TypeArray { element: ty, size })),
            };
        }

        Ok(ty)
    }

    /// Parses a type kind. Does not parse suffixes.
    fn parse_basic_ty_kind(&mut self) -> PResult<'a, TyKind> {
        if self.check_elementary_type() {
            self.parse_elementary_type()
        } else if self.eat_keyword(kw::Function) {
            self.parse_function_type().map(|x| TyKind::Function(Box::new(x)))
        } else if self.eat_keyword(kw::Mapping) {
            self.parse_mapping_type().map(|x| TyKind::Mapping(Box::new(x)))
        } else if self.check_path() {
            self.parse_path().map(TyKind::Custom)
        } else {
            self.unexpected()
        }
    }

    /// Parses an elementary type.
    fn parse_elementary_type(&mut self) -> PResult<'a, TyKind> {
        let id = self.parse_ident_maybe_recover(false)?;
        let kind = match id.name {
            kw::Address => TyKind::Address(self.eat_keyword(kw::Payable)),
            kw::Bool => TyKind::Bool,
            kw::String => TyKind::String,
            kw::Bytes => TyKind::Bytes,
            kw::Fixed => TyKind::Fixed(TySize::ZERO, TyFixedSize::ZERO),
            kw::UFixed => TyKind::UFixed(TySize::ZERO, TyFixedSize::ZERO),
            kw::Int => TyKind::Int(TySize::ZERO),
            kw::UInt => TyKind::UInt(TySize::ZERO),
            s => {
                return self.parse_dynamic_elementary_type(s.as_str()).map_err(|e| e.span(id.span))
            }
        };
        Ok(kind)
    }

    /// Parses `intN`, `uintN`, `bytesN`, `fixedMxN`, or `ufixedMxN`.
    fn parse_dynamic_elementary_type(&mut self, original: &str) -> PResult<'a, TyKind> {
        let s = original;
        if let Some(s) = s.strip_prefix("bytes") {
            debug_assert!(!s.is_empty());
            return Ok(TyKind::FixedBytes(self.parse_fb_size(s)?));
        }

        let tmp = s.strip_prefix('u');
        let unsigned = tmp.is_some();
        let s = tmp.unwrap_or(s);

        if let Some(s) = s.strip_prefix("int") {
            debug_assert!(!s.is_empty());
            let size = self.parse_int_size(s)?;
            return Ok(if unsigned { TyKind::UInt(size) } else { TyKind::Int(size) });
        }

        if let Some(s) = s.strip_prefix("fixed") {
            debug_assert!(!s.is_empty());
            let (m, n) = self.parse_fixed_size(s)?;
            return Ok(if unsigned { TyKind::UFixed(m, n) } else { TyKind::Fixed(m, n) });
        }

        unreachable!("unexpected elementary type: {original:?}");
    }

    fn parse_fb_size(&mut self, s: &str) -> PResult<'a, TySize> {
        self.parse_ty_size_u8(s, 1..=32, false).map(|x| TySize::new(x).unwrap())
    }

    fn parse_int_size(&mut self, s: &str) -> PResult<'a, TySize> {
        self.parse_ty_size_u8(s, 1..=32, true).map(|x| TySize::new(x).unwrap())
    }

    fn parse_fixed_size(&mut self, s: &str) -> PResult<'a, (TySize, TyFixedSize)> {
        let (m, n) = s
            .split_once('x')
            .ok_or_else(|| self.dcx().err("`fixed` sizes must be separated by exactly one 'x'"))?;
        let m = self.parse_int_size(m)?;
        let n = self.parse_ty_size_u8(n, 0..=80, false)?;
        let n = TyFixedSize::new(n).unwrap();
        Ok((m, n))
    }

    /// Parses a type size. The size must be in the range `range`.
    /// If `to_bytes` is true, the size is checked to be a multiple of 8 and then converted from
    /// bits to bytes.
    fn parse_ty_size_u8(
        &mut self,
        s: &str,
        range: RangeInclusive<u8>,
        to_bytes: bool,
    ) -> PResult<'a, u8> {
        parse_ty_size_u8(s, range, to_bytes).map_err(|e| self.dcx().err(e.to_string()))
    }

    /// Parses a function type.
    fn parse_function_type(&mut self) -> PResult<'a, TypeFunction> {
        let parameters = self.parse_function_type_parameter_list()?;
        let visibility = self.parse_visibility();
        let state_mutability = self.parse_state_mutability();
        let returns = if self.eat_keyword(kw::Returns) {
            self.parse_function_type_parameter_list()?
        } else {
            Vec::new()
        };
        Ok(TypeFunction { parameters, visibility, state_mutability, returns })
    }

    fn parse_function_type_parameter_list(&mut self) -> PResult<'a, ParameterList> {
        self.parse_paren_comma_seq(|this| {
            this.parse_variable_declaration(VarDeclMode::AllowStorageWithWarning)
        })
        .map(|(x, _)| x)
    }

    /// Parses a mapping type.
    fn parse_mapping_type(&mut self) -> PResult<'a, TypeMapping> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;

        let key = self.parse_type()?;
        let key_name = if self.check_ident() { Some(self.parse_ident()?) } else { None };

        self.expect(&TokenKind::FatArrow)?;

        let value = self.parse_type()?;
        let value_name = if self.check_ident() { Some(self.parse_ident()?) } else { None };

        self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;

        Ok(TypeMapping { key, key_name, value, value_name })
    }
}

#[derive(Debug, PartialEq)]
enum ParseTySizeError {
    Parse(std::num::ParseIntError),
    TryFrom(std::num::TryFromIntError),
    NotMultipleOf8,
    OutOfRange(RangeInclusive<u16>),
}

impl fmt::Display for ParseTySizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::TryFrom(e) => e.fmt(f),
            Self::NotMultipleOf8 => f.write_str("number must be a multiple of 8"),
            Self::OutOfRange(range) => {
                write!(f, "size is out of range of {}:{} (inclusive)", range.start(), range.end())
            }
        }
    }
}

/// Parses a type size.
///
/// If `to_bytes` is true, the size is checked to be a multiple of 8 and then converted from
/// bits to bytes.
///
/// The final **converted** size must be in the range `range`. This means that if `to_bytes` is
/// true, the range must be in bytes and not bits.
fn parse_ty_size_u8(
    s: &str,
    real_range: RangeInclusive<u8>,
    to_bytes: bool,
) -> Result<u8, ParseTySizeError> {
    let mut n = s.parse::<u16>().map_err(ParseTySizeError::Parse)?;

    if to_bytes {
        if n % 8 != 0 {
            return Err(ParseTySizeError::NotMultipleOf8);
        }
        n /= 8;
    }

    let n = u8::try_from(n).map_err(ParseTySizeError::TryFrom)?;

    if !real_range.contains(&n) {
        let display_range = if to_bytes {
            *real_range.start() as u16 * 8..=*real_range.end() as u16 * 8
        } else {
            *real_range.start() as u16..=*real_range.end() as u16
        };
        return Err(ParseTySizeError::OutOfRange(display_range));
    }

    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size() {
        use ParseTySizeError::*;

        assert_eq!(parse_ty_size_u8("0", 0..=1, false), Ok(0));
        assert_eq!(parse_ty_size_u8("1", 0..=1, false), Ok(1));
        assert_eq!(parse_ty_size_u8("0", 0..=1, true), Ok(0));
        assert_eq!(parse_ty_size_u8("1", 0..=1, true), Err(NotMultipleOf8));
        assert_eq!(parse_ty_size_u8("8", 0..=1, true), Ok(1));

        assert_eq!(parse_ty_size_u8("0", 1..=32, false), Err(OutOfRange(1..=32)));
        assert_eq!(parse_ty_size_u8("0", 1..=32, true), Err(OutOfRange(8..=256)));
        for n in 1..=32 {
            assert_eq!(parse_ty_size_u8(&n.to_string(), 1..=32, false), Ok(n as u8));
            for m in 1..=7u16 {
                assert_eq!(
                    parse_ty_size_u8(&((n - 1) * 8 + m).to_string(), 1..=32, true),
                    Err(NotMultipleOf8)
                );
            }
            assert_eq!(parse_ty_size_u8(&(n * 8).to_string(), 1..=32, true), Ok(n as u8));
        }
    }
}
