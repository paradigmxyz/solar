use super::item::FunctionFlags;
use crate::{PResult, Parser};
use solar_ast::{token::*, *};
use solar_interface::kw;
use std::{fmt, ops::RangeInclusive};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a type.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_type(&mut self) -> PResult<'sess, Type<'ast>> {
        let mut ty = self
            .parse_spanned(Self::parse_basic_ty_kind)
            .map(|(span, kind)| Type { span, kind })?;

        // Parse suffixes.
        while self.eat(TokenKind::OpenDelim(Delimiter::Bracket)) {
            let size = if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Bracket)) {
                None
            } else {
                Some(self.parse_expr()?)
            };
            self.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
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
                    span: _,
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
    ///
    /// Must be used after checking that the next token is an elementary type.
    pub(super) fn parse_elementary_type(&mut self) -> PResult<'sess, ElementaryType> {
        let id = self.parse_ident_any()?;
        debug_assert!(id.is_elementary_type());
        let mut ty = match id.name {
            kw::Address => ElementaryType::Address(false),
            kw::Bool => ElementaryType::Bool,
            kw::String => ElementaryType::String,
            kw::Bytes => ElementaryType::Bytes,
            kw::Fixed => ElementaryType::Fixed(TypeSize::ZERO, TypeFixedSize::ZERO),
            kw::UFixed => ElementaryType::UFixed(TypeSize::ZERO, TypeFixedSize::ZERO),
            kw::Int => ElementaryType::Int(TypeSize::ZERO),
            kw::UInt => ElementaryType::UInt(TypeSize::ZERO),
            s if s >= kw::UInt8 && s <= kw::UInt256 => {
                let bytes = s.as_u32() - kw::UInt8.as_u32() + 1;
                ElementaryType::UInt(TypeSize::new(bytes as u8).unwrap())
            }
            s if s >= kw::Int8 && s <= kw::Int256 => {
                let bytes = s.as_u32() - kw::Int8.as_u32() + 1;
                ElementaryType::Int(TypeSize::new(bytes as u8).unwrap())
            }
            s if s >= kw::Bytes1 && s <= kw::Bytes32 => {
                let bytes = s.as_u32() - kw::Bytes1.as_u32() + 1;
                ElementaryType::FixedBytes(TypeSize::new(bytes as u8).unwrap())
            }
            s => unreachable!("unexpected elementary type: {s}"),
        };

        let sm = self.parse_state_mutability();
        match (&mut ty, sm) {
            (ElementaryType::Address(p), Some(StateMutability::Payable)) => *p = true,
            (_, None) => {}
            (_, Some(_)) => {
                let msg = if matches!(ty, ElementaryType::Address(_)) {
                    "address types can only be payable or non-payable"
                } else {
                    "only address types can have state mutability"
                };
                self.dcx().err(msg).span(id.span.to(self.prev_token.span)).emit();
            }
        }

        // TODO: Move to type checking.
        // if matches!(ty, ElementaryType::Fixed(..) | ElementaryType::UFixed(..)) {
        //     self.dcx().err("`fixed` types are not yet supported").span(id.span).emit();
        // }

        Ok(ty)
    }

    /// Parses a mapping type.
    fn parse_mapping_type(&mut self) -> PResult<'sess, TypeMapping<'ast>> {
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;

        let key = self.parse_type()?;
        // TODO: Move to type checking.
        if !key.is_elementary() && !key.is_custom() {
            let msg =
                "only elementary types or used-defined types can be used as key types in mappings";
            self.dcx().err(msg).span(key.span).emit();
        }
        let key_name = self.parse_ident_opt()?;

        self.expect(TokenKind::FatArrow)?;

        let value = self.parse_type()?;
        let value_name = self.parse_ident_opt()?;

        self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;

        Ok(TypeMapping { key, key_name, value, value_name })
    }
}

#[derive(Debug, PartialEq)]
enum ParseTySizeError {
    Parse(std::num::ParseIntError),
    TryFrom(std::num::TryFromIntError),
    NotMultipleOf8,
    OutOfRange(RangeInclusive<u16>),
    FixedX,
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
            Self::FixedX => f.write_str("`fixed` sizes must be separated by exactly one 'x'"),
        }
    }
}

/// Parses `fixedMxN` or `ufixedMxN`.
#[allow(dead_code)]
fn parse_fixed_type(original: &str) -> Result<Option<ElementaryType>, ParseTySizeError> {
    let s = original;
    let tmp = s.strip_prefix('u');
    let unsigned = tmp.is_some();
    let s = tmp.unwrap_or(s);

    if let Some(s) = s.strip_prefix("fixed") {
        debug_assert!(!s.is_empty());
        let (m, n) = parse_fixed_size(s)?;
        return Ok(Some(if unsigned {
            ElementaryType::UFixed(m, n)
        } else {
            ElementaryType::Fixed(m, n)
        }));
    }

    Ok(None)
}

#[allow(dead_code)]
fn parse_fb_size(s: &str) -> Result<TypeSize, ParseTySizeError> {
    parse_ty_size_u8(s, 1..=32, false).map(|x| TypeSize::new(x).unwrap())
}

#[allow(dead_code)]
fn parse_int_size(s: &str) -> Result<TypeSize, ParseTySizeError> {
    parse_ty_size_u8(s, 1..=32, true).map(|x| TypeSize::new(x).unwrap())
}

#[allow(dead_code)]
fn parse_fixed_size(s: &str) -> Result<(TypeSize, TypeFixedSize), ParseTySizeError> {
    let (m, n) = s.split_once('x').ok_or(ParseTySizeError::FixedX)?;
    let m = parse_int_size(m)?;
    let n = parse_ty_size_u8(n, 0..=80, false)?;
    let n = TypeFixedSize::new(n).unwrap();
    Ok((m, n))
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
        if !n.is_multiple_of(8) {
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
