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

/// Diagnostics and the disk inputs that remained stable for the entire command.
#[derive(Debug)]
pub(crate) struct FlycheckResult {
    pub(crate) diagnostics: DiagnosticMap,
    pub(crate) sources: SourceSnapshot,
    /// False if any expected input was missing, unreadable, or changed during the command.
    pub(crate) sources_unchanged: bool,
}

pub(crate) async fn run(
    config: FlycheckConfig,
    timeout: Duration,
    cancel: oneshot::Receiver<()>,
    source_paths: Vec<PathBuf>,
) -> Result<FlycheckResult, FlycheckError> {
    let source_snapshot = disk_source_snapshot(source_paths.clone()).await?;
    let output = command_output(&config, timeout, cancel).await?;
    let current_source_snapshot = disk_source_snapshot(source_paths).await?;
    let (sources, sources_unchanged) =
        stable_source_snapshot(source_snapshot, current_source_snapshot);
    tokio::task::spawn_blocking(move || {
        let diagnostics = match parse_output(&output, &config, Some(&sources)) {
            Ok(diagnostics) => diagnostics,
            Err(_) if !output.status.success() => return Err(command_failed(&output)),
            Err(error) => return Err(error.into()),
        };

        if !output.status.success() && diagnostics.is_empty() {
            return Err(command_failed(&output));
        }

        Ok(FlycheckResult { diagnostics, sources, sources_unchanged })
    })
    .await
    .map_err(io::Error::other)?
}

#[derive(Debug, Default)]
struct DiskSourceSnapshot {
    sources: SourceSnapshot,
    revisions: FxHashMap<PathBuf, FileRevision>,
    incomplete: bool,
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
            } else {
                snapshot.incomplete = true;
            }
        }
        snapshot
    })
    .await
    .map_err(io::Error::other)
}

fn stable_source_snapshot(
    source_snapshot: DiskSourceSnapshot,
    current_source_snapshot: DiskSourceSnapshot,
) -> (SourceSnapshot, bool) {
    let DiskSourceSnapshot { mut sources, revisions, incomplete } = source_snapshot;
    let source_count = sources.len();
    sources.retain(|path, contents| {
        current_source_snapshot
            .sources
            .get(path)
            .is_some_and(|current| current.byte_slice(..) == contents.byte_slice(..))
            && revisions.get(path).is_some_and(|revision| {
                current_source_snapshot.revisions.get(path) == Some(revision)
            })
    });
    let sources_unchanged = !incomplete
        && !current_source_snapshot.incomplete
        && sources.len() == source_count
        && sources.len() == current_source_snapshot.sources.len();
    (sources, sources_unchanged)
}

