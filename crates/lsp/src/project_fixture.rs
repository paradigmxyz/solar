//! Pure in-memory project fixtures shared by tests and benchmarks.

use lsp_types::Position;
use std::{collections::BTreeMap, fmt};

/// An ordered collection of in-memory fixture files and source markers.
#[derive(Clone, Debug)]
pub(crate) struct ProjectFixture {
    files: Vec<FixtureFile>,
    markers: BTreeMap<String, Vec<FixtureMarker>>,
}

/// A file declared by a [`ProjectFixture`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureFile {
    path: String,
    text: String,
    open: bool,
}

/// A named position in a [`ProjectFixture`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureMarker {
    path: String,
    position: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureError {
    message: String,
}

impl FixtureError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FixtureError {}

impl ProjectFixture {
    /// Parses a fixture and removes its `$N` markers from the file contents.
    pub(crate) fn parse(fixture: &str) -> Self {
        Self::try_parse(fixture).unwrap_or_else(|error| panic!("{error}"))
    }

    pub(crate) fn try_parse(fixture: &str) -> Result<Self, FixtureError> {
        Self::parse_inner(fixture, true)
    }

    /// Parses a fixture without interpreting `$N` sequences as markers.
    pub(crate) fn parse_without_markers(fixture: &str) -> Self {
        Self::parse_inner(fixture, false).unwrap_or_else(|error| panic!("{error}"))
    }

    fn parse_inner(fixture: &str, extract_markers: bool) -> Result<Self, FixtureError> {
        let fixture = trim_indent(fixture);
        let mut files = Vec::new();
        let mut markers = BTreeMap::<String, Vec<FixtureMarker>>::new();
        let mut current = Option::<FixtureFile>::None;

        for line in fixture.split_inclusive('\n') {
            let marker_line = line.trim_end_matches(['\r', '\n']).trim_start();
            if let Some(meta) = marker_line.strip_prefix("//-") {
                if let Some(file) = current.take() {
                    files.push(finish_file(file, extract_markers, &mut markers));
                }
                current = Some(parse_meta(meta.trim())?);
            } else if let Some(file) = &mut current {
                file.text.push_str(line);
            } else if !line.trim().is_empty() {
                return Err(FixtureError::new(format!(
                    "fixture contents before first file marker: {line:?}"
                )));
            }
        }

        if let Some(file) = current {
            files.push(finish_file(file, extract_markers, &mut markers));
        }
        Ok(Self { files, markers })
    }

    pub(crate) fn files(&self) -> &[FixtureFile] {
        &self.files
    }

    #[cfg(feature = "bench")]
    pub(crate) fn markers(&self) -> &BTreeMap<String, Vec<FixtureMarker>> {
        &self.markers
    }

    pub(crate) fn marker(&self, name: &str) -> FixtureMarker {
        let name = normalize_marker_name(name);
        let markers = self.markers.get(name).unwrap_or_else(|| panic!("missing marker `${name}`"));
        assert_eq!(markers.len(), 1, "marker `${name}` is ambiguous: {markers:?}");
        markers[0].clone()
    }
}

impl FixtureFile {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

impl FixtureMarker {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn position(&self) -> Position {
        self.position
    }
}

fn finish_file(
    mut file: FixtureFile,
    extract_markers: bool,
    markers: &mut BTreeMap<String, Vec<FixtureMarker>>,
) -> FixtureFile {
    if file.text.ends_with('\n') {
        file.text.pop();
        if file.text.ends_with('\r') {
            file.text.pop();
        }
    }

    if extract_markers {
        for (name, position) in strip_markers(&mut file.text) {
            markers
                .entry(name)
                .or_default()
                .push(FixtureMarker { path: file.path.clone(), position });
        }
    }
    file
}

fn parse_meta(meta: &str) -> Result<FixtureFile, FixtureError> {
    let mut parts = meta.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| FixtureError::new("fixture marker must contain a path"))?
        .to_string();
    validate_path(&path)?;

    let mut open = false;
    for part in parts {
        match part {
            "open" => open = true,
            other => return Err(FixtureError::new(format!("unknown fixture option `{other}`"))),
        }
    }

