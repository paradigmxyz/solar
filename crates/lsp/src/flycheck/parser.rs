use crate::{
    code_actions::{DiagnosticData, DiagnosticSuggestion, ranges_overlap},
    diagnostics::{
        DiagnosticMap,
        presentation::{DiagnosticMessage, forge_diagnostic_tags, solidity_diagnostic_tags},
    },
    flycheck::config::FlycheckOutput,
};
use crop::Rope;
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Location, NumberOrString, Position, Range, Url,
};
use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_interface::{
    data_structures::map::FxHashMap,
    diagnostics::{
        Applicability, JsonDiagnostic, JsonDiagnosticMessage, JsonDiagnosticSpan, Severity,
    },
    source_map::{FileLoader, SourceMap},
};
use std::{
    borrow::Cow,
    path::{Component, Path, PathBuf},
};

pub(crate) type SourceSnapshot = FxHashMap<PathBuf, Rope>;

pub(super) fn parse(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
    source_snapshot: Option<&SourceSnapshot>,
) -> Result<DiagnosticMap, ParseError> {
    let mut diagnostics = DiagnosticMap::default();
    let mut push = |diagnostic: Option<(Url, LspDiagnostic)>| {
        if let Some((uri, diagnostic)) = diagnostic {
            diagnostics.entry(uri).or_default().push(diagnostic);
        }
    };
    let mut range_cache = ByteRangeCache::new(source_snapshot);
    let source = source(format);

    match format {
        FlycheckOutput::SolcJson => {
            let stream =
                serde_json::Deserializer::from_slice(output).into_iter::<SolcJsonRecord<'_>>();
            for record in stream {
                match record? {
                    SolcJsonRecord::Diagnostic(diagnostic) => {
                        push(solc_diagnostic(diagnostic, cwd, source, &mut range_cache));
                    }
                    SolcJsonRecord::Diagnostics(records)
                    | SolcJsonRecord::Errors(SolcJsonErrors { errors: records }) => {
                        for diagnostic in records {
                            push(solc_diagnostic(diagnostic, cwd, source, &mut range_cache));
                        }
                    }
                }
            }
        }
        FlycheckOutput::ForgeLintJson => {
            let stream = serde_json::Deserializer::from_slice(output)
                .into_iter::<&serde_json::value::RawValue>();
            for raw in stream {
                let raw = raw?;
                match serde_json::from_str(raw.get()) {
                    Ok(JsonEmitterRecord::Rustc(JsonDiagnosticMessage::Diagnostic(diagnostic))) => {
                        push(json_diagnostic(diagnostic, cwd, source, &mut range_cache));
                    }
                    Ok(JsonEmitterRecord::Solc(diagnostic)) => {
                        push(solc_diagnostic(diagnostic, cwd, source, &mut range_cache));
                    }
                    Err(error) => {
                        let value = serde_json::from_str(raw.get())?;
                        if is_json_emitter_diagnostic(&value) {
                            return Err(error.into());
                        }
                    }
                }
            }
        }
    }

    Ok(diagnostics)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("failed to parse flycheck JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SolcJsonRecord<'a> {
    Diagnostic(#[serde(borrow)] SolcInputDiagnostic<'a>),
    Diagnostics(#[serde(borrow)] Vec<SolcInputDiagnostic<'a>>),
    Errors(#[serde(borrow)] SolcJsonErrors<'a>),
}

#[derive(Debug, Deserialize)]
struct SolcJsonErrors<'a> {
    #[serde(borrow)]
    errors: Vec<SolcInputDiagnostic<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolcInputDiagnostic<'a> {
    #[serde(borrow)]
    source_location: Option<SolcInputSourceLocation<'a>>,
    #[serde(default, borrow)]
    secondary_source_locations: Vec<SolcInputSourceLocation<'a>>,
    #[serde(rename = "type", borrow)]
    _type: Cow<'a, str>,
    #[serde(rename = "component", borrow)]
    _component: Cow<'a, str>,
    severity: Severity,
    #[serde(borrow)]
    error_code: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Cow<'a, str>,
}

#[derive(Debug, Deserialize)]
struct SolcInputSourceLocation<'a> {
    #[serde(borrow)]
    file: Cow<'a, str>,
    start: i64,
    end: i64,
    #[serde(borrow)]
    message: Option<Cow<'a, str>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonEmitterRecord<'a> {
    Rustc(#[serde(borrow)] JsonDiagnosticMessage<'a>),
    Solc(#[serde(borrow)] SolcInputDiagnostic<'a>),
}

fn is_json_emitter_diagnostic(value: &serde_json::Value) -> bool {
    value.get("$message_type").and_then(serde_json::Value::as_str) == Some("diagnostic")
        || value.get("severity").is_some() && value.get("message").is_some()
}

fn solc_diagnostic(
    diagnostic: SolcInputDiagnostic<'_>,
    cwd: &Path,
    source: &'static str,
    range_cache: &mut ByteRangeCache<'_>,
) -> Option<(Url, LspDiagnostic)> {
    let location = diagnostic.source_location?;
    let path = resolve_path(range_cache.source_map.file_loader(), cwd, location.file.as_ref());
    let uri = Url::from_file_path(&path).ok()?;
    let (start, end) = if location.start == -1 && location.end == -1 {
        (0, 0)
    } else {
        (usize::try_from(location.start).ok()?, usize::try_from(location.end).ok()?)
    };
    let range = range_cache.checked_range(&path, start, end)?;
    let data = diagnostic_data(range_cache, &path, uri.clone(), Vec::new());
    let tags = if source == "forge-lint" {
        forge_diagnostic_tags(diagnostic.error_code.as_deref())
    } else {
        solidity_diagnostic_tags(diagnostic.error_code.as_deref())
    };
    let mut message =
        DiagnosticMessage::new(Location::new(uri.clone(), range), diagnostic.message.into_owned());
    if let Some(label) = &location.message {
        message.push(None, label);
    }
    for location in diagnostic.secondary_source_locations {
        if let Some(label) = &location.message {
            let path = resolve_path(range_cache.source_map.file_loader(), cwd, &location.file);
            let location = usize::try_from(location.start)
                .ok()
                .zip(usize::try_from(location.end).ok())
                .and_then(|(start, end)| range_cache.location(&path, start, end));
            message.push(location, label);
        }
    }

    Some((
        uri,
        LspDiagnostic {
            range,
            severity: Some(solc_severity(diagnostic.severity)),
            code: diagnostic.error_code.map(|code| NumberOrString::String(code.into_owned())),
            code_description: None,
            source: Some(source.into()),
            message: message.message,
            related_information: Some(message.related_information),
            tags,
            data,
        },
    ))
}

fn json_diagnostic(
    diagnostic: JsonDiagnostic<'_>,
    cwd: &Path,
    source: &'static str,
    range_cache: &mut ByteRangeCache<'_>,
) -> Option<(Url, LspDiagnostic)> {
    let span = primary_span(&diagnostic)?;
    let path = resolve_path(range_cache.source_map.file_loader(), cwd, span.file_name.as_ref());
    let uri = Url::from_file_path(&path).ok()?;
    let range =
        range_cache.checked_range(&path, span.byte_start as usize, span.byte_end as usize)?;
    let suggestions = json_suggestions(&diagnostic, cwd, &uri, range_cache);
    let data = diagnostic_data(range_cache, &path, uri.clone(), suggestions);
    let primary = Location::new(uri.clone(), range);
    let mut message = DiagnosticMessage::new(primary.clone(), diagnostic.message.to_string());
    json_diagnostic_details(&diagnostic, &primary, &mut message, cwd, range_cache);
    let tags = forge_diagnostic_tags(diagnostic.code.as_ref().map(|code| code.code.as_ref()));

    Some((
        uri,
        LspDiagnostic {
            range,
            severity: Some(json_level_severity(diagnostic.level.as_ref())),
            code: diagnostic.code.map(|code| NumberOrString::String(code.code.into_owned())),
            code_description: None,
            source: Some(source.into()),
            message: message.message,
            related_information: Some(message.related_information),
            tags,
            data,
        },
    ))
}

fn json_suggestions(
    diagnostic: &JsonDiagnostic<'_>,
    cwd: &Path,
    uri: &Url,
    range_cache: &mut ByteRangeCache<'_>,
) -> Vec<DiagnosticSuggestion> {
    let mut suggestions = Vec::<DiagnosticSuggestion>::new();
    for child in &diagnostic.children {
        let Some(mut suggestion) = json_suggestion(child, cwd, uri, range_cache) else {
            continue;
        };
        if suggestions.iter_mut().any(|existing| existing.merge_alternatives(&mut suggestion)) {
            continue;
        }
        suggestions.push(suggestion);
    }
    suggestions
}

fn json_suggestion(
    diagnostic: &JsonDiagnostic<'_>,
    cwd: &Path,
    uri: &Url,
    range_cache: &mut ByteRangeCache<'_>,
) -> Option<DiagnosticSuggestion> {
    let mut applicability = None;
    let mut edits = Vec::with_capacity(diagnostic.spans.len());
    let mut byte_ranges = Vec::with_capacity(diagnostic.spans.len());
    for span in &diagnostic.spans {
        let replacement = span.suggested_replacement.as_ref()?;
        let span_applicability = span.suggestion_applicability.unwrap_or_default();
        match applicability {
            Some(applicability) if applicability != span_applicability => return None,
            None => applicability = Some(span_applicability),
            Some(_) => {}
        }

        let path = resolve_path(range_cache.source_map.file_loader(), cwd, span.file_name.as_ref());
        if Url::from_file_path(&path).ok().as_ref() != Some(uri) {
            return None;
        }
        let start = span.byte_start as usize;
        let end = span.byte_end as usize;
        let range = range_cache.checked_range(&path, start, end)?;
        byte_ranges.push(start..end);
        edits.push(lsp_types::TextEdit::new(range, replacement.clone().into_owned()));
    }
    if edits.is_empty() || ranges_overlap(&mut byte_ranges) {
        return None;
    }

    Some(DiagnosticSuggestion::new(
        diagnostic.message.to_string(),
        applicability.unwrap_or(Applicability::Unspecified),
        vec![edits],
    ))
}

fn diagnostic_data(
    range_cache: &mut ByteRangeCache<'_>,
    path: &Path,
    uri: Url,
    suggestions: Vec<DiagnosticSuggestion>,
) -> Option<serde_json::Value> {
    if !range_cache.is_trusted(path) {
        return None;
    }
    let file = range_cache.file(path)?;
    Some(DiagnosticData::from_rope(uri, file, suggestions).to_value())
}

fn primary_span<'a, 'b>(diagnostic: &'a JsonDiagnostic<'b>) -> Option<&'a JsonDiagnosticSpan<'b>> {
    diagnostic.spans.iter().find(|span| span.is_primary).or_else(|| diagnostic.spans.first())
}

fn json_diagnostic_details(
    diagnostic: &JsonDiagnostic<'_>,
    primary: &Location,
    message: &mut DiagnosticMessage,
    cwd: &Path,
    range_cache: &mut ByteRangeCache<'_>,
) {
    for span in &diagnostic.spans {
        let location = json_span_location(span, cwd, range_cache);
        if let Some(label) = &span.label {
            message.push(location, label);
        } else if location.as_ref().is_some_and(|location| location != primary) {
            message.push(location, &diagnostic.message);
        }
    }
    for child in &diagnostic.children {
        let location = child
            .spans
            .iter()
            .find(|span| span.is_primary)
            .and_then(|span| json_span_location(span, cwd, range_cache));
        if location.as_ref().is_none_or(|location| location == primary) {
            message.push(None, &format!("{}: {}", child.level, child.message));
        } else {
            message.push(location, &child.message);
        }
        json_diagnostic_details(child, primary, message, cwd, range_cache);
    }
}

fn json_span_location(
    span: &JsonDiagnosticSpan<'_>,
    cwd: &Path,
    range_cache: &mut ByteRangeCache<'_>,
) -> Option<Location> {
    let path = resolve_path(range_cache.source_map.file_loader(), cwd, &span.file_name);
    range_cache.location(&path, span.byte_start as usize, span.byte_end as usize)
}

fn solc_severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

fn json_level_severity(level: &str) -> DiagnosticSeverity {
    match level {
        "error" | "fatal" | "error: internal compiler error" => DiagnosticSeverity::ERROR,
        "warning" => DiagnosticSeverity::WARNING,
        "note" | "failure-note" | "gas" | "code-size" => DiagnosticSeverity::INFORMATION,
        "help" => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::WARNING,
    }
}

fn source(format: FlycheckOutput) -> &'static str {
    match format {
        FlycheckOutput::SolcJson => "flycheck",
        FlycheckOutput::ForgeLintJson => "forge-lint",
    }
}

fn resolve_path(file_loader: &dyn FileLoader, cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    let path = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    normalize_source_path(file_loader, path)
}

pub(super) fn normalize_source_path(file_loader: &dyn FileLoader, path: PathBuf) -> PathBuf {
    let has_parent = path.components().any(|component| component == Component::ParentDir);
    let normalized = path.normalize();
    if !has_parent {
        return normalized;
    }

    let Ok(canonical) = file_loader.canonicalize_path(&path) else { return normalized };
    let common_prefix = path
        .components()
        .zip(normalized.components())
        .take_while(|(original, normalized)| original == normalized)
        .map(|(component, _)| component)
        .collect::<PathBuf>();
    let Ok(canonical_prefix) = file_loader.canonicalize_path(&common_prefix) else {
        return canonical;
    };
    let Ok(suffix) = canonical.strip_prefix(canonical_prefix) else { return canonical };

    // Preserve unrelated path aliases while resolving parent traversal across symlinks.
    common_prefix.join(suffix)
}

struct ByteRangeCache<'a> {
    source_map: SourceMap,
    source_snapshot: Option<&'a SourceSnapshot>,
    files: FxHashMap<PathBuf, Rope>,
}

impl<'a> ByteRangeCache<'a> {
    fn new(source_snapshot: Option<&'a SourceSnapshot>) -> Self {
        Self { source_map: SourceMap::empty(), source_snapshot, files: FxHashMap::default() }
    }

    fn is_trusted(&self, path: &Path) -> bool {
        self.source_snapshot.is_none_or(|snapshot| snapshot.contains_key(path))
    }

    fn file(&mut self, path: &Path) -> Option<&Rope> {
        if let Some(file) = self.source_snapshot.and_then(|snapshot| snapshot.get(path)) {
            return Some(file);
        }
        if !self.files.contains_key(path) {
            let contents = self.source_map.file_loader().load_file(path).ok()?;
            self.files.insert(path.to_path_buf(), Rope::from(contents));
        }
        self.files.get(path)
    }

    fn checked_range(&mut self, path: &Path, start: usize, end: usize) -> Option<Range> {
        let file = self.file(path)?;
        if start > end
            || end > file.byte_len()
            || !file.is_char_boundary(start)
            || !file.is_char_boundary(end)
        {
            return None;
        }
        Some(Range { start: position_at_byte(file, start), end: position_at_byte(file, end) })
    }

    fn location(&mut self, path: &Path, start: usize, end: usize) -> Option<Location> {
        let uri = Url::from_file_path(path).ok()?;
        let range = self.checked_range(path, start, end)?;
        Some(Location::new(uri, range))
    }
}

fn position_at_byte(file: &Rope, byte: usize) -> Position {
    let byte = byte.min(file.byte_len());
    let line = file.line_of_byte(byte);
    let line_start = file.byte_of_line(line);
    let character = file.utf16_code_unit_of_byte(byte) - file.utf16_code_unit_of_byte(line_start);

    Position { line: line as u32, character: character as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;
    use lsp_types::{DiagnosticRelatedInformation, DiagnosticTag};
    use snapbox::{assert_data_eq, str};
    use solar_interface::diagnostics::{
        JsonDiagnosticCode, JsonDiagnosticSpanLine, SolcDiagnostic, SourceLocation,
    };
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    const SOURCE: &str = "contract Test {\n    uint256 bad_name;\n    string value = \"🚀\";\n}\n";

    fn project() -> (TestProject, Url) {
        let project = TestProject::new();
        project.write_file("/src/Test.sol", SOURCE);
        project.write_file("/src/Base.sol", "contract Base {}");
        let uri = project.uri("/src/Test.sol");
        (project, uri)
    }

    fn parse_all(
        project: &TestProject,
        output: impl AsRef<[u8]>,
        format: FlycheckOutput,
    ) -> DiagnosticMap {
        parse(output.as_ref(), project.root(), format, None).unwrap()
    }

    fn range(line: u32, start: u32, end: u32) -> Range {
        Range::new(Position::new(line, start), Position::new(line, end))
    }

    fn code(code: &str) -> Option<NumberOrString> {
        Some(NumberOrString::String(code.into()))
    }

    fn bad_name() -> (usize, usize) {
        let start = SOURCE.find("bad_name").unwrap();
        (start, start + "bad_name".len())
    }

    #[test]
    fn parses_solc_json_records() {
        let (project, uri) = project();
        let (start, end) = bad_name();
        let rocket = SOURCE.find('🚀').unwrap();
        let file = project.path("/src/Test.sol").to_string_lossy().into_owned();
        let output = [
            serde_json::to_string(&[solc(
                file,
                start,
                end,
                Severity::Warning,
                Some("2018"),
                "array",
            )]),
            serde_json::to_string(&serde_json::json!({
                "errors": [solc("src/Test.sol", 0, 8, Severity::Error, Some("1234"), "envelope")]
            })),
            serde_json::to_string(&serde_json::json!({
                "sourceLocation": { "file": "src/Test.sol", "start": -1, "end": -1 },
                "type": "Warning",
                "component": "general",
                "severity": "info",
                "errorCode": "1878",
                "message": "file level"
            })),
            serde_json::to_string(&solc(
                "src/Test.sol",
                rocket,
                rocket + "🚀".len(),
                Severity::Warning,
                None,
                "rocket",
            )),
        ]
        .map(Result::unwrap)
        .join("\n");

        let diagnostics = parse_all(&project, output, FlycheckOutput::SolcJson);

        let diagnostics = &diagnostics[&uri];
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.message.as_str(),
                    diagnostic.range,
                    diagnostic.severity.unwrap(),
                    diagnostic.code.clone()
                ))
                .collect::<Vec<_>>(),
            [
                ("array", range(1, 12, 20), DiagnosticSeverity::WARNING, code("2018")),
                ("envelope", range(0, 0, 8), DiagnosticSeverity::ERROR, code("1234")),
                ("file level", Range::default(), DiagnosticSeverity::INFORMATION, code("1878")),
                ("rocket", range(2, 20, 22), DiagnosticSeverity::WARNING, None),
            ]
        );
        for diagnostic in diagnostics {
            assert_eq!(diagnostic.source.as_deref(), Some("flycheck"));
            assert!(diagnostic.data.is_some());
        }
    }

    #[test]
    fn quick_fix_metadata_uses_the_flycheck_start_snapshot_for_equivalent_paths() {
        let (project, uri) = project();
        project.write_file("/tools/config", "config");
        let (start, end) = bad_name();
        let json = serde_json::to_string(&[solc(
            "../src/Test.sol",
            start,
            end,
            Severity::Warning,
            Some("2018"),
            "diagnostic",
        )])
        .unwrap();
        let snapshot =
            SourceSnapshot::from_iter([(project.path("/src/Test.sol"), Rope::from(SOURCE))]);
        project.write_file("/src/Test.sol", &SOURCE.replace("bad_name", "new_name"));
        let parse = |snapshot| {
            parse(
                json.as_bytes(),
                &project.path("/tools"),
                FlycheckOutput::SolcJson,
                Some(snapshot),
            )
            .unwrap()
        };

        let diagnostics = parse(&snapshot);
        let data = diagnostics[&uri][0].data.as_ref().expect("snapshot should carry metadata");
        assert_eq!(data["sourceFingerprint"], crate::code_actions::source_fingerprint(SOURCE));
        assert!(parse(&SourceSnapshot::default())[&uri][0].data.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_parent_components_after_symlinks_follow_filesystem_semantics() {
        let project = TestProject::from_fixture(
            r#"
            //- /actual/Target.sol
            contract Target { uint256 value; }
            //- /actual/nested/.keep
            keep
            //- /Target.sol
            contract LexicalTarget { uint256 value; }
            "#,
        );
        symlink(project.path("/actual/nested"), project.path("/link")).unwrap();
        let contents = project.read_file("/actual/Target.sol");
        let start = contents.find("value").unwrap();
        let json = serde_json::to_string(&[solc(
            "link/../Target.sol",
            start,
            start + "value".len(),
            Severity::Warning,
            Some("2018"),
            "diagnostic",
        )])
        .unwrap();

        let path = project.path("/actual/Target.sol");
        let snapshot = SourceSnapshot::from_iter([(path, Rope::from(contents))]);
        let diagnostics =
            parse(json.as_bytes(), project.root(), FlycheckOutput::SolcJson, Some(&snapshot))
                .unwrap();

        let uri = project.uri("/actual/Target.sol");
        assert_eq!(diagnostics.keys().collect::<Vec<_>>(), [&uri]);
        assert!(diagnostics[&uri][0].data.is_some());
    }

    #[test]
    fn parses_forge_lint_records_and_levels() {
        let (project, uri) = project();
        let (start, end) = bad_name();
        let levels = [
            ("note", DiagnosticSeverity::INFORMATION),
            ("gas", DiagnosticSeverity::INFORMATION),
            ("code-size", DiagnosticSeverity::INFORMATION),
            ("help", DiagnosticSeverity::HINT),
            ("error", DiagnosticSeverity::ERROR),
            ("unknown", DiagnosticSeverity::WARNING),
        ];
        let mut output = String::from("{\"$message_type\":\"build_finished\",\"success\":true}\n");
        for (level, _) in levels {
            output += &serde_json::to_string(&forge(start, end, level, level, level)).unwrap();
            output.push('\n');
        }

        let diagnostics = parse_all(&project, output, FlycheckOutput::ForgeLintJson);

        let diagnostics = &diagnostics[&uri];
        assert_eq!(diagnostics.len(), levels.len());
        for (diagnostic, (level, severity)) in diagnostics.iter().zip(levels) {
            assert_eq!(diagnostic.source.as_deref(), Some("forge-lint"));
            assert_eq!(diagnostic.severity, Some(severity));
            assert_eq!(diagnostic.message, level);
            assert_eq!(diagnostic.code, code(level));
            assert_eq!(diagnostic.range, range(1, 12, 20));
        }
    }

    #[test]
    fn preserves_forge_lint_suggestion_alternatives_in_diagnostic_data() {
        let (project, uri) = project();
        let (start, end) = bad_name();
        let mut diagnostic = forge(
            start,
            end,
            "note",
            "mixed-case-variable",
            "mutable variables should use mixedCase",
        );
        let JsonDiagnosticMessage::Diagnostic(record) = &mut diagnostic;
        let suggestion = |replacement| {
            let JsonDiagnosticMessage::Diagnostic(mut suggestion) =
                forge(start, end, "help", "", "convert the name to mixedCase");
            suggestion.code = None;
            suggestion.spans[0].text = Vec::new();
            suggestion.spans[0].suggested_replacement = Some(Cow::Borrowed(replacement));
            suggestion.spans[0].suggestion_applicability = Some(Applicability::MachineApplicable);
            suggestion
        };
        record.children.extend([suggestion("badName"), suggestion("goodName")]);

        let diagnostics = parse_all(
            &project,
            serde_json::to_string(&diagnostic).unwrap(),
            FlycheckOutput::ForgeLintJson,
        );

        assert_data_eq!(
            diagnostics[&uri][0].message.as_str(),
            str![[r#"
mutable variables should use mixedCase
help: convert the name to mixedCase
"#]]
        );
        let data =
            diagnostics[&uri][0].data.as_ref().expect("Forge suggestions should be preserved");
        assert_eq!(data["version"], serde_json::json!(1));
        assert_eq!(data["sourceFingerprint"], crate::code_actions::source_fingerprint(SOURCE));
        assert_eq!(
            data["suggestions"],
            serde_json::json!([{
                "title": "convert the name to mixedCase",
                "applicability": "MachineApplicable",
                "alternatives": [
                    [{
                        "range": {
                            "start": { "line": 1, "character": 12 },
                            "end": { "line": 1, "character": 20 }
                        },
                        "newText": "badName"
                    }],
                    [{
                        "range": {
                            "start": { "line": 1, "character": 12 },
                            "end": { "line": 1, "character": 20 }
                        },
                        "newText": "goodName"
                    }]
                ]
            }])
        );
    }

    #[test]
    fn rejects_invalid_forge_primary_byte_ranges() {
        let (project, _) = project();
        let rocket = SOURCE.find('🚀').unwrap();
        let invalid_ranges = [
            (SOURCE.len() + 1, SOURCE.len() + 2),
            (rocket + "🚀".len(), rocket),
            (rocket + 1, rocket + 2),
        ];

        for (start, end) in invalid_ranges {
            let json = serde_json::to_string(&forge(start, end, "note", "invalid", "invalid"));
            let diagnostics = parse_all(&project, json.unwrap(), FlycheckOutput::ForgeLintJson);
            assert!(diagnostics.is_empty(), "accepted invalid range {start}..{end}");
        }
    }

    #[test]
    fn position_at_byte_handles_utf16_and_line_endings() {
        for (text, expected) in [
            ("", Position::new(0, 0)),
            ("plain", Position::new(0, 5)),
            ("🚀中文", Position::new(0, 4)),
            ("a\r\n🚀中", Position::new(1, 3)),
            ("a\r\n", Position::new(1, 0)),
            ("a\n", Position::new(1, 0)),
        ] {
            let rope = Rope::from(text);
            assert_eq!(position_at_byte(&rope, usize::MAX), expected, "{text:?}");
        }
    }

    #[test]
    fn preserves_solc_secondary_locations_and_unavailable_location_messages() {
        let (project, uri) = project();
        let json = serde_json::json!({
            "errors": [{
                "sourceLocation": { "file": "src/Test.sol", "start": 9, "end": 13 },
                "secondarySourceLocations": [
                    { "file": "src/Base.sol", "start": 9, "end": 13,
                      "message": "base declaration is here" },
                    { "file": "src/Missing.sol", "start": 0, "end": 1,
                      "message": "unavailable declaration" },
                    { "file": "src/Base.sol", "start": 99, "end": 100,
                      "message": "invalid declaration range" },
                    { "file": "src/Base.sol", "start": -1, "end": -1,
                      "message": "no source position is available" }
                ],
                "type": "TypeError",
                "component": "general",
                "severity": "error",
                "errorCode": "4334",
                "message": "cannot override non-virtual function"
            }]
        });

        let diagnostics = parse_all(&project, json.to_string(), FlycheckOutput::SolcJson);

        let diagnostic = &diagnostics[&uri][0];
        assert_data_eq!(
            diagnostic.message.as_str(),
            str![[r#"
cannot override non-virtual function
unavailable declaration
invalid declaration range
no source position is available
"#]]
        );
        assert_eq!(
            diagnostic.related_information.as_deref(),
            Some(
                [DiagnosticRelatedInformation {
                    location: Location::new(project.uri("/src/Base.sol"), range(0, 9, 13),),
                    message: "base declaration is here".into(),
                }]
                .as_slice()
            ),
        );
    }

    #[test]
    fn preserves_forge_span_labels_and_nested_child_diagnostics() {
        let (project, uri) = project();
        let mut json =
            serde_json::to_value(forge(9, 13, "note", "example-lint", "primary message")).unwrap();
        json["spans"][0]["label"] = "primary detail".into();
        let mut secondary = json["spans"][0].clone();
        secondary["file_name"] = "src/Base.sol".into();
        secondary["is_primary"] = false.into();
        secondary["label"] = "base declaration".into();
        json["spans"].as_array_mut().unwrap().push(secondary.clone());
        let mut unavailable = secondary.clone();
        unavailable["file_name"] = "src/Missing.sol".into();
        unavailable["label"] = "unavailable definition".into();
        json["spans"].as_array_mut().unwrap().push(unavailable.clone());
        let mut unlabeled = secondary.clone();
        unlabeled["byte_start"] = 0.into();
        unlabeled["byte_end"] = 8.into();
        unlabeled["label"] = serde_json::Value::Null;
        json["spans"].as_array_mut().unwrap().push(unlabeled);
        unavailable["label"] = serde_json::Value::Null;
        unavailable["is_primary"] = true.into();
        let mut context = secondary.clone();
        context["label"] = "related context".into();
        secondary["is_primary"] = true.into();
        secondary["label"] = serde_json::Value::Null;
        json["children"] = serde_json::json!([
            {
                "message": "use another name", "level": "help", "spans": [],
                "children": [{
                    "message": "names must be distinct", "level": "note",
                    "spans": [], "children": []
                }]
            },
            {
                "message": "related declaration", "level": "note",
                "spans": [secondary], "children": []
            },
            {
                "message": "source is unavailable", "level": "note",
                "spans": [unavailable], "children": []
            },
            {
                "message": "context without a primary span", "level": "note",
                "spans": [context], "children": []
            }
        ]);

        let diagnostics = parse_all(&project, json.to_string(), FlycheckOutput::ForgeLintJson);

        let diagnostic = &diagnostics[&uri][0];
        assert_data_eq!(
            diagnostic.message.as_str(),
            str![[r#"
primary message
primary detail
unavailable definition
help: use another name
note: names must be distinct
note: source is unavailable
note: context without a primary span
"#]]
        );
        let base = project.uri("/src/Base.sol");
        let related = |range, message: &str| DiagnosticRelatedInformation {
            location: Location::new(base.clone(), range),
            message: message.into(),
        };
        assert_eq!(
            diagnostic.related_information,
            Some(vec![
                related(range(0, 9, 13), "base declaration"),
                related(range(0, 0, 8), "primary message"),
                related(range(0, 9, 13), "related declaration"),
                related(range(0, 9, 13), "related context"),
            ]),
        );
    }

    #[test]
    fn classifies_diagnostic_tags_by_emitter_and_code() {
        let (project, uri) = project();
        for (format, rustc_style) in [
            (FlycheckOutput::SolcJson, false),
            (FlycheckOutput::ForgeLintJson, false),
            (FlycheckOutput::ForgeLintJson, true),
        ] {
            for (code, tag) in [
                ("8417", Some(DiagnosticTag::DEPRECATED)),
                ("2072", Some(DiagnosticTag::UNNECESSARY)),
                ("5667", Some(DiagnosticTag::UNNECESSARY)),
                (
                    "unused-import",
                    matches!(format, FlycheckOutput::ForgeLintJson)
                        .then_some(DiagnosticTag::UNNECESSARY),
                ),
                ("2018", None),
                ("unknown", None),
            ] {
                let message = "deprecated unused warning text is not a classifier";
                let json = if rustc_style {
                    serde_json::to_string(&forge(9, 13, "note", code, message))
                } else {
                    serde_json::to_string(&solc(
                        "src/Test.sol",
                        9,
                        13,
                        Severity::Warning,
                        Some(code),
                        message,
                    ))
                };
                let diagnostics = parse_all(&project, json.unwrap(), format);
                assert_eq!(
                    diagnostics[&uri][0].tags,
                    tag.map(|tag| vec![tag]),
                    "format={format:?}, rustc_style={rustc_style}, code={code}"
                );
            }
        }
    }

    fn solc<'a>(
        file: impl Into<Cow<'a, str>>,
        start: usize,
        end: usize,
        severity: Severity,
        code: Option<&'a str>,
        message: &'a str,
    ) -> SolcDiagnostic<'a> {
        SolcDiagnostic {
            source_location: Some(SourceLocation {
                file: file.into(),
                start: start as u32,
                end: end as u32,
                message: None,
            }),
            secondary_source_locations: Vec::new(),
            r#type: Cow::Borrowed("Warning"),
            component: Cow::Borrowed("general"),
            severity,
            error_code: code.map(Cow::Borrowed),
            message: Cow::Borrowed(message),
            formatted_message: None,
        }
    }

    fn forge<'a>(
        start: usize,
        end: usize,
        level: &'a str,
        code: &'a str,
        message: &'a str,
    ) -> JsonDiagnosticMessage<'a> {
        JsonDiagnosticMessage::Diagnostic(JsonDiagnostic {
            message: Cow::Borrowed(message),
            code: Some(JsonDiagnosticCode { code: Cow::Borrowed(code), explanation: None }),
            level: Cow::Borrowed(level),
            spans: vec![JsonDiagnosticSpan {
                file_name: Cow::Borrowed("src/Test.sol"),
                byte_start: start as u32,
                byte_end: end as u32,
                line_start: 1,
                line_end: 1,
                column_start: 1,
                column_end: 1,
                is_primary: true,
                text: vec![JsonDiagnosticSpanLine {
                    text: Cow::Borrowed(""),
                    highlight_start: 1,
                    highlight_end: 1,
                }],
                label: None,
                suggested_replacement: None,
                suggestion_applicability: None,
                expansion: None,
            }],
            children: Vec::new(),
            rendered: None,
        })
    }
}
