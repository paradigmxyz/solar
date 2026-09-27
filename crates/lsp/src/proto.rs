use crate::{
    code_actions::{DiagnosticData, DiagnosticSuggestion},
    vfs::{self, VfsPath},
};
use crop::{Rope, RopeSlice};
use lsp_types::{
    DiagnosticSeverity, NumberOrString, ServerCapabilities, ServerInfo,
    request::{Initialize as LspInitialize, Request},
};
use solar_config::version::SHORT_VERSION;
use solar_interface::{
    BytePos, SourceMap, Span,
    data_structures::map::FxHashMap,
    diagnostics::{Diag, Level},
    source_map::SourceFile,
};
use std::{borrow::Borrow, sync::Arc};

#[derive(Debug)]
pub(crate) enum Initialize {}

/// Reuses source fingerprints while converting compiler diagnostics from one analysis snapshot.
#[derive(Default)]
pub(crate) struct DiagnosticDataCache {
    fingerprints: FxHashMap<BytePos, String>,
}

impl DiagnosticDataCache {
    fn fingerprint(&mut self, file: &SourceFile) -> String {
        self.fingerprints
            .entry(file.start_pos)
            .or_insert_with(|| crate::code_actions::source_fingerprint(&file.src))
            .clone()
    }
}

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

pub(crate) fn byte_range_to_lsp(
    contents: &Rope,
    range: std::ops::Range<usize>,
) -> Option<lsp_types::Range> {
    Some(lsp_types::Range::new(
        position_at_byte(contents, range.start)?,
        position_at_byte(contents, range.end)?,
    ))
}

pub(crate) fn range_contains(range: lsp_types::Range, position: lsp_types::Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    position >= range.start && position < range.end
}

