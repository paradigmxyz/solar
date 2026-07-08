use crate::{diagnostics::DiagnosticMap, flycheck::config::FlycheckOutput};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};
use serde_json::Value;
use solar_interface::{data_structures::map::FxHashMap, source_map::SourceMap};
use std::path::{Path, PathBuf};

pub(super) fn parse(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
) -> Result<DiagnosticMap, ParseError> {
    let mut diagnostics = DiagnosticMap::default();
    let mut range_cache = ByteRangeCache::default();

    let stream = serde_json::Deserializer::from_slice(output).into_iter::<Value>();
    for value in stream {
        collect_diagnostics(&value?, cwd, format, &mut diagnostics, &mut range_cache);
    }

    Ok(diagnostics)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("failed to parse flycheck JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

fn collect_diagnostics(
    value: &Value,
    cwd: &Path,
    format: FlycheckOutput,
    diagnostics: &mut DiagnosticMap,
    range_cache: &mut ByteRangeCache,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_diagnostics(value, cwd, format, diagnostics, range_cache);
            }
        }
        Value::Object(object) => {
            for key in ["diagnostics", "findings", "errors"] {
                if let Some(value) = object.get(key) {
                    collect_diagnostics(value, cwd, format, diagnostics, range_cache);
                    return;
                }
            }

            if let Some((uri, diagnostic)) = external_diagnostic(value, cwd, format, range_cache) {
                diagnostics.entry(uri).or_default().push(diagnostic);
            }
        }
        _ => {}
    }
}

fn external_diagnostic(
    value: &Value,
    cwd: &Path,
    format: FlycheckOutput,
    range_cache: &mut ByteRangeCache,
) -> Option<(Url, Diagnostic)> {
    let location = source_location(value);
    let path = resolve_path(cwd, location.file?);
    let uri = Url::from_file_path(&path).ok()?;
    let range = location.range(&path, range_cache)?;
    let message = message(value)?;
    let code = diagnostic_code(value);

    Some((
        uri,
        Diagnostic {
            range,
            severity: Some(severity(value)),
            code: code.map(NumberOrString::String),
            code_description: None,
            source: Some(source(format).into()),
            message,
            related_information: None,
            tags: None,
            data: None,
        },
    ))
}

fn source_location(value: &Value) -> ExternalLocation<'_> {
    value
        .get("sourceLocation")
        .or_else(|| value.get("source_location"))
        .or_else(|| value.get("location"))
        .or_else(|| value.get("span"))
        .or_else(|| rustc_primary_span(value))
        .map_or_else(|| ExternalLocation::from_value(value), ExternalLocation::from_value)
}

fn message(value: &Value) -> Option<String> {
    string_field(value, &["message", "msg", "title", "description"]).map(ToOwned::to_owned).or_else(
        || string_field(value, &["formattedMessage", "formatted_message"]).map(clean_rendered),
    )
}

fn diagnostic_code(value: &Value) -> Option<String> {
    string_field(value, &["errorCode", "error_code", "code", "lint", "id", "name"])
        .or_else(|| value.get("code").and_then(|code| string_field(code, &["code"])))
        .map(ToOwned::to_owned)
}

fn severity(value: &Value) -> DiagnosticSeverity {
    let Some(severity) = string_field(value, &["severity", "level"]) else {
        return DiagnosticSeverity::WARNING;
    };

    match severity {
        "error" | "fatal" | "high" => DiagnosticSeverity::ERROR,
        "warning" | "warn" | "medium" | "med" | "low" | "gas" | "code-size" => {
            DiagnosticSeverity::WARNING
        }
        "info" | "information" => DiagnosticSeverity::INFORMATION,
        "hint" | "help" => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::WARNING,
    }
}

fn source(format: FlycheckOutput) -> &'static str {
    match format {
        FlycheckOutput::SolcJson => "flycheck",
        FlycheckOutput::ForgeLintJson => "forge-lint",
    }
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn clean_rendered(rendered: &str) -> String {
    rendered.lines().next().unwrap_or(rendered).to_string()
}

fn rustc_primary_span(value: &Value) -> Option<&Value> {
    let spans = value.get("spans")?.as_array()?;
    spans
        .iter()
        .find(|span| bool_field(span, &["is_primary"]).unwrap_or(false))
        .or_else(|| spans.first())
}

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
}

struct ExternalLocation<'a> {
    file: Option<&'a str>,
    start: Option<u64>,
    end: Option<u64>,
    line: Option<u64>,
    column: Option<u64>,
    end_line: Option<u64>,
    end_column: Option<u64>,
}

