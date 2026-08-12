use crate::{
    code_actions::{DiagnosticData, DiagnosticSuggestion},
    vfs::{self, VfsPath},
};
use crop::Rope;
use lsp_types::{
    DiagnosticSeverity, NumberOrString, ServerCapabilities, ServerInfo,
    request::{Initialize as LspInitialize, Request},
};
use solar_config::version::SHORT_VERSION;
use solar_interface::{
    CharPos, SourceMap, Span,
    diagnostics::{Diag, Level},
    source_map::SpanLoc,
};

#[derive(Debug)]
pub(crate) enum Initialize {}

#[derive(Debug, serde::Serialize)]
#[serde(transparent)]
pub(crate) struct InitializeParams {
    inner: lsp_types::InitializeParams,
    #[serde(skip)]
    pull_diagnostic_data_support: bool,
}

impl InitializeParams {
    pub(crate) fn into_inner(self) -> lsp_types::InitializeParams {
        self.inner
    }

    pub(crate) fn pull_diagnostic_data_support(&self) -> bool {
        self.pull_diagnostic_data_support
    }
}

impl<'de> serde::Deserialize<'de> for InitializeParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        let pull_diagnostic_data_support = value
            .pointer("/capabilities/textDocument/diagnostic/dataSupport")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        // LSP 3.17 uses `workspace.diagnostics`, while lsp-types 0.95.1 deserializes its
        // `WorkspaceClientCapabilities::diagnostic` field from the singular spelling. Remove this
        // shim after upgrading to an lsp-types version that accepts the plural wire field.
        if let Some(workspace) =
            value.pointer_mut("/capabilities/workspace").and_then(serde_json::Value::as_object_mut)
            && let Some(diagnostics) = workspace.remove("diagnostics")
        {
            workspace.insert("diagnostic".into(), diagnostics);
        }
        <lsp_types::InitializeParams as serde::Deserialize>::deserialize(value)
            .map(|inner| Self { inner, pull_diagnostic_data_support })
            .map_err(serde::de::Error::custom)
    }
}

impl Request for Initialize {
    type Params = InitializeParams;
    type Result = InitializeResponse;
    const METHOD: &'static str = LspInitialize::METHOD;
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InitializeResponse {
    capabilities: AdvertisedServerCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_info: Option<ServerInfo>,
}

impl InitializeResponse {
    pub(crate) fn new(capabilities: ServerCapabilities) -> Self {
        Self {
            capabilities: AdvertisedServerCapabilities {
                base: capabilities,
                type_hierarchy_provider: true,
            },
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvertisedServerCapabilities {
    #[serde(flatten)]
    base: ServerCapabilities,
    // The pinned lsp-types release omits this LSP 3.17 server capability.
    type_hierarchy_provider: bool,
}

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

/// Maps between byte offsets and LSP UTF-16 positions for one document.
pub(crate) struct LspPositionIndex<'a> {
    rope: &'a Rope,
    line_starts: Vec<usize>,
}

impl<'a> LspPositionIndex<'a> {
    pub(crate) fn new(rope: &'a Rope) -> Self {
        let mut line_starts = Vec::with_capacity(rope.line_len() + 1);
        line_starts.push(0);

        let mut bytes = rope.bytes().enumerate().peekable();
        while let Some((offset, byte)) = bytes.next() {
            let next_line = match byte {
                b'\r' => {
                    let mut next_line = offset + 1;
                    if bytes.peek().is_some_and(|(_, byte)| *byte == b'\n') {
                        bytes.next();
                        next_line += 1;
                    }
                    Some(next_line)
                }
                b'\n' => Some(offset + 1),
                _ => None,
            };
            if let Some(next_line) = next_line {
                line_starts.push(next_line);
            }
        }

        Self { rope, line_starts }
    }

    pub(crate) fn checked_text_range(
        &self,
        range: lsp_types::Range,
    ) -> Option<std::ops::Range<usize>> {
        let start = self.byte_position(range.start)?;
        let end = self.byte_position(range.end)?;
        (start <= end).then_some(start..end)
    }

    fn byte_position(&self, position: lsp_types::Position) -> Option<usize> {
        let line = usize::try_from(position.line).ok()?;
        let start = *self.line_starts.get(line)?;
        let end = self.line_end(line);
        let target = usize::try_from(position.character).ok()?;
        let mut utf16 = 0;
        let mut byte = start;
        for ch in self.rope.byte_slice(start..end).chars() {
            if utf16 == target {
                return Some(byte);
            }
            let next = utf16 + ch.len_utf16();
            if target < next {
                return None;
            }
            utf16 = next;
            byte += ch.len_utf8();
        }
        Some(end)
    }

    pub(crate) fn position_at_byte(&self, byte: usize) -> Option<lsp_types::Position> {
        if byte > self.rope.byte_len() || !self.rope.is_char_boundary(byte) {
            return None;
        }
        let line = self.line_starts.partition_point(|&start| start <= byte).checked_sub(1)?;
        let start = self.line_starts[line];
        if byte > self.line_end(line) {
            return None;
        }
        let character =
            self.rope.byte_slice(start..byte).chars().map(|ch| ch.len_utf16()).sum::<usize>();
        Some(lsp_types::Position::new(u32::try_from(line).ok()?, u32::try_from(character).ok()?))
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.rope.byte_len()
    }

    fn line_end(&self, line: usize) -> usize {
        let Some(&next_start) = self.line_starts.get(line + 1) else {
            return self.rope.byte_len();
        };
        if self.rope.byte(next_start - 1) == b'\n'
            && next_start >= 2
            && self.rope.byte(next_start - 2) == b'\r'
        {
            next_start - 2
        } else {
            next_start - 1
        }
    }
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
    Some(line_start + byte)
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
    let primary_span = diag.span.primary_span()?;
    let lsp_types::Location { uri, range } = span_to_location(source_map, primary_span)?;
    let data = diagnostic_data(source_map, &uri, primary_span, diag)?;
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
            data: Some(data),
        },
    ))
}