pub(crate) fn range_size_key(range: lsp_types::Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

pub(crate) fn range_key(range: lsp_types::Range) -> (u32, u32, u32, u32) {
    (range.start.line, range.start.character, range.end.line, range.end.character)
}

pub(crate) fn vfs_path(url: &lsp_types::Url) -> Option<vfs::VfsPath> {
    url.to_file_path().map(VfsPath::from).ok()
}

/// Returns the internal document URI using the same lexical identity as the VFS.
///
/// This does not resolve symlinks or require the file to exist. Non-file URIs are preserved;
/// callers retain their existing support checks. Common canonical URIs need no allocation.
pub(crate) fn normalize_file_uri(uri: lsp_types::Url) -> lsp_types::Url {
    if uri.scheme() != "file" || is_normalized_file_uri(&uri) {
        return uri;
    }
    vfs_path(&uri)
        .and_then(|path| lsp_types::Url::from_file_path(path.as_path()?).ok())
        .unwrap_or(uri)
}

/// Checks the allocation-free fast path for lexical file URI normalization.
#[inline(always)]
pub(crate) fn is_normalized_file_uri(uri: &lsp_types::Url) -> bool {
    let path = uri.path();
    let is_windows_drive_root = cfg!(windows)
        && path.len() == 4
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_alphabetic()
        && path.as_bytes()[2] == b':'
        && path.as_bytes()[3] == b'/';
    let has_lowercase_windows_drive = cfg!(windows)
        && path.len() >= 3
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_lowercase()
        && path.as_bytes()[2] == b':';
    uri.scheme() == "file"
        && uri.host_str().is_none()
        && uri.query().is_none()
        && uri.fragment().is_none()
        && path.starts_with('/')
        && !path.as_bytes().contains(&b'%')
        && !path.as_bytes().windows(2).any(|bytes| bytes == b"//")
        && (!path.ends_with('/') || path == "/" || is_windows_drive_root)
        && !path.split('/').any(|segment| matches!(segment, "." | ".."))
        && !has_lowercase_windows_drive
}

/// Converts an LSP UTF-16 range, clamping oversized columns to the line end and missing lines
/// to EOF. Reversed ranges and positions inside surrogate pairs are rejected.
///
/// This assumes the position encoding in LSP is UTF-16, which is mandatory to support in the LSP
/// spec.
pub(crate) fn text_range(rope: &Rope, range: lsp_types::Range) -> Option<std::ops::Range<usize>> {
    // First-line edits need no document-wide index or standalone-CR scan.
    if range.start.line == 0 && range.end.line == 0 {
        let mut end = 0;
        for chunk in rope.chunks() {
            if let Some(offset) = memchr::memchr2(b'\r', b'\n', chunk.as_bytes()) {
                end += offset;
                break;
            }
            end += chunk.len();
        }
        return byte_range_in_line(
            &rope.byte_slice(..end),
            range.start.character,
            range.end.character,
        );
    }
    LspPositionIndex::new(rope).text_range(range)
}

fn has_standalone_cr(rope: &Rope) -> bool {
    let mut chunks = rope.chunks().peekable();
    while let Some(chunk) = chunks.next() {
        for offset in memchr::memchr_iter(b'\r', chunk.as_bytes()) {
            let next = chunk
                .as_bytes()
                .get(offset + 1)
                .or_else(|| chunks.peek().and_then(|next| next.as_bytes().first()));
            if next != Some(&b'\n') {
                return true;
            }
        }
    }
    false
}

/// Maps between byte offsets and LSP UTF-16 positions for one document.
pub(crate) struct LspPositionIndex<R> {
    rope: R,
    // Short-lived conversions can use Rope's LF index. Owned request snapshots retain direct
    // line lookup, and standalone CR always requires our explicit LSP line index.
    line_starts: Option<Vec<usize>>,
}

impl<'a> LspPositionIndex<&'a Rope> {
    pub(crate) fn new(rope: &'a Rope) -> Self {
        // Below 1 KiB the small line vector is cheaper than repeated tree lookups during edits.
        let indexed = rope.byte_len() <= 1024 || has_standalone_cr(rope);
        Self { rope, line_starts: indexed.then(|| collect_line_starts(rope)) }
    }
}

impl LspPositionIndex<Rope> {
    pub(crate) fn from_rope(rope: Rope) -> Self {
        let line_starts = collect_line_starts(&rope);
        Self { rope, line_starts: Some(line_starts) }
    }
}

impl<R: Borrow<Rope>> LspPositionIndex<R> {
    pub(crate) fn rope(&self) -> &Rope {
        self.rope.borrow()
    }

    pub(crate) fn checked_text_range(
        &self,
        range: lsp_types::Range,
    ) -> Option<std::ops::Range<usize>> {
        if range.start > range.end {
            return None;
        }
        let line = usize::try_from(range.start.line).ok()?;
        let line_start = self.line_start(line)?;
        // Most source ranges stay on one line; reuse its slice for both UTF-16 endpoints.
        let contents = self.rope().byte_slice(line_start..self.line_end(line));
        if range.start.line == range.end.line {
            let bytes = byte_range_in_line(&contents, range.start.character, range.end.character)?;
            return Some(line_start + bytes.start..line_start + bytes.end);
        }
        let start = line_start + byte_column(&contents, range.start.character)?;
        let end = self.byte_position(range.end)?;
        (start <= end).then_some(start..end)
    }

    /// Like `checked_text_range`, but maps missing lines to EOF for incoming protocol positions.
    pub(crate) fn text_range(&self, range: lsp_types::Range) -> Option<std::ops::Range<usize>> {
        if range.start > range.end {
            return None;
        }
        if self.line_start(range.end.line as usize).is_some() {
            return self.checked_text_range(range);
        }
        let start = if self.line_start(range.start.line as usize).is_some() {
            self.byte_position(range.start)?
        } else {
            self.byte_len()
        };
        Some(start..self.byte_len())
    }

    fn byte_position(&self, position: lsp_types::Position) -> Option<usize> {
        let rope = self.rope();
        let line = usize::try_from(position.line).ok()?;
        let start = self.line_start(line)?;
        let end = self.line_end(line);
        let contents = rope.byte_slice(start..end);
        Some(start + byte_column(&contents, position.character)?)
    }

    pub(crate) fn position_at_byte(&self, byte: usize) -> Option<lsp_types::Position> {
        let rope = self.rope();
        let line = self.line_at_byte(byte)?;
        let start = self.line_start(line)?;
        let character = rope.byte_slice(start..byte).utf16_len();
        Some(lsp_types::Position::new(u32::try_from(line).ok()?, u32::try_from(character).ok()?))
    }

    pub(crate) fn line_at_byte(&self, byte: usize) -> Option<usize> {
        let rope = self.rope();
        if byte > rope.byte_len() || !rope.is_char_boundary(byte) {
            return None;
        }
        let line = if let Some(starts) = &self.line_starts {
            starts.partition_point(|&start| start <= byte).checked_sub(1)?
        } else {
            rope.line_of_byte(byte)
        };
        (byte <= self.line_end(line)).then_some(line)
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.rope().byte_len()
    }

    fn line_start(&self, line: usize) -> Option<usize> {
        if let Some(starts) = &self.line_starts {
            starts.get(line).copied()
        } else {
            let rope = self.rope();
            (line <= rope.line_of_byte(rope.byte_len())).then(|| rope.byte_of_line(line))
        }
    }

    fn line_end(&self, line: usize) -> usize {
        let rope = self.rope();
        let Some(next_start) = self.line_start(line + 1) else {
            return rope.byte_len();
        };
        if rope.byte(next_start - 1) == b'\n'
            && next_start >= 2
            && rope.byte(next_start - 2) == b'\r'
        {
            next_start - 2
        } else {
            next_start - 1
        }
    }
}

fn byte_range_in_line(
    contents: &RopeSlice<'_>,
    start: u32,
    end: u32,
) -> Option<std::ops::Range<usize>> {
    if start > end {
        return None;
    }
    let start_byte = byte_column(contents, start)?;
    let end_byte = if start == end { start_byte } else { byte_column(contents, end)? };
    Some(start_byte..end_byte)
}

fn byte_column(contents: &RopeSlice<'_>, character: u32) -> Option<usize> {
    let target = usize::try_from(character).ok()?;
    // ASCII lines use one UTF-16 code unit per byte.
    if contents.byte_len() == contents.utf16_len() {
        return Some(target.min(contents.byte_len()));
    }
    let mut utf16 = 0;
    let mut byte = 0;
    for ch in contents.chars() {
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
    Some(contents.byte_len())
}

fn collect_line_starts(rope: &Rope) -> Vec<usize> {
    let mut line_starts = Vec::with_capacity(rope.line_len() + 1);
    line_starts.push(0);

    let mut chunk_start = 0;
    let mut previous_cr_end = None;
    for chunk in rope.chunks() {
        for index in memchr::memchr2_iter(b'\r', b'\n', chunk.as_bytes()) {
            let offset = chunk_start + index;
            let is_cr = chunk.as_bytes()[index] == b'\r';
            // Merge CRLF even when the two bytes belong to different rope chunks.
            if !is_cr && previous_cr_end == Some(offset) {
                *line_starts.last_mut().unwrap() = offset + 1;
            } else {
                line_starts.push(offset + 1);
            }
            previous_cr_end = is_cr.then_some(offset + 1);
        }
        chunk_start += chunk.len();
    }

    line_starts
}

/// Converts an LSP UTF-16 range to a byte range, rejecting invalid positions.
pub(crate) fn checked_text_range(
    rope: &Rope,
    range: lsp_types::Range,
) -> Option<std::ops::Range<usize>> {
    LspPositionIndex::new(rope).checked_text_range(range)
}

/// Converts a byte offset into an LSP UTF-16 position.
pub(crate) fn position_at_byte(rope: &Rope, byte: usize) -> Option<lsp_types::Position> {
    LspPositionIndex::new(rope).position_at_byte(byte)
}

pub(crate) fn diagnostic_with_cache(
    source_map: &SourceMap,
    diag: &Diag,
    cache: &mut DiagnosticDataCache,
) -> Option<(lsp_types::Url, lsp_types::Diagnostic)> {
    let primary_span = diag.span.primary_span()?;
    let lsp_types::Location { uri, range } = span_to_location(source_map, primary_span)?;
    let data = diagnostic_data(source_map, &uri, primary_span, diag, cache)?;
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
    cache: &mut DiagnosticDataCache,
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
    Some(
        DiagnosticData::from_fingerprint(uri.clone(), cache.fingerprint(&file), suggestions)
            .to_value(),
    )
}

#[cfg(feature = "bench")]
pub(crate) fn benchmark_diagnostic_conversion(
    source: String,
    diagnostic_count: usize,
    cached: bool,
) -> usize {
    let source_map = SourceMap::empty();
    let file = source_map
        .new_source_file(std::env::temp_dir().join("solar-lsp-diagnostics.sol"), source)
        .expect("benchmark source should fit in a source file");
    let span = Span::new(file.start_pos, file.start_pos + BytePos::from_usize(1));
    let diagnostics = (0..diagnostic_count)
        .map(|_| {
            let mut diagnostic = Diag::new(Level::Warning, "benchmark diagnostic");
            diagnostic.span(span);
            diagnostic
        })
        .collect::<Vec<_>>();
    let mut cache = DiagnosticDataCache::default();
    let mut converted = 0;
    for diagnostic in &diagnostics {
        let result = if cached {
            diagnostic_with_cache(&source_map, diagnostic, &mut cache)
        } else {
            diagnostic_with_cache(&source_map, diagnostic, &mut DiagnosticDataCache::default())
        };
        converted += usize::from(result.is_some());
    }
    converted
}

/// Converts compiler spans to LSP locations from a snapshot of source files and their URIs.
///
/// The cache is local to one source map and must not outlive its analysis build. Construct it
/// after source loading is complete so every source file is included in the eager snapshot.
pub(crate) struct LocationConverter {
    files: Vec<Arc<SourceFile>>,
    uris: FxHashMap<BytePos, lsp_types::Url>,
}

impl LocationConverter {
    pub(crate) fn new(source_map: Arc<SourceMap>) -> Self {
        let files = source_map.files().to_vec();
        let mut uris = FxHashMap::with_capacity_and_hasher(files.len(), Default::default());
        for file in files.iter() {
            if let Some(path) = file.name.as_real()
                && let Ok(uri) = lsp_types::Url::from_file_path(path)
            {
                uris.insert(file.start_pos, uri);
            }
        }
        Self { files, uris }
    }

    pub(crate) fn file_uri(&self, file: &SourceFile) -> Option<&lsp_types::Url> {
        self.uris.get(&file.start_pos)
    }

    pub(crate) fn location(&self, span: Span) -> Option<lsp_types::Location> {
        if span.is_dummy() {
            return None;
        }
        let next = self.files.partition_point(|file| file.start_pos <= span.lo());
        let file = self.files.get(next.checked_sub(1)?)?;
        // Source files are ordered by start position. The next file alone determines whether
        // the span crosses files, so neither endpoint needs a locked source-map lookup.
        if self.files.get(next).is_some_and(|next| span.hi() >= next.start_pos) {
            return None;
        }
        Some(lsp_types::Location {
            uri: self.file_uri(file)?.clone(),
            range: lsp_types::Range {
                start: lsp_position(file, span.lo())?,
                end: lsp_position(file, span.hi())?,
            },
        })
    }
}

pub(crate) fn span_to_location(source_map: &SourceMap, span: Span) -> Option<lsp_types::Location> {
    span_to_location_with(source_map, span, |file| {
        lsp_types::Url::from_file_path(file.name.as_real().unwrap()).ok()
    })
}

fn span_to_location_with(
    source_map: &SourceMap,
    span: Span,
    uri: impl FnOnce(&SourceFile) -> Option<lsp_types::Url>,
) -> Option<lsp_types::Location> {
    if source_map.is_empty() || span.is_dummy() {
        return None;
    }

    let file = source_map.lookup_source_file(span.lo());
    let hi_file = source_map.lookup_source_file(span.hi());
    if file.start_pos != hi_file.start_pos {
        return None;
    }
    Some(lsp_types::Location {
        uri: uri(&file)?,
        range: lsp_types::Range {
            start: lsp_position(&file, span.lo())?,
            end: lsp_position(&file, span.hi())?,
        },
    })
}

fn lsp_position(file: &SourceFile, pos: BytePos) -> Option<lsp_types::Position> {
    let offset = file.relative_position(pos);
    let line_index = file.lookup_line(offset)?;
    let start = file.lines()[line_index].to_usize();
    let column = offset.to_usize().checked_sub(start)?;
    let character = if file.multibyte_chars.is_empty() {
        // Keep the old `get_line` behavior without scanning the line: a line's `lines` entry
        // includes the following `\n`, while LSP ranges stop before that terminator. A CRLF line
        // deliberately retains its `\r`, matching `get_line` and the previous conversion.
        let mut end = file
            .lines()
            .get(line_index + 1)
            .map_or_else(|| file.source_len.to_usize(), |pos| pos.to_usize());
        if end > start && file.src.as_bytes().get(end - 1) == Some(&b'\n') {
            end -= 1;
        }
        u32::try_from(column.min(end.saturating_sub(start))).ok()?
    } else {
        // Only scan this line's prefix, not all multibyte characters preceding the line.
        let line = file.get_line(line_index)?;
        let prefix = line.get(..column.min(line.len()))?;
        u32::try_from(prefix.encode_utf16().count()).ok()?
    };
    Some(lsp_types::Position::new(u32::try_from(line_index).ok()?, character))
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
    use super::{
        checked_text_range, collect_line_starts, normalize_file_uri, position_at_byte, text_range,
    };
    use crate::utils::apply_document_changes;
    use crop::Rope;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url, request::Request};
    use solar_interface::{
        BytePos, SourceMap, Span,
        diagnostics::{Applicability, Diag, DiagMsg, Level},
        source_map::FileName,
    };
    use std::sync::Arc;

    #[test]
    fn equivalent_file_uris_share_the_vfs_document_key() {
        let canonical = Url::from_file_path(std::env::temp_dir().join("Token.sol")).unwrap();
        for spelling in ["%54oken.sol", "/Token.sol", "nested%2F..%2FToken.sol"] {
            let alias = Url::parse(&canonical.as_str().replacen("Token.sol", spelling, 1)).unwrap();
            assert_ne!(alias, canonical);
            assert_eq!(super::vfs_path(&alias), super::vfs_path(&canonical));
            assert_eq!(normalize_file_uri(alias), canonical);
        }
        assert_eq!(normalize_file_uri(canonical.clone()), canonical);
    }

    fn vfs_file_uri(uri: Url) -> Url {
        super::vfs_path(&uri)
            .and_then(|path| Url::from_file_path(path.as_path()?).ok())
            .unwrap_or(uri)
    }

    #[test]
    fn file_uri_fast_path_matches_vfs_identity() {
        let mut uris = vec![
            Url::from_file_path(std::env::temp_dir().join("Canonical.sol")).unwrap(),
            Url::from_file_path(std::env::temp_dir().join("nested/../Token.sol")).unwrap(),
            Url::from_file_path(std::env::temp_dir().join("nested/./Token.sol")).unwrap(),
            Url::parse("file:///").unwrap(),
            Url::parse("file:///tmp/Encoded%20Name.sol").unwrap(),
            Url::parse("file:///tmp//Repeated.sol").unwrap(),
            Url::parse("file:///tmp/directory/").unwrap(),
            Url::parse("file://localhost/tmp/Hosted.sol").unwrap(),
            Url::parse("file:///tmp/Query.sol?version=1").unwrap(),
            Url::parse("file:///tmp/Fragment.sol#source").unwrap(),
        ];
        if cfg!(windows) {
            uris.extend([
                Url::parse("file:///C:/tmp/Canonical.sol").unwrap(),
                Url::parse("file:///C:/").unwrap(),
                Url::parse("file:///tmp/NoDrive.sol").unwrap(),
                Url::parse("file:///C%3A/tmp/EncodedDrive.sol").unwrap(),
                Url::parse("file://server/share/Hosted.sol").unwrap(),
            ]);
        }

        for uri in uris {
            assert_eq!(normalize_file_uri(uri.clone()), vfs_file_uri(uri.clone()), "{uri}");
        }
    }

    #[test]
    fn normalize_file_uri_preserves_non_file_uris() {
        let uri = Url::parse("untitled:/tmp/Virtual.sol").unwrap();

        assert_eq!(normalize_file_uri(uri.clone()), uri);
    }

    #[cfg(windows)]
    #[test]
    fn normalize_file_uri_canonicalizes_lowercase_windows_drive() {
        let lowercase = Url::parse("file:///c:/tmp/Contract.sol").unwrap();
        let uppercase = Url::parse("file:///C:/tmp/Contract.sol").unwrap();

        assert_eq!(normalize_file_uri(lowercase), uppercase);
    }

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
    fn line_starts_match_mixed_newlines_across_rope_chunks() {
        for padding in 0..2048 {
            let source =
                format!("{}\r\n{}", "a".repeat(padding), "😀\r\nA\rB\n\r\n\n\r".repeat(200));
            let rope = Rope::from(source.as_str());
            assert!(rope.chunks().count() > 1);
            let expected = std::iter::once(0)
                .chain(source.bytes().enumerate().filter_map(|(offset, byte)| {
                    (byte == b'\n'
                        || (byte == b'\r' && source.as_bytes().get(offset + 1) != Some(&b'\n')))
                    .then_some(offset + 1)
                }))
                .collect::<Vec<_>>();
            assert_eq!(collect_line_starts(&rope), expected, "padding {padding}");
        }
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

        let mut cache = super::DiagnosticDataCache::default();
        let (_, diagnostic) =
            super::diagnostic_with_cache(&source_map, &diagnostic, &mut cache).unwrap();
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
    fn span_to_location_uses_utf16_columns() {
        let source = "a😀中value\n";
        let source_map = SourceMap::empty();
        let file = source_map
            .new_source_file(std::env::temp_dir().join("Utf16Location.sol"), source)
            .unwrap();
        let start = source.find("value").unwrap();
        let span = Span::new(
            file.start_pos + BytePos::from_usize(start),
            file.start_pos + BytePos::from_usize(start + "value".len()),
        );

        let location = super::span_to_location(&source_map, span).unwrap();

        assert_eq!(location.range, Range::new(Position::new(0, 4), Position::new(0, 9)));
    }

    #[test]
    fn span_to_location_matches_character_columns_on_later_lines() {
        let source = "// 😀中é\r\n// ─────────\ncontract C { string s = unicode\"😀é\"; }\n";
        let source_map = SourceMap::empty();
        let file = source_map
            .new_source_file(std::env::temp_dir().join("MultilineLocation.sol"), source)
            .unwrap();
        for offset in source.char_indices().map(|(offset, _)| offset).chain([source.len()]) {
            let pos = file.start_pos + BytePos::from_usize(offset);
            let span = Span::new(pos, pos);
            if span.is_dummy() {
                continue;
            }
            let (line, column) = file.lookup_file_pos(file.relative_position(pos));
            let expected_column = file
                .get_line(line - 1)
                .unwrap()
                .chars()
                .take(column.to_usize())
                .map(char::len_utf16)
                .sum::<usize>();
            let location = super::span_to_location(&source_map, span).unwrap();
            let expected = Position::new((line - 1) as u32, expected_column as u32);
            assert_eq!(location.range, Range::new(expected, expected), "byte {offset}");
        }
    }

    #[test]
    fn span_to_location_clamps_ascii_line_terminators() {
        for (source, end_character) in [("value\n", 5), ("value\r\n", 6), ("value", 5)] {
            let source_map = SourceMap::empty();
            let file = source_map
                .new_source_file(std::env::temp_dir().join("AsciiLocation.sol"), source)
                .unwrap();
            let location = super::span_to_location(
                &source_map,
                Span::new(file.start_pos, file.end_position()),
            )
            .unwrap();

            assert_eq!(
                location.range,
                Range::new(Position::new(0, 0), Position::new(0, end_character))
            );
        }
    }

    #[test]
    fn span_to_location_rejects_empty_dummy_and_cross_file_spans() {
        let empty = SourceMap::empty();
        assert!(super::span_to_location(&empty, Span::DUMMY).is_none());

        let source_map = SourceMap::empty();
        let first = source_map
            .new_source_file(std::env::temp_dir().join("FirstLocation.sol"), "first")
            .unwrap();
        let second = source_map
            .new_source_file(std::env::temp_dir().join("SecondLocation.sol"), "second")
            .unwrap();
        let cross_file = Span::new(first.start_pos, second.start_pos);

        assert!(super::span_to_location(&source_map, cross_file).is_none());
    }

    #[test]
    fn location_converter_matches_source_map_positions() {
        let source_map = Arc::new(SourceMap::empty());
        let empty = super::LocationConverter::new(source_map.clone());
        for span in [Span::DUMMY, Span::new(BytePos(1), BytePos(2))] {
            assert_eq!(empty.location(span), super::span_to_location(&source_map, span));
        }

        let files = ["value\n", "", "a😀中value\r\nsecond line\n", "value\r\n", "last"]
            .iter()
            .enumerate()
            .map(|(index, &source)| {
                source_map
                    .new_source_file(
                        std::env::temp_dir().join(format!("Location {index}.sol")),
                        source,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let locations = super::LocationConverter::new(source_map.clone());
        for file in &files {
            assert_eq!(
                locations.file_uri(file),
                lsp_types::Url::from_file_path(file.name.as_real().unwrap()).ok().as_ref()
            );
            let positions = file
                .src
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([file.src.len()])
                .map(|offset| file.start_pos + BytePos::from_usize(offset))
                .collect::<Vec<_>>();
            for (index, &lo) in positions.iter().enumerate() {
                for &hi in &positions[index..] {
                    let span = Span::new(lo, hi);
                    assert_eq!(
                        locations.location(span),
                        super::span_to_location(&source_map, span),
                        "span {lo:?}..{hi:?}"
                    );
                }
            }
        }
        for pair in files.windows(2) {
            let span = Span::new(pair[0].end_position(), pair[1].start_pos);
            assert_eq!(locations.location(span), super::span_to_location(&source_map, span));
            assert!(locations.location(span).is_none());
        }

        // The original conversion clamps positions beyond the last file's end.
        let last = files.last().unwrap();
        let span = Span::new(last.start_pos, last.end_position() + BytePos(5));
        assert_eq!(locations.location(span), super::span_to_location(&source_map, span));
    }

    #[test]
    fn location_converter_preserves_files_without_uris() {
        let source_map = Arc::new(SourceMap::empty());
        let first =
            source_map.new_source_file(std::env::temp_dir().join("FirstUri.sol"), "first").unwrap();
        let custom = source_map.new_source_file(FileName::custom("virtual.sol"), "custom").unwrap();
        let relative =
            source_map.new_source_file(FileName::real("relative.sol"), "relative").unwrap();
        let last =
            source_map.new_source_file(std::env::temp_dir().join("LastUri.sol"), "last").unwrap();
        let locations = super::LocationConverter::new(source_map.clone());

        for file in [&custom, &relative] {
            assert!(locations.file_uri(file).is_none());
            assert!(locations.location(Span::new(file.start_pos, file.end_position())).is_none());
            assert!(locations.location(Span::new(first.start_pos, file.start_pos)).is_none());
        }
        let relative_span = Span::new(relative.start_pos, relative.end_position());
        assert_eq!(
            locations.location(relative_span),
            super::span_to_location(&source_map, relative_span)
        );
        let last_span = Span::new(last.start_pos, last.end_position());
        assert_eq!(locations.location(last_span), super::span_to_location(&source_map, last_span));
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
        assert!(
            checked_text_range(&rope, Range::new(Position::new(0, 1), Position::new(0, 1)),)
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
    fn checked_text_range_validates_same_line_endpoint_pairs() {
        let columns = [0, 1, 2, 3, 4, 5, 6, u32::MAX];
        for (line, offsets) in [
            ("value", [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), Some(5), Some(5)]),
            ("a😀中z", [Some(0), Some(1), None, Some(5), Some(8), Some(9), Some(9), Some(9)]),
        ] {
            for ending in ["\n", "\r\n", "\r"] {
                let prefix = format!("😀{ending}");
                let source = format!("{prefix}{line}{ending}tail");
                let rope = Rope::from(source.as_str());
                let index = super::LspPositionIndex::new(&rope);
                for (&start, &start_offset) in columns.iter().zip(&offsets) {
                    for (&end, &end_offset) in columns.iter().zip(&offsets) {
                        let expected = start_offset
                            .zip(end_offset)
                            .filter(|_| start <= end)
                            .map(|(start, end)| prefix.len() + start..prefix.len() + end);
                        assert_eq!(
                            index.checked_text_range(Range::new(
                                Position::new(1, start),
                                Position::new(1, end),
                            )),
                            expected,
                            "source: {source:?}, columns: {start}..{end}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn checked_text_range_handles_ascii_lines_after_unicode() {
        let ascii = "value ".repeat(1024);
        for ending in ["\n", "\r\n", "\r"] {
            let prefix = format!("😀{ending}");
            let source = format!("{prefix}{ascii}{ending}");
            let rope = Rope::from(source.as_str());
            let index = super::LspPositionIndex::new(&rope);
            for character in [0, 1, 1023, ascii.len() as u32, u32::MAX] {
                let position = Position::new(1, character);
                let byte = prefix.len() + (character as usize).min(ascii.len());
                assert_eq!(
                    index.checked_text_range(Range::new(position, position)),
                    Some(byte..byte)
                );
            }
            assert_eq!(
                index.checked_text_range(Range::new(Position::new(1, 1), Position::new(1, 5))),
                Some(prefix.len() + 1..prefix.len() + 5)
            );
            assert!(
                index
                    .checked_text_range(Range::new(Position::new(1, 5), Position::new(1, 1)))
                    .is_none()
            );
            assert!(
                index
                    .checked_text_range(Range::new(Position::new(1, 0), Position::new(3, 0)))
                    .is_none()
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
    fn position_conversions_support_standalone_carriage_returns() {
        let rope = Rope::from("a😀\rvalue");
        for (position, byte) in
            [(Position::new(0, 3), 5), (Position::new(1, 0), 6), (Position::new(1, 5), 11)]
        {
            let range = Range::new(position, position);
            assert_eq!(checked_text_range(&rope, range), Some(byte..byte));
            assert_eq!(position_at_byte(&rope, byte), Some(position));
        }
        assert!(position_at_byte(&rope, 2).is_none());
    }

    #[test]
    fn position_conversions_accept_trailing_carriage_return_line() {
        let rope = Rope::from("value\r");
        let position = Position::new(1, 0);
        assert_eq!(
            checked_text_range(&rope, Range::new(position, position)),
            Some(rope.byte_len()..rope.byte_len())
        );
        assert_eq!(position_at_byte(&rope, rope.byte_len()), Some(position));
    }

    #[test]
    fn text_range_uses_standalone_carriage_return_lines() {
        let rope = Rope::from("first\rsecond");
        let range =
            text_range(&rope, Range::new(Position::new(1, 0), Position::new(1, 6))).unwrap();
        assert_eq!(rope.byte_slice(range).to_string(), "second");
    }

    #[test]
    fn text_range_clamps_protocol_positions() {
        for ending in ["\n", "\r\n", "\r"] {
            for padding in [String::new(), "x".repeat(2048)] {
                let first = format!("{padding}a😀b");
                let source = format!("{first}{ending}céc{ending}");
                let rope = Rope::from(source.as_str());
                let index = super::LspPositionIndex::from_rope(rope.clone());
                let columns = padding.len() as u32;
                for (position, byte) in [
                    (Position::new(0, columns + 1), Some(padding.len() + 1)),
                    (Position::new(0, columns + 2), None),
                    (Position::new(0, columns + 3), Some(padding.len() + 5)),
                    (Position::new(0, columns + 5), Some(first.len())),
                    (Position::new(0, u32::MAX), Some(first.len())),
                    (Position::new(1, 2), Some(first.len() + ending.len() + 3)),
                    (Position::new(1, u32::MAX), Some(source.len() - ending.len())),
                    (Position::new(2, u32::MAX), Some(source.len())),
                    (Position::new(99, 1), Some(source.len())),
                    (Position::new(u32::MAX, u32::MAX), Some(source.len())),
                ] {
                    let range = Range::new(position, position);
                    let expected = byte.map(|byte| byte..byte);
                    assert_eq!(text_range(&rope, range), expected, "{ending:?}: {position:?}");
                    assert_eq!(index.text_range(range), expected, "{ending:?}: {position:?}");
                }
                let range = Range::new(Position::new(0, u32::MAX), Position::new(99, 1));
                let expected = Some(first.len()..source.len());
                assert_eq!(text_range(&rope, range), expected);
                assert_eq!(index.text_range(range), expected);
            }
        }
        let position = Position::new(u32::MAX, u32::MAX);
        assert_eq!(text_range(&Rope::new(), Range::new(position, position)), Some(0..0));
    }

    #[test]
    fn text_range_rejects_reversed_ranges_even_after_clamping() {
        let rope = Rope::from("abc\n😀");
        for range in [
            Range::new(Position::new(0, u32::MAX), Position::new(0, 4)),
            Range::new(Position::new(99, 0), Position::new(2, 0)),
            Range::new(Position::new(1, 1), Position::new(1, 2)),
        ] {
            assert_eq!(text_range(&rope, range), None);
            assert_eq!(super::LspPositionIndex::from_rope(rope.clone()).text_range(range), None);
        }
    }

    #[test]
    fn text_range_matches_index_for_invalid_positions() {
        let positions = [
            Position::new(0, 0),
            Position::new(0, 2),
            Position::new(0, 5),
            Position::new(1, 0),
            Position::new(1, 3),
            Position::new(2, 0),
            Position::new(3, 1),
            Position::new(u32::MAX, 0),
            Position::new(0, u32::MAX),
        ];
        for source in ["", "abc", "abc\n", "abc\r\n", "a😀b\ncdef\n", "a😀b\rcdef\r"] {
            let rope = Rope::from(source);
            let index = super::LspPositionIndex::from_rope(rope.clone());
            for &start in &positions {
                for &end in &positions {
                    let range = Range::new(start, end);
                    let expected = index.text_range(range);
                    let actual = text_range(&rope, range);
                    assert_eq!(actual, expected, "source: {source:?}, range: {range:?}");
                }
            }
        }
    }

    #[test]
    fn text_range_detects_standalone_cr_across_rope_chunks() {
        let mut split_crlf = false;
        for padding in 0..2048 {
            let source = format!("{}{}", "a".repeat(padding), "😀\r\nA\n\r\n".repeat(200));
            let mut rope = Rope::from(source.as_str());
            let chunks = rope.chunks().collect::<Vec<_>>();
            assert!(chunks.len() > 1);
            split_crlf |=
                chunks.windows(2).any(|pair| pair[0].ends_with('\r') && pair[1].starts_with('\n'));
            assert!(!super::has_standalone_cr(&rope));

            // Removing LF changes a CRLF into a standalone CR, including chunk boundaries.
            let newline = source.find('\n').unwrap();
            rope.replace(newline..newline + 1, "");
            assert!(super::has_standalone_cr(&rope));
        }
        assert!(split_crlf, "fixture must include a CRLF split across rope chunks");
        assert!(super::has_standalone_cr(&Rope::from("tail\r")));
        assert!(!super::has_standalone_cr(&Rope::new()));
    }

    #[test]
    fn incoming_edits_match_index_when_line_endings_change() {
        for ending in ["\n", "\r\n", "\r"] {
            let original = Rope::from(["a😀b", "céc", "tail", ""].join(ending));
            let changes = [
                (Range::new(Position::new(2, 4), Position::new(2, 4)), "\r"),
                (Range::new(Position::new(2, 0), Position::new(2, 1)), "\n"),
                (Range::new(Position::new(1, 1), Position::new(1, 2)), "中😀"),
                (Range::new(Position::new(0, 1), Position::new(0, 3)), "🙂"),
                (Range::new(Position::new(0, 4), Position::new(1, 0)), "\r"),
                (Range::new(Position::new(1, 0), Position::new(1, 0)), "\n"),
            ]
            .map(|(range, text)| TextDocumentContentChangeEvent {
                range: Some(range),
                range_length: None,
                text: text.into(),
            });
            let mut expected = original.clone();
            for change in &changes {
                let range = super::LspPositionIndex::from_rope(expected.clone())
                    .text_range(change.range.unwrap())
                    .unwrap();
                expected.replace(range, &change.text);
            }
            assert_eq!(apply_document_changes(&original, changes.into()).unwrap(), expected);
        }
    }

    #[test]
    fn transient_position_index_matches_snapshot() {
        for source in [
            String::new(),
            "a😀b".into(),
            "a😀b\n\ncéc\ntail\n".into(),
            "a😀b\r\n\r\ncéc\r\ntail\r\n".into(),
            "a😀b\rcéc\r\ntail\n".into(),
            "a😀b\r\n".repeat(1024),
        ] {
            let rope = Rope::from(source.as_str());
            let transient = super::LspPositionIndex::new(&rope);
            let snapshot = super::LspPositionIndex::from_rope(rope.clone());
            for byte in 0..=source.len() + 1 {
                assert_eq!(transient.position_at_byte(byte), snapshot.position_at_byte(byte));
            }
            for line in 0..=rope.line_len() as u32 + 2 {
                for column in (0..=8).chain([u32::MAX]) {
                    let start = Position::new(line, column);
                    for end in [start, Position::new(line, 8), Position::new(line + 1, 2)] {
                        let range = Range::new(start, end);
                        assert_eq!(
                            transient.checked_text_range(range),
                            snapshot.checked_text_range(range),
                            "range: {range:?}"
                        );
                    }
                }
            }
        }
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
    fn position_at_byte_matches_utf16_columns_across_rope_chunks() {
        let long_line = "xé中😀".repeat(512);
        for ending in ["\n", "\r\n", "\r"] {
            let lines = ["preceding😀", long_line.as_str(), ""];
            let source = lines.join(ending);
            let rope = Rope::from(source.as_str());
            let index = super::LspPositionIndex::new(&rope);
            let mut start = 0;
            for (line, text) in lines.into_iter().enumerate() {
                for byte in 0..=text.len() {
                    let expected = text.get(..byte).map(|prefix| {
                        Position::new(line as u32, prefix.encode_utf16().count() as u32)
                    });
                    assert_eq!(index.position_at_byte(start + byte), expected);
                }
                if ending == "\r\n" && line + 1 < lines.len() {
                    assert!(index.position_at_byte(start + text.len() + 1).is_none());
                }
                start += text.len() + ending.len();
            }
            assert!(index.position_at_byte(source.len() + 1).is_none());
        }
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
