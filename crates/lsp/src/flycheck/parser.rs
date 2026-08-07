use crate::{
    code_actions::{DiagnosticData, DiagnosticSuggestion, ranges_overlap},
    diagnostics::DiagnosticMap,
    flycheck::config::FlycheckOutput,
};
use crop::Rope;
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url,
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
) -> Result<DiagnosticMap, ParseError> {
    parse_with_snapshot(output, cwd, format, None)
}

pub(super) fn parse_from_snapshot(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
    source_snapshot: &SourceSnapshot,
) -> Result<DiagnosticMap, ParseError> {
    parse_with_snapshot(output, cwd, format, Some(source_snapshot))
}

fn parse_with_snapshot(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
    source_snapshot: Option<&SourceSnapshot>,
) -> Result<DiagnosticMap, ParseError> {
    let mut diagnostics = DiagnosticMap::default();
    let mut range_cache = ByteRangeCache::new(source_snapshot);
    let source = source(format);

    match format {
        FlycheckOutput::SolcJson => {
            let stream =
                serde_json::Deserializer::from_slice(output).into_iter::<SolcJsonRecord<'_>>();
            for record in stream {
                collect_solc_json(record?, cwd, source, &mut diagnostics, &mut range_cache);
            }
        }
        FlycheckOutput::ForgeLintJson => {
            let stream = serde_json::Deserializer::from_slice(output)
                .into_iter::<&serde_json::value::RawValue>();
            for raw in stream {
                let raw = raw?;
                let record = serde_json::from_str(raw.get());

                match record {
                    Ok(record) => collect_json_emitter(
                        record,
                        cwd,
                        source,
                        &mut diagnostics,
                        &mut range_cache,
                    ),
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

fn collect_solc_json(
    record: SolcJsonRecord<'_>,
    cwd: &Path,
    source: &'static str,
    diagnostics: &mut DiagnosticMap,
    range_cache: &mut ByteRangeCache<'_>,
) {
    match record {
        SolcJsonRecord::Diagnostic(diagnostic) => {
            push_diagnostic(diagnostics, solc_diagnostic(diagnostic, cwd, source, range_cache));
        }
        SolcJsonRecord::Diagnostics(diagnostics_)
        | SolcJsonRecord::Errors(SolcJsonErrors { errors: diagnostics_ }) => {
            for diagnostic in diagnostics_ {
                push_diagnostic(diagnostics, solc_diagnostic(diagnostic, cwd, source, range_cache));
            }
        }
    }
}

fn collect_json_emitter(
    record: JsonEmitterRecord<'_>,
    cwd: &Path,
    source: &'static str,
    diagnostics: &mut DiagnosticMap,
    range_cache: &mut ByteRangeCache<'_>,
) {
    match record {
        JsonEmitterRecord::Rustc(JsonDiagnosticMessage::Diagnostic(diagnostic)) => {
            push_diagnostic(diagnostics, json_diagnostic(diagnostic, cwd, source, range_cache));
        }
        JsonEmitterRecord::Solc(diagnostic) => {
            push_diagnostic(diagnostics, solc_diagnostic(diagnostic, cwd, source, range_cache));
        }
    }
}

fn push_diagnostic(diagnostics: &mut DiagnosticMap, diagnostic: Option<(Url, LspDiagnostic)>) {
    if let Some((uri, diagnostic)) = diagnostic {
        diagnostics.entry(uri).or_default().push(diagnostic);
    }
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

    Some((
        uri,
        LspDiagnostic {
            range,
            severity: Some(solc_severity(diagnostic.severity)),
            code: diagnostic.error_code.map(|code| NumberOrString::String(code.into_owned())),
            code_description: None,
            source: Some(source.into()),
            message: diagnostic.message.into_owned(),
            related_information: None,
            tags: None,
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
    let (path, byte_start, byte_end) = {
        let span = primary_span(&diagnostic)?;
        (
            resolve_path(range_cache.source_map.file_loader(), cwd, span.file_name.as_ref()),
            span.byte_start as usize,
            span.byte_end as usize,
        )
    };

    let uri = Url::from_file_path(&path).ok()?;
    let range = range_cache.checked_range(&path, byte_start, byte_end)?;
    let suggestions = json_suggestions(&diagnostic, cwd, &uri, range_cache);
    let data = diagnostic_data(range_cache, &path, uri.clone(), suggestions);

    Some((
        uri,
        LspDiagnostic {
            range,
            severity: Some(json_level_severity(diagnostic.level.as_ref())),
            code: diagnostic.code.map(|code| NumberOrString::String(code.code.into_owned())),
            code_description: None,
            source: Some(source.into()),
            message: diagnostic.message.into_owned(),
            related_information: None,
            tags: None,
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
    use lsp_types::DiagnosticSeverity;
    use solar_interface::diagnostics::{
        Applicability, JsonDiagnosticCode, JsonDiagnosticSpanLine, SolcDiagnostic, SourceLocation,
    };
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn parses_solc_like_diagnostics() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                function bad_name() public {}
            }
            "#,
        );
        let file = project.path("/src/Test.sol");
        let json = serde_json::to_string(&[solc_diagnostic_fixture(
            Cow::Owned(file.to_string_lossy().into_owned()),
            20,
            24,
            Severity::Warning,
            Some("2018"),
            "function name should use mixedCase",
        )])
        .unwrap();

        let diagnostics = parse(json.as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostic = &diagnostics[&uri][0];
        assert_eq!(diagnostic.source.as_deref(), Some("flycheck"));
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.message, "function name should use mixedCase");
        assert_eq!(diagnostic.code, Some(NumberOrString::String("2018".into())));
    }

    #[test]
    fn parses_standard_json_error_envelope() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;
            }
            "#,
        );
        let json = serde_json::json!({
            "errors": [solc_diagnostic_fixture(
                Cow::Borrowed("src/Test.sol"),
                20,
                24,
                Severity::Warning,
                Some("2018"),
                "mutable variables should use mixedCase",
            )]
        });

        let diagnostics =
            parse(json.to_string().as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostic = &diagnostics[&uri][0];
        assert_eq!(diagnostic.source.as_deref(), Some("flycheck"));
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.code, Some(NumberOrString::String("2018".into())));
    }

    #[test]
    fn maps_solc_file_level_diagnostics_to_document_start() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let json = serde_json::json!([{
            "sourceLocation": {
                "file": "src/Test.sol",
                "start": -1,
                "end": -1
            },
            "type": "Warning",
            "component": "general",
            "severity": "warning",
            "errorCode": "1878",
            "message": "SPDX license identifier not provided in source file."
        }]);

        let diagnostics =
            parse(json.to_string().as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostic = &diagnostics[&uri][0];
        assert_eq!(diagnostic.range, Range::default());
        assert!(diagnostic.data.is_some());
    }

    #[test]
    fn quick_fix_metadata_uses_the_flycheck_start_snapshot() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { uint256 old_name; }
            "#,
        );
        let file = project.path("/src/Test.sol");
        let source_at_start = project.read_file("/src/Test.sol");
        let start = source_at_start.find("old_name").unwrap();
        let json = serde_json::to_string(&[solc_diagnostic_fixture(
            Cow::Owned(file.to_string_lossy().into_owned()),
            start,
            start + "old_name".len(),
            Severity::Warning,
            Some("2018"),
            "mutable variables should use mixedCase",
        )])
        .unwrap();
        let mut snapshot = SourceSnapshot::default();
        snapshot.insert(file.clone(), Rope::from(source_at_start.as_str()));
        project.write_file("/src/Test.sol", "contract Test { uint256 new_name; }\n");

        let diagnostics = parse_from_snapshot(
            json.as_bytes(),
            project.root(),
            FlycheckOutput::SolcJson,
            &snapshot,
        )
        .unwrap();
        let uri = Url::from_file_path(file).unwrap();
        let data = diagnostics[&uri][0].data.as_ref().expect("snapshot should carry metadata");
        assert_eq!(
            data["sourceFingerprint"],
            crate::code_actions::source_fingerprint(&source_at_start)
        );

        let diagnostics = parse_from_snapshot(
            json.as_bytes(),
            project.root(),
            FlycheckOutput::SolcJson,
            &SourceSnapshot::default(),
        )
        .unwrap();
        assert!(diagnostics[&uri][0].data.is_none());
    }

    #[test]
    fn quick_fix_metadata_normalizes_equivalent_diagnostic_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { uint256 value; }
            //- /tools/config
            config
            "#,
        );
        let file = project.path("/src/Test.sol");
        let contents = project.read_file("/src/Test.sol");
        let start = contents.find("value").unwrap();
        let json = serde_json::to_string(&[solc_diagnostic_fixture(
            Cow::Borrowed("../src/Test.sol"),
            start,
            start + "value".len(),
            Severity::Warning,
            Some("2018"),
            "diagnostic",
        )])
        .unwrap();
        let snapshot = SourceSnapshot::from_iter([(file.clone(), Rope::from(contents))]);

        let diagnostics = parse_from_snapshot(
            json.as_bytes(),
            &project.path("/tools"),
            FlycheckOutput::SolcJson,
            &snapshot,
        )
        .unwrap();

        let uri = Url::from_file_path(file).unwrap();
        assert!(diagnostics[&uri][0].data.is_some());
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
        let json = serde_json::to_string(&[solc_diagnostic_fixture(
            Cow::Borrowed("link/../Target.sol"),
            start,
            start + "value".len(),
            Severity::Warning,
            Some("2018"),
            "diagnostic",
        )])
        .unwrap();

        let path = project.path("/actual/Target.sol");
        let snapshot = SourceSnapshot::from_iter([(path.clone(), Rope::from(contents))]);
        let diagnostics = parse_from_snapshot(
            json.as_bytes(),
            project.root(),
            FlycheckOutput::SolcJson,
            &snapshot,
        )
        .unwrap();

        let uri = Url::from_file_path(path).unwrap();
        assert_eq!(diagnostics.keys().collect::<Vec<_>>(), [&uri]);
        assert!(diagnostics[&uri][0].data.is_some());
    }

    #[test]
    fn parses_rustc_style_forge_lint_diagnostics() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;

                function bad_function() public {}
            }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let variable_start = contents.find("bad_name").unwrap();
        let variable_end = variable_start + "bad_name".len();
        let function_start = contents.find("bad_function").unwrap();
        let function_end = function_start + "bad_function".len();
        let variable = serde_json::to_string(&json_diagnostic_fixture(
            variable_start,
            variable_end,
            "mixed-case-variable",
            "mutable variables should use mixedCase",
        ))
        .unwrap();
        let function = serde_json::to_string(&json_diagnostic_fixture(
            function_start,
            function_end,
            "mixed-case-function",
            "function names should use mixedCase",
        ))
        .unwrap();
        let output = format!("{variable}\n{function}\n");

        let diagnostics =
            parse(output.as_bytes(), project.root(), FlycheckOutput::ForgeLintJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostics = &diagnostics[&uri];
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].source.as_deref(), Some("forge-lint"));
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::INFORMATION));
        assert_eq!(diagnostics[0].message, "mutable variables should use mixedCase");
        assert_eq!(diagnostics[0].code, Some(NumberOrString::String("mixed-case-variable".into())));
        assert_eq!(diagnostics[1].message, "function names should use mixedCase");
        assert_eq!(diagnostics[1].code, Some(NumberOrString::String("mixed-case-function".into())));
    }

    #[test]
    fn preserves_forge_lint_suggestion_alternatives_in_diagnostic_data() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;
            }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let start = contents.find("bad_name").unwrap();
        let end = start + "bad_name".len();
        let mut diagnostic = json_diagnostic_fixture(
            start,
            end,
            "mixed-case-variable",
            "mutable variables should use mixedCase",
        );
        let JsonDiagnosticMessage::Diagnostic(record) = &mut diagnostic;
        let suggestion = |replacement| JsonDiagnostic {
            message: Cow::Borrowed("convert the name to mixedCase"),
            code: None,
            level: Cow::Borrowed("help"),
            spans: vec![JsonDiagnosticSpan {
                file_name: Cow::Borrowed("src/Test.sol"),
                byte_start: start as u32,
                byte_end: end as u32,
                line_start: 1,
                line_end: 1,
                column_start: 1,
                column_end: 1,
                is_primary: true,
                text: Vec::new(),
                label: None,
                suggested_replacement: Some(Cow::Borrowed(replacement)),
                suggestion_applicability: Some(Applicability::MachineApplicable),
                expansion: None,
            }],
            children: Vec::new(),
            rendered: None,
        };
        record.children.extend([suggestion("badName"), suggestion("goodName")]);
        let json = serde_json::to_string(&diagnostic).unwrap();

        let diagnostics =
            parse(json.as_bytes(), project.root(), FlycheckOutput::ForgeLintJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let data =
            diagnostics[&uri][0].data.as_ref().expect("Forge suggestions should be preserved");
        assert_eq!(data["version"], serde_json::json!(1));
        assert_eq!(data["sourceFingerprint"], crate::code_actions::source_fingerprint(&contents));
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
    fn ignores_non_diagnostic_forge_json_records() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;
            }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let start = contents.find("bad_name").unwrap();
        let end = start + "bad_name".len();
        let diagnostic = serde_json::to_string(&json_diagnostic_fixture(
            start,
            end,
            "mixed-case-variable",
            "mutable variables should use mixedCase",
        ))
        .unwrap();
        let output =
            format!("{{\"$message_type\":\"build_finished\",\"success\":true}}\n{diagnostic}\n");

        let diagnostics =
            parse(output.as_bytes(), project.root(), FlycheckOutput::ForgeLintJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        assert_eq!(diagnostics[&uri].len(), 1);
        assert_eq!(diagnostics[&uri][0].message, "mutable variables should use mixedCase");
    }

    #[test]
    fn preserves_foundry_information_lint_severities() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;
            }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let start = contents.find("bad_name").unwrap();
        let end = start + "bad_name".len();
        let levels = ["note", "gas", "code-size"];
        let output = levels
            .map(|level| {
                serde_json::to_string(&json_diagnostic_fixture_with_level(
                    start,
                    end,
                    level,
                    level,
                    "foundry lint",
                ))
                .unwrap()
            })
            .join("\n");

        let diagnostics =
            parse(output.as_bytes(), project.root(), FlycheckOutput::ForgeLintJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostics = &diagnostics[&uri];
        assert_eq!(diagnostics.len(), levels.len());
        for diagnostic in diagnostics {
            assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::INFORMATION));
        }
    }

    #[test]
    fn byte_offsets_are_converted_to_utf16_positions() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                string value = "🚀";
            }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let start = contents.find('🚀').unwrap();
        let end = start + "🚀".len();
        let json = serde_json::to_string(&[solc_diagnostic_fixture(
            Cow::Borrowed("src/Test.sol"),
            start,
            end,
            Severity::Warning,
            None,
            "rocket",
        )])
        .unwrap();

        let diagnostics = parse(json.as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let range = diagnostics[&uri][0].range;
        assert_eq!(range.end.character - range.start.character, 2);
    }

    #[test]
    fn rejects_invalid_forge_primary_byte_ranges() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { string value = "🚀"; }
            "#,
        );
        let contents = project.read_file("/src/Test.sol");
        let rocket = contents.find('🚀').unwrap();
        let invalid_ranges = [
            (contents.len() + 1, contents.len() + 2),
            (rocket + "🚀".len(), rocket),
            (rocket + 1, rocket + 2),
        ];

        for (start, end) in invalid_ranges {
            let json = serde_json::to_string(&json_diagnostic_fixture(
                start,
                end,
                "invalid-range",
                "invalid range",
            ))
            .unwrap();
            let diagnostics =
                parse(json.as_bytes(), project.root(), FlycheckOutput::ForgeLintJson).unwrap();
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

    fn solc_diagnostic_fixture(
        file: Cow<'static, str>,
        start: usize,
        end: usize,
        severity: Severity,
        code: Option<&'static str>,
        message: &'static str,
    ) -> SolcDiagnostic<'static> {
        SolcDiagnostic {
            source_location: Some(SourceLocation {
                file,
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

    fn json_diagnostic_fixture(
        start: usize,
        end: usize,
        code: &'static str,
        message: &'static str,
    ) -> JsonDiagnosticMessage<'static> {
        json_diagnostic_fixture_with_level(start, end, "note", code, message)
    }

    fn json_diagnostic_fixture_with_level(
        start: usize,
        end: usize,
        level: &'static str,
        code: &'static str,
        message: &'static str,
    ) -> JsonDiagnosticMessage<'static> {
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
