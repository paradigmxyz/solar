use crate::{
    diagnostics::DiagnosticMap,
    flycheck::{FlycheckConfig, config::FlycheckOutput, parser, parser::SourceSnapshot},
};
use crop::Rope;
use solar_interface::{data_structures::map::FxHashMap, source_map::SourceMap};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::oneshot,
    task::JoinHandle,
    time,
};

pub(crate) async fn run(
    config: FlycheckConfig,
    timeout: Duration,
    cancel: oneshot::Receiver<()>,
    source_paths: Vec<PathBuf>,
) -> Result<DiagnosticMap, FlycheckError> {
    let source_snapshot = disk_source_snapshot(source_paths.clone()).await?;
    let output = command_output(&config, timeout, cancel).await?;
    let current_source_snapshot = disk_source_snapshot(source_paths).await?;
    let source_snapshot = stable_source_snapshot(source_snapshot, &current_source_snapshot);
    let (output, diagnostics) = tokio::task::spawn_blocking(move || {
        let diagnostics = parse_output_with_snapshot(&output, &config, Some(&source_snapshot));
        (output, diagnostics)
    })
    .await
    .map_err(io::Error::other)?;
    let diagnostics = match diagnostics {
        Ok(diagnostics) => diagnostics,
        Err(_) if !output.status.success() => return Err(command_failed(&output)),
        Err(error) => return Err(error.into()),
    };

    if !output.status.success() && diagnostics.is_empty() {
        return Err(command_failed(&output));
    }

    Ok(diagnostics)
}

#[derive(Debug, Default)]
struct DiskSourceSnapshot {
    sources: SourceSnapshot,
    revisions: FxHashMap<PathBuf, FileRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileRevision {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    ctime: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
}

impl FileRevision {
    fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            ctime: metadata.ctime(),
            #[cfg(unix)]
            ctime_nsec: metadata.ctime_nsec(),
        })
    }
}

async fn disk_source_snapshot(paths: Vec<PathBuf>) -> io::Result<DiskSourceSnapshot> {
    tokio::task::spawn_blocking(move || {
        let source_map = SourceMap::empty();
        let mut snapshot = DiskSourceSnapshot::default();
        for path in paths {
            let path = parser::normalize_source_path(source_map.file_loader(), path);
            if let Ok(before) = FileRevision::read(&path)
                && let Ok(contents) = source_map.file_loader().load_file(&path)
                && let Ok(after) = FileRevision::read(&path)
                && before == after
            {
                snapshot.revisions.insert(path.clone(), after);
                snapshot.sources.insert(path, Rope::from(contents));
            }
        }
        snapshot
    })
    .await
    .map_err(io::Error::other)
}

fn stable_source_snapshot(
    source_snapshot: DiskSourceSnapshot,
    current_source_snapshot: &DiskSourceSnapshot,
) -> SourceSnapshot {
    let DiskSourceSnapshot { sources, revisions } = source_snapshot;
    sources
        .into_iter()
        .filter(|(path, contents)| {
            current_source_snapshot
                .sources
                .get(path)
                .is_some_and(|current| current.byte_slice(..) == contents.byte_slice(..))
                && revisions.get(path).is_some_and(|revision| {
                    current_source_snapshot.revisions.get(path) == Some(revision)
                })
        })
        .collect()
}

