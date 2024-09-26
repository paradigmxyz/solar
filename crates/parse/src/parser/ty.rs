use super::item::FunctionFlags;
use crate::{PResult, Parser};
use std::{fmt, ops::RangeInclusive};
use sulk_ast::{ast::*, token::*};
use sulk_interface::kw;

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a type.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_type(&mut self) -> PResult<'sess, Type<'ast>> {
        let mut ty = self
            .parse_spanned(Self::parse_basic_ty_kind)
            .map(|(span, kind)| Type { span, kind })?;

        // Parse suffixes.
        while self.eat(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            let size = if self.check_noexpect(&TokenKind::CloseDelim(Delimiter::Bracket)) {
                None
            } else {
                Some(self.parse_expr()?)
            };
            self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?;
            ty = Type {
                span: ty.span.to(self.prev_token.span),
                kind: TypeKind::Array(self.alloc(TypeArray { element: ty, size })),
            };
        }

        Ok(ty)
    }

    /// Parses a type kind. Does not parse suffixes.
    fn parse_basic_ty_kind(&mut self) -> PResult<'sess, TypeKind<'ast>> {
        if self.check_elementary_type() {
            self.parse_elementary_type().map(TypeKind::Elementary)
        } else if self.eat_keyword(kw::Function) {
            self.parse_function_header(FunctionFlags::FUNCTION_TY).map(|f| {
                let FunctionHeader {
                    name: _,
                    parameters,
                    visibility,
                    state_mutability,
                    modifiers: _,
                    virtual_: _,
                    override_: _,
                    returns,
                } = f;
                TypeKind::Function(self.alloc(TypeFunction {
                    parameters,
                    visibility,
                    state_mutability,
                    returns,
                }))
            })
        } else if self.eat_keyword(kw::Mapping) {
            self.parse_mapping_type().map(|x| TypeKind::Mapping(self.alloc(x)))
        } else if self.check_path() {
            self.parse_path().map(TypeKind::Custom)
        } else {
            self.unexpected()
        }
    }

    /// Parses an elementary type.
    pub(super) fn parse_elementary_type(&mut self) -> PResult<'sess, ElementaryType> {
        let id = self.parse_ident_any()?;
        Ok(match id.name {
            kw::Address => ElementaryType::Address(match self.parse_state_mutability() {
                Some(StateMutability::Payable) => true,
                None => false,
                _ => {
                    let msg = "address types can only be payable or non-payable";
                    self.dcx().err(msg).span(id.span.to(self.prev_token.span)).emit();
                    false
                }
            }),
            kw::Bool => ElementaryType::Bool,
            kw::String => ElementaryType::String,
            kw::Bytes => ElementaryType::Bytes,
            kw::Fixed => ElementaryType::Fixed(TypeSize::ZERO, TypeFixedSize::ZERO),
            kw::UFixed => ElementaryType::UFixed(TypeSize::ZERO, TypeFixedSize::ZERO),
            kw::Int => ElementaryType::Int(TypeSize::ZERO),
            kw::UInt => ElementaryType::UInt(TypeSize::ZERO),
            s => self.parse_dynamic_elementary_type(s.as_str()).map_err(|e| e.span(id.span))?,
        })
    }

    /// Parses `intN`, `uintN`, `bytesN`, `fixedMxN`, or `ufixedMxN`.
    fn parse_dynamic_elementary_type(&mut self, original: &str) -> PResult<'sess, ElementaryType> {
        let s = original;
        if let Some(s) = s.strip_prefix("bytes") {
            debug_assert!(!s.is_empty());
            return Ok(ElementaryType::FixedBytes(self.parse_fb_size(s)?));
        }

        let tmp = s.strip_prefix('u');
        let unsigned = tmp.is_some();
        let s = tmp.unwrap_or(s);

        if let Some(s) = s.strip_prefix("int") {
            debug_assert!(!s.is_empty());
            let size = self.parse_int_size(s)?;
            return Ok(if unsigned {
                ElementaryType::UInt(size)
            } else {
                ElementaryType::Int(size)
            });
        }

        if let Some(s) = s.strip_prefix("fixed") {
            debug_assert!(!s.is_empty());
            let (m, n) = self.parse_fixed_size(s)?;
            return Ok(if unsigned {
                ElementaryType::UFixed(m, n)
            } else {
                ElementaryType::Fixed(m, n)
            });
        }

        unreachable!("unexpected elementary type: {original:?}");
    }

    fn parse_fb_size(&mut self, s: &str) -> PResult<'sess, TypeSize> {
        self.parse_ty_size_u8(s, 1..=32, false).map(|x| TypeSize::new(x).unwrap())
    }

    fn parse_int_size(&mut self, s: &str) -> PResult<'sess, TypeSize> {
        self.parse_ty_size_u8(s, 1..=32, true).map(|x| TypeSize::new(x).unwrap())
    }

    fn parse_fixed_size(&mut self, s: &str) -> PResult<'sess, (TypeSize, TypeFixedSize)> {
        let (m, n) = s
            .split_once('x')
            .ok_or_else(|| self.dcx().err("`fixed` sizes must be separated by exactly one 'x'"))?;
        let m = self.parse_int_size(m)?;
        let n = self.parse_ty_size_u8(n, 0..=80, false)?;
        let n = TypeFixedSize::new(n).unwrap();
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
    ) -> PResult<'sess, u8> {
        parse_ty_size_u8(s, range, to_bytes).map_err(|e| self.dcx().err(e.to_string()))
    }

    /// Parses a mapping type.
    fn parse_mapping_type(&mut self) -> PResult<'sess, TypeMapping<'ast>> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;

        let key = self.parse_type()?;
        if !key.is_elementary() && !key.is_custom() {
            let msg =
                "only elementary types or used-defined types can be used as key types in mappings";
            self.dcx().err(msg).span(key.span).emit();
        }
        let key_name = self.parse_ident_opt()?;

        self.expect(&TokenKind::FatArrow)?;

        let value = self.parse_type()?;
        let value_name = self.parse_ident_opt()?;

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
