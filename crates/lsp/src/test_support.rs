use crate::{
    config::{Config, negotiate_capabilities},
    project_fixture::{FixtureMarker, ProjectFixture},
    vfs::{Vfs, VfsPath},
};
use crop::Rope;
use lsp_types::{InitializeParams, Url, WorkspaceFolder};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[cfg(unix)]
pub(crate) fn process_exists(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) struct TestProject {
    tmp: TempDir,
    open_files: Vec<(PathBuf, String)>,
}

pub(crate) struct MarkedProject {
    project: TestProject,
    fixture: ProjectFixture,
}

impl TestProject {
    pub(crate) fn new() -> Self {
        Self { tmp: TempDir::new().unwrap(), open_files: Vec::new() }
    }

    pub(crate) fn from_fixture(fixture: &str) -> Self {
        Self::from_project_fixture(&ProjectFixture::parse_without_markers(fixture))
    }

    fn from_project_fixture(fixture: &ProjectFixture) -> Self {
        let mut project = Self::new();
        for file in fixture.files() {
            project.write_file(file.path(), file.text());
            if file.is_open() {
                project.open_file(file.path(), file.text());
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
            vfs.set_file_contents_with_version(
                VfsPath::from(path.clone()),
                Some(Rope::from(contents.as_str())),
                Some(0),
            );
        }
        vfs
    }
}

impl MarkedProject {
    pub(crate) fn from_fixture(fixture: &str) -> Self {
        let fixture = ProjectFixture::parse(fixture);
        let project = TestProject::from_project_fixture(&fixture);
        Self { project, fixture }
    }

    pub(crate) fn project(&self) -> &TestProject {
        &self.project
    }

    pub(crate) fn project_mut(&mut self) -> &mut TestProject {
        &mut self.project
    }

    pub(crate) fn marker(&self, name: &str) -> &FixtureMarker {
        self.fixture.marker(name)
    }
}
