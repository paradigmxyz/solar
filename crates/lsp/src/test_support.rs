use crate::{
    config::{Config, negotiate_capabilities},
    vfs::{Vfs, VfsPath},
};
use crop::Rope;
use lsp_types::{InitializeParams, Position, Url, WorkspaceFolder};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

pub(crate) struct TestProject {
    tmp: TempDir,
    open_files: Vec<(PathBuf, String)>,
}

pub(crate) struct FixtureCursor {
    pub(crate) path: PathBuf,
    pub(crate) position: Position,
    pub(crate) uri: Url,
}

pub(crate) struct FixtureWithCursor {
    pub(crate) project: TestProject,
    pub(crate) cursor: FixtureCursor,
    pub(crate) files: Vec<PathBuf>,
}

impl TestProject {
    pub(crate) fn new() -> Self {
        Self { tmp: TempDir::new().unwrap(), open_files: Vec::new() }
    }

    pub(crate) fn from_fixture(fixture: &str) -> Self {
        let mut project = Self::new();
        for file in parse_fixture(fixture) {
            project.write_file(&file.path, &file.text);
            if file.open {
                project.open_file(&file.path, &file.text);
            }
        }
        project
    }

    pub(crate) fn from_fixture_with_cursor(fixture: &str) -> FixtureWithCursor {
        let mut project = Self::new();
        let mut cursor = None;
        let mut files = Vec::new();
        for mut file in parse_fixture(fixture) {
            let path = project.path(&file.path);
            files.push(path.clone());
            if let Some((text, position)) = strip_cursor_marker(&file.text) {
                assert!(cursor.is_none(), "fixture contains multiple `$1` cursor markers");
                file.text = text;
                cursor = Some(FixtureCursor {
                    uri: Url::from_file_path(&path).unwrap(),
                    path,
                    position,
                });
            }
            project.write_file(&file.path, &file.text);
            if file.open {
                project.open_file(&file.path, &file.text);
            }
        }

        FixtureWithCursor {
            project,
            cursor: cursor.expect("fixture must contain one `$1` cursor marker"),
            files,
        }
    }

    pub(crate) fn root(&self) -> &Path {
        self.tmp.path()
    }

    pub(crate) fn path(&self, path: &str) -> PathBuf {
        self.tmp.path().join(path.strip_prefix('/').unwrap_or(path))
    }

    pub(crate) fn write_file(&self, path: &str, contents: &str) {
        let path = self.path(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    pub(crate) fn read_file(&self, path: &str) -> String {
        let mut contents = String::new();
        fs::File::open(self.path(path)).unwrap().read_to_string(&mut contents).unwrap();
        contents
    }

    pub(crate) fn open_file(&mut self, path: &str, contents: &str) {
        let path = self.path(path);
        if let Some((_, existing)) =
            self.open_files.iter_mut().find(|(candidate, _)| candidate == &path)
        {
            existing.clear();
            existing.push_str(contents);
        } else {
            self.open_files.push((path, contents.to_string()));
        }
    }

    pub(crate) fn remove_file(&self, path: &str) {
        fs::remove_file(self.path(path)).unwrap();
    }

    pub(crate) fn initialize_params(&self) -> InitializeParams {
        self.initialize_params_with_roots(&["/"])
    }

    pub(crate) fn initialize_params_with_roots(&self, roots: &[&str]) -> InitializeParams {
        InitializeParams {
            workspace_folders: Some(
                roots
                    .iter()
                    .map(|root| {
                        let path = self.path(root);
                        WorkspaceFolder {
                            uri: Url::from_file_path(&path).unwrap(),
                            name: path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("root")
                                .into(),
                        }
                    })
                    .collect(),
            ),
            ..Default::default()
        }
    }

    pub(crate) fn config(&self) -> Config {
        let (_, mut config) = negotiate_capabilities(self.initialize_params());
        config.rediscover_workspaces();
        config
    }

    pub(crate) fn config_with_roots(&self, roots: &[&str]) -> Config {
        let (_, mut config) = negotiate_capabilities(self.initialize_params_with_roots(roots));
        config.rediscover_workspaces();
        config
    }

    pub(crate) fn vfs(&self) -> Vfs {
        let mut vfs = Vfs::default();
        for (path, contents) in &self.open_files {
            vfs.set_file_contents(VfsPath::from(path.clone()), Some(Rope::from(contents.as_str())));
        }
        vfs
    }
}

struct FixtureFile {
    path: String,
    text: String,
    open: bool,
}

fn parse_fixture(fixture: &str) -> Vec<FixtureFile> {
    let fixture = trim_indent(fixture);
    let mut files = Vec::new();
    let mut current = Option::<FixtureFile>::None;

    for line in fixture.split_inclusive('\n') {
        let marker_line = line.trim_end_matches(['\r', '\n']).trim_start();
        if let Some(meta) = marker_line.strip_prefix("//-") {
            if let Some(file) = current.take() {
                files.push(finish_file(file));
            }
            current = Some(parse_meta(meta.trim()));
        } else if let Some(file) = &mut current {
            file.text.push_str(line);
        } else if !line.trim().is_empty() {
            panic!("fixture contents before first file marker: {line:?}");
        }
    }

    if let Some(file) = current {
        files.push(finish_file(file));
    }
    files
}

fn strip_cursor_marker(text: &str) -> Option<(String, Position)> {
    let marker = "$1";
    let offset = text.find(marker)?;
    assert!(
        !text[offset + marker.len()..].contains(marker),
        "fixture contains multiple `$1` cursor markers",
    );

    let position = position_at_offset(text, offset);
    let mut stripped = String::with_capacity(text.len() - marker.len());
    stripped.push_str(&text[..offset]);
    stripped.push_str(&text[offset + marker.len()..]);
    Some((stripped, position))
}

fn position_at_offset(text: &str, offset: usize) -> Position {
    assert!(text.is_char_boundary(offset), "cursor marker must be at a character boundary");
    let prefix = &text[..offset];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    Position { line, character }
}

fn finish_file(mut file: FixtureFile) -> FixtureFile {
    if file.text.ends_with('\n') {
        file.text.pop();
        if file.text.ends_with('\r') {
            file.text.pop();
        }
    }
    file
}

fn parse_meta(meta: &str) -> FixtureFile {
    let mut parts = meta.split_whitespace();
    let path = parts.next().expect("fixture marker must contain a path").to_string();
    assert!(path.starts_with('/'), "fixture path must start with `/`: {path}");

    let mut open = false;
    for part in parts {
        match part {
            "open" => open = true,
            other => panic!("unknown fixture option `{other}`"),
        }
    }

    FixtureFile { path, text: String::new(), open }
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
