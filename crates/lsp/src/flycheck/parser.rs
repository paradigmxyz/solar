use crate::{diagnostics::DiagnosticMap, flycheck::config::FlycheckOutput};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};
use serde_json::Value;
use solar_interface::source_map::SourceMap;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

pub(super) fn parse(
    output: &[u8],
    cwd: &Path,
    format: FlycheckOutput,
) -> Result<DiagnosticMap, ParseError> {
    let values = parse_values(output)?;
    let mut diagnostics = DiagnosticMap::default();

    for value in values {
        collect_diagnostics(value, cwd, format, &mut diagnostics);
    }

    Ok(diagnostics)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("failed to parse flycheck JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

fn parse_values(output: &[u8]) -> Result<Vec<Value>, ParseError> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }

    if let Ok(value) = serde_json::from_slice::<Value>(output) {
        return Ok(vec![value]);
    }

    let stream = serde_json::Deserializer::from_slice(output).into_iter::<Value>();
    stream.collect::<Result<Vec<_>, _>>().map_err(ParseError::Json)
}

fn collect_diagnostics(
    value: Value,
    cwd: &Path,
    format: FlycheckOutput,
    diagnostics: &mut DiagnosticMap,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_diagnostics(value, cwd, format, diagnostics);
            }
        }
        Value::Object(mut object) => {
            for key in ["diagnostics", "findings", "errors"] {
                if let Some(value) = object.remove(key) {
                    collect_diagnostics(value, cwd, format, diagnostics);
                    return;
                }
            }

            if let Some((uri, diagnostic)) =
                external_diagnostic(&Value::Object(object), cwd, format)
            {
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
) -> Option<(Url, Diagnostic)> {
    let location = source_location(value)?;
    let path = resolve_path(cwd, location.file.as_ref()?);
    let uri = Url::from_file_path(&path).ok()?;
    let range = location.range(&path)?;
    let message = message(value)?;
    let code = diagnostic_code(value);

    Some((
        uri,
        Diagnostic {
            range,
            severity: Some(severity(value, format)),
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

fn source_location(value: &Value) -> Option<ExternalLocation<'_>> {
    value
        .get("sourceLocation")
        .or_else(|| value.get("source_location"))
        .or_else(|| value.get("location"))
        .or_else(|| value.get("span"))
        .and_then(ExternalLocation::from_value)
        .or_else(|| ExternalLocation::from_value(value))
}

fn message(value: &Value) -> Option<String> {
    string_field(value, &["message", "msg", "title", "description"]).map(ToOwned::to_owned).or_else(
        || string_field(value, &["formattedMessage", "formatted_message"]).map(clean_rendered),
    )
}

fn diagnostic_code(value: &Value) -> Option<String> {
    string_field(value, &["errorCode", "error_code", "code", "lint", "id", "name"])
        .map(ToOwned::to_owned)
}

fn severity(value: &Value, format: FlycheckOutput) -> DiagnosticSeverity {
    let Some(severity) = string_field(value, &["severity", "level"]) else {
        return match format {
            FlycheckOutput::SolcJson => DiagnosticSeverity::WARNING,
            FlycheckOutput::ForgeLintJson => DiagnosticSeverity::WARNING,
        };
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

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
}

struct ExternalLocation<'a> {
    file: Option<Cow<'a, str>>,
    start: Option<u64>,
    end: Option<u64>,
    line: Option<u64>,
    column: Option<u64>,
    end_line: Option<u64>,
    end_column: Option<u64>,
}

impl<'a> ExternalLocation<'a> {
    fn from_value(value: &'a Value) -> Option<Self> {
        Some(Self {
            file: string_field(value, &["file", "path", "source", "filename"])
                .map(Cow::Borrowed)
                .or_else(|| {
                    value
                        .get("source")
                        .and_then(|source| string_field(source, &["file", "path", "filename"]))
                        .map(Cow::Borrowed)
                }),
            start: numeric_field(value, &["start", "byteStart", "byte_start"]),
            end: numeric_field(value, &["end", "byteEnd", "byte_end"]),
            line: numeric_field(value, &["line", "startLine", "lineStart", "line_start"]),
            column: numeric_field(value, &["column", "col", "startColumn", "columnStart"]),
            end_line: numeric_field(value, &["endLine", "lineEnd", "line_end"]),
            end_column: numeric_field(value, &["endColumn", "columnEnd", "column_end"]),
        })
    }

    fn range(&self, path: &Path) -> Option<Range> {
        if let (Some(start), Some(end)) = (self.start, self.end) {
            return byte_range(path, start as usize, end as usize);
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

fn byte_range(path: &Path, start: usize, end: usize) -> Option<Range> {
    let source_map = SourceMap::empty();
    let contents = source_map.file_loader().load_file(path).ok()?;
    Some(Range { start: position_at_byte(&contents, start), end: position_at_byte(&contents, end) })
}

fn position_at_byte(contents: &str, byte: usize) -> Position {
    let byte = byte.min(contents.len());
    let mut line = 0u32;
    let mut character = 0u32;

    for (idx, char) in contents.char_indices() {
        if idx >= byte {
            break;
        }
        if char == '\n' {
            line += 1;
            character = 0;
        } else {
            character += char.len_utf16() as u32;
        }
    }

    Position { line, character }
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