    Ok(FixtureFile { path, text: String::new(), open })
}

fn validate_path(path: &str) -> Result<(), FixtureError> {
    let Some(relative) = path.strip_prefix('/') else {
        return Err(FixtureError::new(format!("fixture path must start with `/`: {path}")));
    };
    if relative.is_empty()
        || relative.split('/').any(|component| {
            component.is_empty()
                || matches!(component, "." | "..")
                || component.contains(['\\', ':'])
        })
    {
        return Err(FixtureError::new(format!(
            "fixture path must stay within the project root: {path}"
        )));
    }
    Ok(())
}

fn strip_markers(text: &mut String) -> Vec<(String, Position)> {
    let mut stripped = String::with_capacity(text.len());
    let mut markers = Vec::new();
    let mut chars = text.chars().peekable();
    let mut line = 0;
    let mut character = 0;

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            let mut name = String::new();
            while let Some(next) = chars.peek() {
                if next.is_ascii_digit() {
                    name.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            markers.push((name, Position { line, character }));
            continue;
        }

        stripped.push(ch);
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    *text = stripped;
    markers
}

fn normalize_marker_name(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

fn trim_indent(text: &str) -> String {
    let text = text.trim_matches('\n');
    let indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut trimmed = String::new();
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            trimmed.push_str(line.trim_start());
        } else {
            trimmed.push_str(line.get(indent..).unwrap_or(line));
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordered_multi_file_fixture() {
        let fixture = ProjectFixture::parse(
            r#"
                //- /src/First.sol open
                contract First {}
                //- /src/Second.sol
                contract Second {}
                //- /foundry.toml
                [profile.default]
            "#,
        );

        let files = fixture.files();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path(), "/src/First.sol");
        assert_eq!(files[0].text(), "contract First {}");
        assert!(files[0].is_open());
        assert_eq!(files[1].path(), "/src/Second.sol");
        assert_eq!(files[1].text(), "contract Second {}");
        assert!(!files[1].is_open());
        assert_eq!(files[2].path(), "/foundry.toml");
        assert_eq!(files[2].text(), "[profile.default]");
    }

    #[test]
    fn markers_use_utf16_positions() {
        let fixture = ProjectFixture::parse(concat!(
            "//- /Unicode.sol\n",
            "contract Unicode {\n",
            "    string value = \"\u{1F600}\";$0\n",
            "    function $12read() external {}\n",
            "}\n",
        ));

        let file = &fixture.files()[0];
        assert_eq!(
            file.text(),
            "contract Unicode {\n    string value = \"\u{1F600}\";\n    function read() external {}\n}"
        );
        assert_eq!(fixture.marker("0").position(), Position::new(1, 24));
        assert_eq!(fixture.marker("$12").position(), Position::new(2, 13));
        assert_eq!(fixture.marker("12").path(), "/Unicode.sol");
    }

    #[test]
    fn parsing_without_markers_preserves_marker_text() {
        let fixture = ProjectFixture::parse_without_markers(
            r#"
                //- /Raw.sol
                contract $0Raw {}
            "#,
        );

        assert_eq!(fixture.files()[0].text(), "contract $0Raw {}");
    }

    #[test]
    fn rejects_paths_that_can_escape_the_project_root() {
        for path in [
            "/../Outside.sol",
            "/src/../../Outside.sol",
            "/C:/Outside.sol",
            r"/src\..\Outside.sol",
            "//server/Outside.sol",
        ] {
            let fixture = format!("//- {path}\ncontract Outside {{}}");
            assert!(ProjectFixture::try_parse(&fixture).is_err(), "accepted `{path}`");
        }
    }

    #[test]
    #[should_panic(expected = "missing marker `$9`")]
    fn missing_marker_is_rejected() {
        ProjectFixture::parse("//- /Empty.sol\n").marker("$9");
    }

    #[test]
    #[should_panic(expected = "marker `$0` is ambiguous")]
    fn ambiguous_marker_is_rejected() {
        ProjectFixture::parse("//- /First.sol\n$0\n//- /Second.sol\n$0").marker("0");
    }
}