fn command_failed(output: &Output) -> FlycheckError {
    FlycheckError::Failed {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

#[cfg(test)]
fn parse_output(
    output: &Output,
    config: &FlycheckConfig,
) -> Result<DiagnosticMap, parser::ParseError> {
    parse_output_with_snapshot(output, config, None)
}

fn parse_output_with_snapshot(
    output: &Output,
    config: &FlycheckConfig,
    source_snapshot: Option<&SourceSnapshot>,
) -> Result<DiagnosticMap, parser::ParseError> {
    if config.output != FlycheckOutput::ForgeLintJson {
        return parse_diagnostics(&output.stdout, config, source_snapshot);
    }

    let stdout = parse_json_records(&output.stdout, config, source_snapshot);
    let stderr = parse_json_records(&output.stderr, config, source_snapshot);
    match (stdout, stderr) {
        (None, None) => Ok(DiagnosticMap::default()),
        (Some(result), None) | (None, Some(result)) => result,
        (Some(Ok(mut stdout)), Some(Ok(stderr))) => {
            merge_diagnostics(&mut stdout, stderr);
            Ok(stdout)
        }
        (Some(Ok(diagnostics)), Some(Err(_))) | (Some(Err(_)), Some(Ok(diagnostics))) => {
            Ok(diagnostics)
        }
        (Some(Err(error)), Some(Err(_))) => Err(error),
    }
}

fn parse_json_records(
    output: &[u8],
    config: &FlycheckConfig,
    source_snapshot: Option<&SourceSnapshot>,
) -> Option<Result<DiagnosticMap, parser::ParseError>> {
    let mut has_json = false;
    let mut has_plain_text = false;
    for line in output.split(|byte| *byte == b'\n') {
        match line.trim_ascii().first() {
            Some(b'{' | b'[') => has_json = true,
            Some(_) => has_plain_text = true,
            None => {}
        }
    }
    if !has_json {
        return None;
    }
    if !has_plain_text {
        return Some(parse_diagnostics(output, config, source_snapshot));
    }

    let mut json = Vec::new();
    for line in output.split(|byte| *byte == b'\n') {
        let line = line.trim_ascii();
        if matches!(line.first(), Some(b'{' | b'[')) {
            json.extend_from_slice(line);
            json.push(b'\n');
        }
    }

    (!json.is_empty()).then(|| parse_diagnostics(&json, config, source_snapshot))
}

fn parse_diagnostics(
    output: &[u8],
    config: &FlycheckConfig,
    source_snapshot: Option<&SourceSnapshot>,
) -> Result<DiagnosticMap, parser::ParseError> {
    match source_snapshot {
        Some(source_snapshot) => {
            parser::parse_from_snapshot(output, &config.cwd, config.output, source_snapshot)
        }
        None => parser::parse(output, &config.cwd, config.output),
    }
}

fn merge_diagnostics(into: &mut DiagnosticMap, diagnostics: DiagnosticMap) {
    for (uri, mut diagnostics) in diagnostics {
        into.entry(uri).or_default().append(&mut diagnostics);
    }
}

async fn command_output(
    config: &FlycheckConfig,
    timeout: Duration,
    mut cancel: oneshot::Receiver<()>,
) -> Result<Output, FlycheckError> {
    let mut child = Command::new(&config.command)
        .args(&config.args)
        .current_dir(&config.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = read_pipe(child.stdout.take().expect("stdout was piped"));
    let stderr = read_pipe(child.stderr.take().expect("stderr was piped"));
    let status = tokio::select! {
        status = child.wait() => status?,
        _ = time::sleep(timeout) => {
            kill_child(&mut child, &stdout, &stderr).await?;
            return Err(FlycheckError::Timeout);
        }
        _ = &mut cancel => {
            kill_child(&mut child, &stdout, &stderr).await?;
            return Err(FlycheckError::Cancelled);
        }
    };

    Ok(Output { status, stdout: collect_pipe(stdout).await?, stderr: collect_pipe(stderr).await? })
}

async fn kill_child(
    child: &mut Child,
    stdout: &JoinHandle<io::Result<Vec<u8>>>,
    stderr: &JoinHandle<io::Result<Vec<u8>>>,
) -> io::Result<()> {
    let result = child.kill().await;
    stdout.abort();
    stderr.abort();
    result
}

fn read_pipe(pipe: impl AsyncRead + Send + Unpin + 'static) -> JoinHandle<io::Result<Vec<u8>>> {
    tokio::spawn(async move {
        let mut pipe = pipe;
        let mut output = Vec::new();
        pipe.read_to_end(&mut output).await?;
        Ok(output)
    })
}

async fn collect_pipe(pipe: JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    pipe.await.map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::process_exists;
    use crate::{config::negotiate_capabilities, test_support::TestProject};
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[test]
    fn forge_lint_json_diagnostics_are_collected_from_stderr_when_stdout_is_nonempty() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let stdout = br#"{"$message_type":"build_finished","success":true}"#.to_vec();
        let mut stderr = b"forge warning\n".to_vec();
        stderr.extend(solc_diagnostic("stderr diagnostic"));
        let output = Output { status: success_status(), stdout, stderr };
        let config = forge_lint_config(&project);

        let diagnostics = parse_output(&output, &config).unwrap();

        let uri = lsp_types::Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        assert_eq!(diagnostics[&uri].len(), 1);
        assert_eq!(diagnostics[&uri][0].message, "stderr diagnostic");
    }

    #[test]
    fn forge_lint_json_diagnostics_are_collected_from_stdout_with_plain_stderr() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let output = Output {
            status: success_status(),
            stdout: solc_diagnostic("stdout diagnostic"),
            stderr: b"forge warning".to_vec(),
        };
        let config = forge_lint_config(&project);

        let diagnostics = parse_output(&output, &config).unwrap();

        let uri = lsp_types::Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        assert_eq!(diagnostics[&uri].len(), 1);
        assert_eq!(diagnostics[&uri][0].message, "stdout diagnostic");
    }

    #[test]
    fn forge_lint_plain_warnings_without_diagnostics_are_ignored() {
        let project = TestProject::new();
        let output = Output {
            status: success_status(),
            stdout: Vec::new(),
            stderr: b"forge warning\nanother warning\n".to_vec(),
        };
        let config = forge_lint_config(&project);

        assert!(parse_output(&output, &config).unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn changed_source_files_are_excluded_from_metadata_snapshot() {
        let project = TestProject::from_fixture(
            r#"
            //- /Test.sol
            old
            "#,
        );
        let path = project.path("/Test.sol");
        let before = disk_source_snapshot(vec![path.clone()]).await.unwrap();
        project.write_file("/Test.sol", "new");
        let after = disk_source_snapshot(vec![path]).await.unwrap();

        assert!(stable_source_snapshot(before, &after).is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn source_snapshot_reads_disk_contents() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let path = project.path("/src/Test.sol");

        let snapshot = disk_source_snapshot(vec![path.clone()]).await.unwrap();
        assert_eq!(snapshot.sources[&path].byte_slice(..), "contract Test {}");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn source_snapshot_parent_components_after_symlinks_follow_filesystem_semantics() {
        let project = TestProject::from_fixture(
            r#"
            //- /actual/Target.sol
            filesystem target
            //- /actual/nested/.keep
            keep
            //- /Target.sol
            lexical target
            "#,
        );
        symlink(project.path("/actual/nested"), project.path("/link")).unwrap();
        let path = project.path("/link/../Target.sol");
        let resolved = project.path("/actual/Target.sol");

        let snapshot = disk_source_snapshot(vec![path]).await.unwrap();

        assert_eq!(snapshot.sources.keys().collect::<Vec<_>>(), [&resolved]);
        assert_eq!(snapshot.sources[&resolved].byte_slice(..), "filesystem target");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn changed_source_during_flycheck_omits_quick_fix_metadata() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { uint256 old_name; }
            "#,
        );
        let path = project.path("/src/Test.sol");
        let diagnostic = String::from_utf8(solc_diagnostic("source changed")).unwrap();
        let config = FlycheckConfig {
            id: "changing-source".into(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '%s\\n' 'contract Test { uint256 new_name; }' > \"$1\"; printf '%s\\n' \"$2\""
                    .into(),
                "sh".into(),
                path.display().to_string(),
                diagnostic,
            ],
            cwd: project.root().to_path_buf(),
            workspace_root: project.root().to_path_buf(),
            output: FlycheckOutput::SolcJson,
        };
        let (_cancel, cancelled) = oneshot::channel();

        let diagnostics =
            run(config, Duration::from_secs(30), cancelled, vec![path.clone()]).await.unwrap();

        let uri = lsp_types::Url::from_file_path(path).unwrap();
        assert_eq!(diagnostics[&uri].len(), 1);
        assert_eq!(diagnostics[&uri][0].message, "source changed");
        assert!(diagnostics[&uri][0].data.is_none());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn restored_source_during_flycheck_omits_quick_fix_metadata() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { uint256 original; }
            "#,
        );
        let path = project.path("/src/Test.sol");
        let original = project.read_file("/src/Test.sol");
        let diagnostic = String::from_utf8(solc_diagnostic("source changed and restored")).unwrap();
        let config = FlycheckConfig {
            id: "restored-source".into(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '%s' 'contract Test { uint256 temporary; }' > \"$1\"; printf '%s\\n' \"$2\"; printf '%s' \"$3\" > \"$1\""
                    .into(),
                "sh".into(),
                path.display().to_string(),
                diagnostic,
                original.clone(),
            ],
            cwd: project.root().to_path_buf(),
            workspace_root: project.root().to_path_buf(),
            output: FlycheckOutput::SolcJson,
        };
        let (_cancel, cancelled) = oneshot::channel();

        let diagnostics =
            run(config, Duration::from_secs(30), cancelled, vec![path.clone()]).await.unwrap();

        assert_eq!(project.read_file("/src/Test.sol"), original);
        let uri = lsp_types::Url::from_file_path(path).unwrap();
        assert_eq!(diagnostics[&uri].len(), 1);
        assert_eq!(diagnostics[&uri][0].message, "source changed and restored");
        assert!(diagnostics[&uri][0].data.is_none());
    }

    fn forge_lint_config(project: &TestProject) -> FlycheckConfig {
        FlycheckConfig {
            id: "forge-lint".into(),
            command: "forge".into(),
            args: Vec::new(),
            cwd: project.root().to_path_buf(),
            workspace_root: project.root().to_path_buf(),
            output: FlycheckOutput::ForgeLintJson,
        }
    }

    fn solc_diagnostic(message: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "sourceLocation": {
                "file": "src/Test.sol",
                "start": 9,
                "end": 13,
            },
            "secondarySourceLocations": [],
            "type": "Warning",
            "component": "general",
            "severity": "warning",
            "errorCode": "1234",
            "message": message,
            "formattedMessage": null,
        }))
        .unwrap()
    }

    #[cfg(unix)]
    fn success_status() -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(windows)]
    fn success_status() -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn failed_forge_lint_with_non_json_stderr_reports_command_failure() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            "#,
        );
        let config = FlycheckConfig {
            id: "forge-lint".into(),
            command: "/bin/sh".into(),
            args: vec!["-c".into(), "printf 'compiler failed' >&2; exit 1".into()],
            cwd: project.root().to_path_buf(),
            workspace_root: project.root().to_path_buf(),
            output: FlycheckOutput::ForgeLintJson,
        };
        let (_cancel, cancelled) = oneshot::channel();

        let error = run(config, Duration::from_secs(30), cancelled, Vec::new()).await.unwrap_err();

        assert!(matches!(
            error,
            FlycheckError::Failed { status: Some(1), stderr } if stderr == "compiler failed"
        ));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn timeout_kills_child_process() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let pid_path = project.path("/flycheck-pid.txt");
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "timeout-repro",
                "command": "/bin/sh",
                "args": [
                    "-c",
                    "printf '%s' \"$$\" > \"$1\"; exec sleep 120",
                    "sh",
                    pid_path.display().to_string(),
                ],
            }],
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let [config] =
            config.flychecks_for_path(&project.path("/src/Test.sol")).try_into().unwrap();

        let (_cancel, cancelled) = oneshot::channel();
        let error = command_output(&config, Duration::from_secs(1), cancelled).await.unwrap_err();

        assert!(matches!(error, FlycheckError::Timeout));
        let pid = project.read_file("/flycheck-pid.txt").parse().unwrap();
        assert!(!process_exists(pid));
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FlycheckError {
    #[error("flycheck command timed out")]
    Timeout,
    #[error("flycheck command cancelled")]
    Cancelled,
    #[error("failed to run flycheck command: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] parser::ParseError),
    #[error("flycheck command failed with status {status:?}: {stderr}")]
    Failed { status: Option<i32>, stderr: String },
}
