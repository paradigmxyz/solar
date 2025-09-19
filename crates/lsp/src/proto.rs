use crop::Rope;
use lsp_types::{DiagnosticSeverity, NumberOrString};
use solar_interface::{
    CharPos, SourceMap, Span,
    diagnostics::{Diag, Level},
    source_map::SpanLoc,
};

use crate::vfs::{self, VfsPath};

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
    let start_line = if range.start.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.start.line as usize)
    };
    let start = rope.byte_of_utf16_code_unit(start_line + range.start.character as usize);
    let end_line = if range.end.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.end.line as usize)
    };
    let end = rope.byte_of_utf16_code_unit(end_line + range.end.character as usize);

    start..end
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
            code: diag.code.as_ref().map(|id| NumberOrString::String(id.as_string())),
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

fn span_to_location(source_map: &SourceMap, span: Span) -> Option<lsp_types::Location> {
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
        Level::FailureNote | Level::Fatal | Level::Bug | Level::Error => DiagnosticSeverity::ERROR,
        Level::Warning => DiagnosticSeverity::WARNING,
        Level::Help | Level::OnceHelp => DiagnosticSeverity::HINT,
        Level::Note | Level::OnceNote | Level::Allow => DiagnosticSeverity::INFORMATION,
    }
}