fn diagnostic_data(
    source_map: &SourceMap,
    uri: &lsp_types::Url,
    primary_span: Span,
    diag: &Diag,
) -> Option<serde_json::Value> {
    let (file, _) = source_map.span_to_location_info(primary_span);
    let file = file?;
    let suggestions = diag
        .suggestions
        .iter()
        .filter_map(|suggestion| {
            let alternatives = suggestion
                .substitutions
                .iter()
                .filter_map(|substitution| {
                    substitution
                        .parts
                        .iter()
                        .map(|part| {
                            let location = span_to_location(source_map, part.span)?;
                            (location.uri == *uri).then(|| {
                                lsp_types::TextEdit::new(location.range, part.snippet.to_string())
                            })
                        })
                        .collect::<Option<Vec<_>>>()
                        .filter(|edits| !edits.is_empty())
                })
                .collect::<Vec<_>>();
            (!alternatives.is_empty()).then(|| {
                DiagnosticSuggestion::new(
                    suggestion.msg.to_string(),
                    suggestion.applicability,
                    alternatives,
                )
            })
        })
        .collect();
    Some(DiagnosticData::new(uri.clone(), &file.src, suggestions).to_value())
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
    use lsp_types::{Position, Range, request::Request};
    use solar_interface::{
        BytePos, SourceMap, Span,
        diagnostics::{Applicability, Diag, DiagMsg, Level},
    };

    fn diagnostic_refresh_support(workspace: serde_json::Value) -> Option<bool> {
        let params: <super::Initialize as Request>::Params =
            serde_json::from_value(serde_json::json!({
                "capabilities": { "workspace": workspace }
            }))
            .unwrap();

        params
            .into_inner()
            .capabilities
            .workspace
            .and_then(|workspace| workspace.diagnostic)
            .and_then(|diagnostic| diagnostic.refresh_support)
    }

    #[test]
    fn initialize_params_accept_standard_diagnostics_capability() {
        assert_eq!(
            diagnostic_refresh_support(serde_json::json!({
                "diagnostics": { "refreshSupport": true }
            })),
            Some(true)
        );
    }

    #[test]
    fn initialize_params_accept_singular_diagnostic_fallback() {
        assert_eq!(
            diagnostic_refresh_support(serde_json::json!({
                "diagnostic": { "refreshSupport": true }
            })),
            Some(true)
        );
    }

    #[test]
    fn initialize_params_prefer_standard_diagnostics_capability() {
        for (diagnostic, diagnostics, expected) in
            [(false, true, Some(true)), (true, false, Some(false))]
        {
            assert_eq!(
                diagnostic_refresh_support(serde_json::json!({
                    "diagnostic": { "refreshSupport": diagnostic },
                    "diagnostics": { "refreshSupport": diagnostics }
                })),
                expected
            );
        }
    }

    #[test]
    fn initialize_params_preserve_pull_diagnostic_data_support() {
        for (data_support, expected) in [(None, false), (Some(false), false), (Some(true), true)] {
            let diagnostic =
                data_support.map(|data_support| serde_json::json!({ "dataSupport": data_support }));
            let params: <super::Initialize as Request>::Params =
                serde_json::from_value(serde_json::json!({
                    "capabilities": { "textDocument": { "diagnostic": diagnostic } }
                }))
                .unwrap();

            assert_eq!(params.pull_diagnostic_data_support(), expected);
        }
    }

    #[test]
    fn diagnostic_preserves_structured_suggestion_alternatives() {
        let source = "contract Test {\n    function f() public view {}\n}\n";
        let source_map = SourceMap::empty();
        let file = source_map
            .new_source_file(std::env::temp_dir().join("StructuredSuggestion.sol"), source)
            .unwrap();
        let named_span = |name: &str| {
            let start = source.find(name).unwrap();
            Span::new(
                file.start_pos + BytePos::from_usize(start),
                file.start_pos + BytePos::from_usize(start + name.len()),
            )
        };
        let name = named_span("f");
        let public = named_span("public");
        let view = named_span("view");
        let mut diagnostic = Diag::new(Level::Warning, "inefficient function");
        diagnostic.span(name).multipart_suggestions(
            "change visibility and mutability",
            [
                vec![(public, DiagMsg::from("external")), (view, DiagMsg::from("pure"))],
                vec![(public, DiagMsg::from("internal")), (view, DiagMsg::from("payable"))],
            ],
            Applicability::MaybeIncorrect,
        );

        let (_, diagnostic) = super::diagnostic(&source_map, &diagnostic).unwrap();
        let data = diagnostic.data.expect("structured suggestions should be preserved");

        assert_eq!(data["version"], serde_json::json!(1));
        assert_eq!(data["sourceFingerprint"], crate::code_actions::source_fingerprint(source));
        assert_eq!(
            data["suggestions"],
            serde_json::json!([{
                "title": "change visibility and mutability",
                "applicability": "MaybeIncorrect",
                "alternatives": [
                    [
                        {
                            "range": {
                                "start": { "line": 1, "character": 17 },
                                "end": { "line": 1, "character": 23 }
                            },
                            "newText": "external"
                        },
                        {
                            "range": {
                                "start": { "line": 1, "character": 24 },
                                "end": { "line": 1, "character": 28 }
                            },
                            "newText": "pure"
                        }
                    ],
                    [
                        {
                            "range": {
                                "start": { "line": 1, "character": 17 },
                                "end": { "line": 1, "character": 23 }
                            },
                            "newText": "internal"
                        },
                        {
                            "range": {
                                "start": { "line": 1, "character": 24 },
                                "end": { "line": 1, "character": 28 }
                            },
                            "newText": "payable"
                        }
                    ]
                ]
            }])
        );
    }

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
    fn checked_text_range_clamps_columns_past_crlf_line_end() {
        let rope = Rope::from("value\r\nnext");
        for character in [6, u32::MAX] {
            assert_eq!(
                checked_text_range(
                    &rope,
                    Range::new(Position::new(0, character), Position::new(0, character)),
                ),
                Some(5..5)
            );
        }
    }

    #[test]
    fn lsp_position_index_supports_standalone_carriage_returns() {
        let rope = Rope::from("a😀\rvalue");
        let index = super::LspPositionIndex::new(&rope);
        for (position, byte) in
            [(Position::new(0, 3), 5), (Position::new(1, 0), 6), (Position::new(1, 5), 11)]
        {
            let range = Range::new(position, position);
            assert_eq!(index.checked_text_range(range), Some(byte..byte));
            assert_eq!(index.position_at_byte(byte), Some(position));
        }
        assert!(index.position_at_byte(2).is_none());
    }

    #[test]
    fn lsp_position_index_accepts_trailing_carriage_return_line() {
        let rope = Rope::from("value\r");
        let index = super::LspPositionIndex::new(&rope);
        let position = Position::new(1, 0);
        assert_eq!(
            index.checked_text_range(Range::new(position, position)),
            Some(rope.byte_len()..rope.byte_len())
        );
        assert_eq!(index.position_at_byte(rope.byte_len()), Some(position));
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