fn command_failed(output: &Output) -> FlycheckError {
    FlycheckError::Failed {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

fn parse_output(
    output: &Output,
    config: &FlycheckConfig,
    source_snapshot: Option<&SourceSnapshot>,
) -> Result<DiagnosticMap, parser::ParseError> {
    let parse = |output: &[u8]| parser::parse(output, &config.cwd, config.output, source_snapshot);
    if config.output != FlycheckOutput::ForgeLintJson {
        return parse(&output.stdout);
    }

    let stdout = parse_json_records(&output.stdout, parse);
    let stderr = parse_json_records(&output.stderr, parse);
    match (stdout, stderr) {
        (None, None) => Ok(DiagnosticMap::default()),
        (Some(result), None) | (None, Some(result)) => result,
        (Some(Ok(mut stdout)), Some(Ok(stderr))) => {
            for (uri, mut diagnostics) in stderr {
                stdout.entry(uri).or_default().append(&mut diagnostics);
            }
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
    parse: impl Fn(&[u8]) -> Result<DiagnosticMap, parser::ParseError>,
) -> Option<Result<DiagnosticMap, parser::ParseError>> {
    let mut json = Vec::new();
    let mut has_plain_text = false;
    for line in output.split(|byte| *byte == b'\n') {
        let line = line.trim_ascii();
        match line.first() {
            Some(b'{' | b'[') => {
                json.extend_from_slice(line);
                json.push(b'\n');
            }
            Some(_) => has_plain_text = true,
            None => {}
        }
    }
    (!json.is_empty()).then(|| parse(if has_plain_text { &json } else { output }))
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

fn read_pipe(mut pipe: impl AsyncRead + Send + Unpin + 'static) -> JoinHandle<io::Result<Vec<u8>>> {
    tokio::spawn(async move {
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
    use crate::test_support::TestProject;
    #[cfg(unix)]
    use crate::{config::negotiate_capabilities, test_support::process_exists};
    #[cfg(unix)]
    use std::os::unix::{fs::symlink, process::ExitStatusExt};
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[test]
    fn forge_lint_json_diagnostics_are_collected_from_either_stream() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let uri = lsp_types::Url::from_file_path(project.path("/src/Test.sol")).unwrap();
        let config = FlycheckConfig {
            id: "forge-lint".into(),
            command: "forge".into(),
            args: Vec::new(),
            cwd: project.root().to_path_buf(),
            workspace_root: project.root().to_path_buf(),
            output: FlycheckOutput::ForgeLintJson,
        };
        let messages = |stdout: Vec<u8>, stderr: Vec<u8>| {
            let output = Output { status: std::process::ExitStatus::from_raw(0), stdout, stderr };
            let diagnostics = parse_output(&output, &config, None).unwrap();
            diagnostics.get(&uri).map_or_else(Vec::new, |diagnostics| {
                diagnostics.iter().map(|diagnostic| diagnostic.message.clone()).collect()
            })
        };

        let mut stderr = b"forge warning\n".to_vec();
        stderr.extend(solc_diagnostic("stderr diagnostic"));
        let stdout = br#"{"$message_type":"build_finished","success":true}"#.to_vec();
        assert_eq!(messages(stdout, stderr), ["stderr diagnostic"]);
        let stdout = solc_diagnostic("stdout diagnostic");
        assert_eq!(messages(stdout, b"forge warning".to_vec()), ["stdout diagnostic"]);
        let stdout = solc_diagnostic("stdout diagnostic");
        assert_eq!(
            messages(stdout, solc_diagnostic("stderr diagnostic")),
            ["stdout diagnostic", "stderr diagnostic"]
        );
        assert!(messages(Vec::new(), b"forge warning\nanother warning\n".to_vec()).is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn source_snapshot_keeps_only_stable_disk_contents() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let path = project.path("/src/Test.sol");
        let missing = project.path("/Missing.sol");
        let snapshot = || disk_source_snapshot(vec![path.clone()]);

        let before = snapshot().await.unwrap();
        assert_eq!(before.sources[&path].byte_slice(..), "contract Test {}");
        let (sources, unchanged) = stable_source_snapshot(before, snapshot().await.unwrap());
        assert!(unchanged);
        assert_eq!(sources[&path].byte_slice(..), "contract Test {}");

        let before = snapshot().await.unwrap();
        project.write_file("/src/Test.sol", "new");
        let (sources, unchanged) = stable_source_snapshot(before, snapshot().await.unwrap());
        assert!(sources.is_empty());
        assert!(!unchanged);

        let before = disk_source_snapshot(vec![missing.clone()]).await.unwrap();
        let after = disk_source_snapshot(vec![missing]).await.unwrap();
        let (sources, unchanged) = stable_source_snapshot(before, after);
        assert!(sources.is_empty());
        assert!(!unchanged);
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
    async fn changed_sources_during_flycheck_invalidate_the_result() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Test.sol
            contract Test { uint256 old_name; }
            //- /src/Dependency.sol
            contract Dependency {}
            "#,
        );
        let path = project.path("/src/Test.sol");
        let dependency = project.path("/src/Dependency.sol");
        let original = project.read_file("/src/Test.sol");
        // Restoring writes the same bytes back, so only the timestamps can tell; on a file
        // system with coarse timestamps the restore could share the tick that created the
        // fixture, so age the fixture first and restore before other cases change it.
        let fixture = std::fs::File::options().write(true).open(&path).unwrap();
        fixture.set_modified(SystemTime::now() - Duration::from_secs(60)).unwrap();
        drop(fixture);
        let uri = lsp_types::Url::from_file_path(&path).unwrap();

        for (script, changed, restored) in [
            (
                "printf '%s' 'contract Test { uint256 temporary; }' > \"$1\"; \
                 printf '%s' \"$3\" > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"",
                &path,
                true,
            ),
            ("printf '%s\\n' 'contract Test { uint256 new_name; }' > \"$1\"", &path, false),
            (
                "printf '%s\\n' 'contract Dependency { uint256 changed; }' > \"$2\"",
                &dependency,
                false,
            ),
        ] {
            let config = FlycheckConfig {
                id: "changing-source".into(),
                command: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    format!("{script}; printf '%s\\n' \"$4\""),
                    "sh".into(),
                    path.display().to_string(),
                    dependency.display().to_string(),
                    original.clone(),
                    String::from_utf8(solc_diagnostic("diagnostic")).unwrap(),
                ],
                cwd: project.root().to_path_buf(),
                workspace_root: project.root().to_path_buf(),
                output: FlycheckOutput::SolcJson,
            };
            let (_cancel, cancelled) = oneshot::channel();
            let sources = vec![path.clone(), dependency.clone()];

            let result = run(config, Duration::from_secs(30), cancelled, sources).await.unwrap();

            assert!(!result.sources_unchanged, "{script}");
            assert_eq!(result.sources.contains_key(&path), changed != &path, "{script}");
            assert_eq!(result.sources.contains_key(&dependency), changed != &dependency);
            let [diagnostic] = result.diagnostics[&uri].as_slice() else { panic!("{script}") };
            assert_eq!(diagnostic.message, "diagnostic");
            assert_eq!(diagnostic.data.is_some(), changed != &path, "{script}");
            if restored {
                assert_eq!(project.read_file("/src/Test.sol"), original);
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn failed_forge_lint_with_non_json_stderr_reports_command_failure() {
        let project = TestProject::new();
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

    fn solc_diagnostic(message: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "sourceLocation": { "file": "src/Test.sol", "start": 9, "end": 13 },
            "type": "Warning",
            "component": "general",
            "severity": "warning",
            "errorCode": "1234",
            "message": message,
        }))
        .unwrap()
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
