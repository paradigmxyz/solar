use crate::{diagnostics::DiagnosticMap, flycheck::config::FlycheckOutput};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url,
};
use serde::Deserialize;
use solar_interface::{
    data_structures::map::FxHashMap,
    diagnostics::{
        JsonDiagnostic, JsonDiagnosticMessage, JsonDiagnosticSpan, Severity, SolcDiagnostic,
    },
    source_map::SourceMap,
};
use std::path::{Path, PathBuf};

pub(super) fn parse(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
) -> Result<DiagnosticMap, ParseError> {
    let mut diagnostics = DiagnosticMap::default();
    let mut range_cache = ByteRangeCache::default();
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
            let stream =
                serde_json::Deserializer::from_slice(output).into_iter::<JsonEmitterRecord<'_>>();
            for record in stream {
                collect_json_emitter(record?, cwd, source, &mut diagnostics, &mut range_cache);
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
    Diagnostic(#[serde(borrow)] SolcDiagnostic<'a>),
    Diagnostics(#[serde(borrow)] Vec<SolcDiagnostic<'a>>),
    Errors(#[serde(borrow)] SolcJsonErrors<'a>),
}

#[derive(Debug, Deserialize)]
struct SolcJsonErrors<'a> {
    #[serde(borrow)]
    errors: Vec<SolcDiagnostic<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonEmitterRecord<'a> {
    Rustc(#[serde(borrow)] JsonDiagnosticMessage<'a>),
    Solc(#[serde(borrow)] SolcDiagnostic<'a>),
}

fn collect_solc_json(
    record: SolcJsonRecord<'_>,
    cwd: &Path,
    source: &'static str,
    diagnostics: &mut DiagnosticMap,
    range_cache: &mut ByteRangeCache,
) {
    match record {
        SolcJsonRecord::Diagnostic(diagnostic) => {
            push_diagnostic(diagnostics, solc_diagnostic(diagnostic, cwd, source, range_cache));
        }
        SolcJsonRecord::Diagnostics(diagnostics_) => {
            for diagnostic in diagnostics_ {
                push_diagnostic(diagnostics, solc_diagnostic(diagnostic, cwd, source, range_cache));
            }
        }
        SolcJsonRecord::Errors(output) => {
            for diagnostic in output.errors {
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
    range_cache: &mut ByteRangeCache,
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
    diagnostic: SolcDiagnostic<'_>,
    cwd: &Path,
    source: &'static str,
    range_cache: &mut ByteRangeCache,
) -> Option<(Url, LspDiagnostic)> {
    let location = diagnostic.source_location?;
    let path = resolve_path(cwd, location.file.as_ref());
    let uri = Url::from_file_path(&path).ok()?;
    let range = range_cache.range(&path, location.start as usize, location.end as usize)?;

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
            data: None,
        },
    ))
}

fn json_diagnostic(
    diagnostic: JsonDiagnostic<'_>,
    cwd: &Path,
    source: &'static str,
    range_cache: &mut ByteRangeCache,
) -> Option<(Url, LspDiagnostic)> {
    let (path, byte_start, byte_end) = {
        let span = primary_span(&diagnostic)?;
        (
            resolve_path(cwd, span.file_name.as_ref()),
            span.byte_start as usize,
            span.byte_end as usize,
        )
    };

    let uri = Url::from_file_path(&path).ok()?;
    let range = range_cache.range(&path, byte_start, byte_end)?;

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
            data: None,
        },
    ))
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
        "note" | "failure-note" => DiagnosticSeverity::INFORMATION,
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

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
}

#[derive(Default)]
struct ByteRangeCache {
    source_map: SourceMap,
    files: FxHashMap<PathBuf, LineIndex>,
}

impl ByteRangeCache {
    fn range(&mut self, path: &Path, start: usize, end: usize) -> Option<Range> {
        if !self.files.contains_key(path) {
            let contents = self.source_map.file_loader().load_file(path).ok()?;
            self.files.insert(path.to_path_buf(), LineIndex::new(contents));
        }

        let file = self.files.get(path)?;
        Some(Range { start: file.position_at_byte(start), end: file.position_at_byte(end) })
    }
}

struct LineIndex {
    contents: String,
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(contents: String) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in contents.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }

        Self { contents, line_starts }
    }

    fn position_at_byte(&self, byte: usize) -> Position {
        let byte = byte.min(self.contents.len());
        let line = self.line_starts.partition_point(|start| *start <= byte).saturating_sub(1);
        let line_start = self.line_starts[line];
        let mut character = 0u32;

        for (idx, char) in self.contents[line_start..].char_indices() {
            if line_start + idx >= byte {
                break;
            }
            character += char.len_utf16() as u32;
        }

        Position { line: line as u32, character }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;
    use lsp_types::DiagnosticSeverity;
    use solar_interface::diagnostics::{
        JsonDiagnosticCode, JsonDiagnosticSpanLine, SourceLocation,
    };
    use std::borrow::Cow;

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
        assert_eq!(diagnostics[0].message, "mutable variables should use mixedCase");
        assert_eq!(diagnostics[0].code, Some(NumberOrString::String("mixed-case-variable".into())));
        assert_eq!(diagnostics[1].message, "function names should use mixedCase");
        assert_eq!(diagnostics[1].code, Some(NumberOrString::String("mixed-case-function".into())));
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
        JsonDiagnosticMessage::Diagnostic(JsonDiagnostic {
            message: Cow::Borrowed(message),
            code: Some(JsonDiagnosticCode { code: Cow::Borrowed(code), explanation: None }),
            level: Cow::Borrowed("note"),
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
            }],
            children: Vec::new(),
            rendered: None,
        })
    }
}