impl<'a> ExternalLocation<'a> {
    fn from_value(value: &'a Value) -> Self {
        Self {
            file: string_field(value, &["file", "path", "source", "filename"])
                .or_else(|| {
                    value
                        .get("source")
                        .and_then(|source| string_field(source, &["file", "path", "filename"]))
                })
                .or_else(|| string_field(value, &["file_name"])),
            start: numeric_field(value, &["start", "byteStart", "byte_start"]),
            end: numeric_field(value, &["end", "byteEnd", "byte_end"]),
            line: numeric_field(value, &["line", "startLine", "lineStart", "line_start"]),
            column: numeric_field(
                value,
                &["column", "col", "startColumn", "columnStart", "column_start"],
            ),
            end_line: numeric_field(value, &["endLine", "lineEnd", "line_end"]),
            end_column: numeric_field(value, &["endColumn", "columnEnd", "column_end"]),
        }
    }

    fn range(&self, path: &Path, range_cache: &mut ByteRangeCache) -> Option<Range> {
        if let (Some(start), Some(end)) = (self.start, self.end) {
            return range_cache.range(path, start as usize, end as usize);
        }

        let line = self.line?;
        let column = self.column.unwrap_or(1);
        let end_line = self.end_line.unwrap_or(line);
        let end_column = self.end_column.unwrap_or(column + 1);
        Some(Range {
            start: one_based_position(line, column),
            end: one_based_position(end_line, end_column),
        })
    }
}

fn numeric_field(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value.get(*key).and_then(Value::as_bool))
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

fn one_based_position(line: u64, column: u64) -> Position {
    Position { line: line.saturating_sub(1) as u32, character: column.saturating_sub(1) as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;
    use lsp_types::DiagnosticSeverity;

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
        let json = serde_json::json!([{
            "sourceLocation": {
                "file": file,
                "start": 20,
                "end": 24
            },
            "severity": "warning",
            "errorCode": "2018",
            "message": "function name should use mixedCase"
        }]);

        let diagnostics =
            parse(json.to_string().as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostic = &diagnostics[&uri][0];
        assert_eq!(diagnostic.source.as_deref(), Some("flycheck"));
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.message, "function name should use mixedCase");
        assert_eq!(diagnostic.code, Some(NumberOrString::String("2018".into())));
    }

    #[test]
    fn parses_enveloped_forge_lint_findings() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {
                uint256 bad_name;
            }
            "#,
        );
        let json = serde_json::json!({
            "findings": [{
                "sourceLocation": {
                    "file": "src/Test.sol",
                    "start": 20,
                    "end": 24
                },
                "severity": "med",
                "lint": "mixed-case-variable",
                "message": "mutable variables should use mixedCase"
            }]
        });

        let diagnostics =
            parse(json.to_string().as_bytes(), project.root(), FlycheckOutput::ForgeLintJson)
                .unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let diagnostic = &diagnostics[&uri][0];
        assert_eq!(diagnostic.source.as_deref(), Some("forge-lint"));
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostic.code, Some(NumberOrString::String("mixed-case-variable".into())));
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
        let variable = serde_json::json!({
            "$message_type": "diag",
            "message": "mutable variables should use mixedCase",
            "code": { "code": "mixed-case-variable", "explanation": null },
            "level": "note",
            "spans": [{
                "file_name": "src/Test.sol",
                "byte_start": variable_start,
                "byte_end": variable_end,
                "line_start": 2,
                "line_end": 2,
                "column_start": 25,
                "column_end": 33,
                "is_primary": true
            }]
        });
        let function = serde_json::json!({
            "$message_type": "diag",
            "message": "function names should use mixedCase",
            "code": { "code": "mixed-case-function", "explanation": null },
            "level": "note",
            "spans": [{
                "file_name": "src/Test.sol",
                "byte_start": function_start,
                "byte_end": function_end,
                "line_start": 4,
                "line_end": 4,
                "column_start": 26,
                "column_end": 38,
                "is_primary": true
            }]
        });
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
        let json = serde_json::json!([{
            "sourceLocation": {
                "file": "src/Test.sol",
                "start": start,
                "end": end
            },
            "message": "rocket"
        }]);

        let diagnostics =
            parse(json.to_string().as_bytes(), project.root(), FlycheckOutput::SolcJson).unwrap();

        let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let range = diagnostics[&uri][0].range;
        assert_eq!(range.end.character - range.start.character, 2);
    }
}
