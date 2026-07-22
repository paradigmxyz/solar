use crate::vfs::{self, VfsPath};
use crop::Rope;
use lsp_types::{DiagnosticSeverity, NumberOrString};
use solar_interface::{
    CharPos, SourceMap, Span,
    diagnostics::{Diag, Level},
    source_map::SpanLoc,
};

pub(crate) fn vfs_path(url: &lsp_types::Url) -> Option<vfs::VfsPath> {
    url.to_file_path().map(VfsPath::from).ok()
}

/// Converts an [`lsp_types::Range`] to a [`Range`].
///
/// This assumes the position encoding in LSP is UTF-16, which is mandatory to support in the LSP
/// spec.
///
/// [`Range`]: std::ops::Range
pub(crate) fn text_range(rope: &Rope, range: lsp_types::Range) -> std::ops::Range<usize> {
    let start_line_byte = if range.start.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.start.line as usize)
    };
    let start_line_utf16 = rope.utf16_code_unit_of_byte(start_line_byte);
    let start = rope.byte_of_utf16_code_unit(start_line_utf16 + range.start.character as usize);
    let end_line_byte = if range.end.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.end.line as usize)
    };
    let end_line_utf16 = rope.utf16_code_unit_of_byte(end_line_byte);
    let end = rope.byte_of_utf16_code_unit(end_line_utf16 + range.end.character as usize);

    start..end
}

/// Converts an LSP UTF-16 range to a byte range, rejecting invalid positions.
pub(crate) fn checked_text_range(
    rope: &Rope,
    range: lsp_types::Range,
) -> Option<std::ops::Range<usize>> {
    let start = checked_byte_position(rope, range.start)?;
    let end = checked_byte_position(rope, range.end)?;
    (start <= end).then_some(start..end)
}

fn checked_byte_position(rope: &Rope, position: lsp_types::Position) -> Option<usize> {
    let line_index = usize::try_from(position.line).ok()?;
    if line_index >= rope.line_len() {
        let is_trailing_line = line_index == rope.line_len()
            && position.character == 0
            && (rope.byte_len() == 0 || rope.byte(rope.byte_len() - 1) == b'\n');
        return is_trailing_line.then_some(rope.byte_len());
    }

    let line_start = rope.byte_of_line(line_index);
    let line = rope.line(line_index);
    let target = usize::try_from(position.character).ok()?;
    let mut utf16 = 0;
    let mut byte = 0;
    for ch in line.chars() {
        if utf16 == target {
            return Some(line_start + byte);
        }
        let next = utf16 + ch.len_utf16();
        if target < next {
            return None;
        }
        utf16 = next;
        byte += ch.len_utf8();
    }
    (utf16 == target).then_some(line_start + byte)
}

/// Converts a byte offset into an LSP UTF-16 position.
pub(crate) fn position_at_byte(rope: &Rope, byte: usize) -> Option<lsp_types::Position> {
    if byte > rope.byte_len() || !rope.is_char_boundary(byte) {
        return None;
    }
    let line = rope.line_of_byte(byte);
    let line_start = rope.byte_of_line(line);
    let character = rope.utf16_code_unit_of_byte(byte) - rope.utf16_code_unit_of_byte(line_start);
    let position =
        lsp_types::Position::new(u32::try_from(line).ok()?, u32::try_from(character).ok()?);
    (checked_byte_position(rope, position) == Some(byte)).then_some(position)
}

// TODO: track `None`s here as they shouldn't happen?
pub(crate) fn diagnostic(
    source_map: &SourceMap,
    diag: &Diag,
) -> Option<(lsp_types::Url, lsp_types::Diagnostic)> {
    let lsp_types::Location { uri, range } =
        span_to_location(source_map, diag.span.primary_span()?)?;
    Some((
        // SAFETY: currently we only use `FileName::Real`
        uri,
        lsp_types::Diagnostic {
            range,
            severity: Some(severity(diag.level())),
            code: diag.code.as_ref().map(|id| NumberOrString::String(id.as_str().to_owned())),
            code_description: None,
            source: Some("solar".into()),
            message: diag.label().into_owned(),
            related_information: Some(
                diag.children
                    .iter()
                    .filter_map(|subdiag| {
                        Some(lsp_types::DiagnosticRelatedInformation {
                            location: span_to_location(source_map, subdiag.span.primary_span()?)?,
                            message: subdiag.label().to_string(),
                        })
                    })
                    .collect(),
            ),
            tags: None,
            data: None,
        },
    ))
}

