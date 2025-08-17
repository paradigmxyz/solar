use crop::Rope;
use lsp_types::{DiagnosticSeverity, NumberOrString};
use solar_interface::{
    SourceMap,
    diagnostics::{Diag, Level},
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

// lookup_byte_offset -> source file + local offset
// look up line map + convert to utf16

// todo: track `None`s here as they shouldn't happen?
pub(crate) fn diagnostic(
    source_map: &SourceMap,
    diag: &Diag,
) -> Option<(lsp_types::Url, lsp_types::Diagnostic)> {
    // todo unwraps
    // todo dont use span_to_loc_info because lsp expects utf16 XD chungus life
    let (file, lo_line, lo_col, hi_line, hi_col) =
        source_map.span_to_location_info(diag.span.primary_span()?);
    let file_url = lsp_types::Url::from_file_path(file?.name.as_real().unwrap()).ok()?;

    Some((
        // SAFETY: currently we only use `FileName::Real`
        file_url,
        lsp_types::Diagnostic {
            // todo helper
            range: lsp_types::Range {
                start: lsp_types::Position { line: lo_line as u32, character: lo_col as u32 },
                end: lsp_types::Position { line: hi_line as u32, character: hi_col as u32 },
            },
            // todo helper
            severity: Some(match diag.level() {
                Level::FailureNote | Level::Fatal | Level::Bug => DiagnosticSeverity::ERROR,
                Level::Error => DiagnosticSeverity::ERROR,
                Level::Warning => DiagnosticSeverity::WARNING,
                Level::Help | Level::OnceHelp => DiagnosticSeverity::HINT,
                Level::Note | Level::OnceNote | Level::Allow => DiagnosticSeverity::INFORMATION,
            }),
            code: diag.code.as_ref().map(|id| NumberOrString::String(id.as_string())),
            code_description: None,
            source: Some("solar".into()),
            message: diag.label().into_owned(),
            // todo subdiags
            related_information: Some(
                diag.children
                    .iter()
                    .filter_map(|subdiag| {
                        let (file, lo_line, lo_col, hi_line, hi_col) =
                            source_map.span_to_location_info(subdiag.span.primary_span().unwrap());
                        let file_url =
                            lsp_types::Url::from_file_path(file?.name.as_real().unwrap()).ok()?;
                        Some(lsp_types::DiagnosticRelatedInformation {
                            // todo unwraps
                            location: lsp_types::Location {
                                uri: file_url,
                                range: lsp_types::Range {
                                    start: lsp_types::Position {
                                        line: lo_line as u32,
                                        character: lo_col as u32,
                                    },
                                    end: lsp_types::Position {
                                        line: hi_line as u32,
                                        character: hi_col as u32,
                                    },
                                },
                            },
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
