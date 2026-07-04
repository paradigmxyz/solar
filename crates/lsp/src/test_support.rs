use crate::{
    config::{Config, negotiate_capabilities},
    vfs::{Vfs, VfsPath},
};
use crop::Rope;
use lsp_types::{InitializeParams, Position, Url, WorkspaceFolder};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

pub(crate) struct TestProject {
    tmp: TempDir,
    open_files: Vec<(PathBuf, String)>,
}

pub(crate) struct MarkedProject {
    project: TestProject,
    markers: BTreeMap<String, Vec<Marker>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Marker {
    path: String,
    position: Position,
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

impl MarkedProject {
    pub(crate) fn from_fixture(fixture: &str) -> Self {
        let mut project = TestProject::new();
        let mut markers = BTreeMap::<String, Vec<Marker>>::new();
        for mut file in parse_fixture(fixture) {
            let file_markers = strip_markers(&mut file.text);
            for (name, position) in file_markers {
                markers.entry(name).or_default().push(Marker { path: file.path.clone(), position });
            }
            project.write_file(&file.path, &file.text);
            if file.open {
                project.open_file(&file.path, &file.text);
            }
        }
        Self { project, markers }
    }

    pub(crate) fn project(&self) -> &TestProject {
        &self.project
    }

    pub(crate) fn marker(&self, name: &str) -> Marker {
        let name = normalize_marker_name(name);
        let markers = self.markers.get(name).unwrap_or_else(|| panic!("missing marker `${name}`"));
        assert_eq!(markers.len(), 1, "marker `${name}` is ambiguous: {markers:?}");
        markers[0].clone()
    }
}

impl Marker {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn position(&self) -> Position {
        self.position
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