pub(crate) fn span_to_location(source_map: &SourceMap, span: Span) -> Option<lsp_types::Location> {
    let (file, SpanLoc { lo, hi }) = source_map.span_to_location_info(span);
    let file = file?;

    Some(lsp_types::Location {
        uri: lsp_types::Url::from_file_path(file.name.as_real().unwrap()).ok()?,
        range: lsp_types::Range {
            start: lsp_types::Position {
                line: lo.line as u32 - 1,
                character: utf16_column(lo.col, file.get_line(lo.line - 1)?),
            },
            end: lsp_types::Position {
                line: hi.line as u32 - 1,
                character: utf16_column(hi.col, file.get_line(hi.line - 1)?),
            },
        },
    })
}

/// Takes a UTF8 string slice and a UTF8 character position (relative to the line start), and
/// converts the position to a UTF16 character position.
fn utf16_column(utf8_pos: CharPos, line: &str) -> u32 {
    let mut utf16_codepoints = 0;
    for (idx, char) in line.chars().enumerate() {
        if idx >= utf8_pos.to_usize() {
            break;
        }
        utf16_codepoints += char.len_utf16();
    }

    utf16_codepoints as u32
}

#[inline]
fn severity(level: Level) -> lsp_types::DiagnosticSeverity {
    match level {
        Level::Fatal | Level::Bug | Level::Error => DiagnosticSeverity::ERROR,
        Level::Warning => DiagnosticSeverity::WARNING,
        Level::Help | Level::OnceHelp => DiagnosticSeverity::HINT,
        Level::Note | Level::OnceNote | Level::FailureNote | Level::Allow => {
            DiagnosticSeverity::INFORMATION
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_text_range, position_at_byte};
    use crop::Rope;
    use lsp_types::{Position, Range};

    #[test]
    fn checked_text_range_uses_utf16_columns() {
        let rope = Rope::from("a😀中value\r\n");
        let range = checked_text_range(&rope, Range::new(Position::new(0, 4), Position::new(0, 9)))
            .unwrap();
        assert_eq!(rope.byte_slice(range).to_string(), "value");
    }

    #[test]
    fn checked_text_range_rejects_split_surrogates_and_missing_lines() {
        let rope = Rope::from("😀");
        assert!(
            checked_text_range(&rope, Range::new(Position::new(0, 1), Position::new(0, 2)),)
                .is_none()
        );
        assert!(
            checked_text_range(&rope, Range::new(Position::new(1, 0), Position::new(1, 0)),)
                .is_none()
        );
    }

    #[test]
    fn checked_text_range_rejects_positions_inside_crlf() {
        let rope = Rope::from("value\r\nnext");
        assert!(
            checked_text_range(&rope, Range::new(Position::new(0, 6), Position::new(0, 6)))
                .is_none()
        );
    }

    #[test]
    fn position_at_byte_round_trips_utf16_positions_across_crlf() {
        let rope = Rope::from("a😀中\r\nvalue");
        for position in
            [Position::new(0, 0), Position::new(0, 1), Position::new(0, 3), Position::new(1, 5)]
        {
            let byte = checked_text_range(&rope, Range::new(position, position)).unwrap().start;
            assert_eq!(position_at_byte(&rope, byte), Some(position));
        }
        assert!(position_at_byte(&rope, 2).is_none());
        assert!(position_at_byte(&rope, 9).is_none());
        assert!(position_at_byte(&rope, rope.byte_len() + 1).is_none());
    }

    #[test]
    fn position_conversions_accept_empty_and_trailing_lines() {
        for (source, position) in [
            ("", Position::new(0, 0)),
            ("value\n", Position::new(1, 0)),
            ("value\r\n", Position::new(1, 0)),
        ] {
            let rope = Rope::from(source);
            let range = Range::new(position, position);
            assert_eq!(checked_text_range(&rope, range), Some(rope.byte_len()..rope.byte_len()));
            assert_eq!(position_at_byte(&rope, rope.byte_len()), Some(position));
        }
    }
}
