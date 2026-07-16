use crate::workspace::load_foundry_document;
use glob::{MatchOptions, Pattern};
use normalize_path::NormalizePath;
use std::{io, path::Path, process::Stdio, string::FromUtf8Error, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, time};

const FORMATTER_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn run(forge: &Path, root: &Path, source: &str) -> Result<String, FormatterError> {
    run_with_timeout(forge, root, source, FORMATTER_TIMEOUT).await
}

pub(crate) fn is_ignored(path: &Path, root: &Path) -> bool {
    let Ok(document) = load_foundry_document(&root.join("foundry.toml")) else {
        return false;
    };
    let root = root.normalize();
    let path = path.normalize();
    let options = MatchOptions { require_literal_separator: true, ..MatchOptions::new() };

    document.formatter_ignores().iter().any(|ignore| {
        let ignore = ignore.trim_end_matches(['/', '\\']);
        Pattern::new(&root.join(ignore).to_string_lossy()).is_ok_and(|pattern| {
            path.ancestors()
                .take_while(|ancestor| ancestor.starts_with(&root))
                .any(|candidate| pattern.matches_path_with(candidate, options))
        })
    })
}

async fn run_with_timeout(
    forge: &Path,
    root: &Path,
    source: &str,
    timeout: Duration,
) -> Result<String, FormatterError> {
    let mut child = Command::new(forge)
        .args(["fmt", "--raw", "--root"])
        .arg(root)
        .arg("-")
        .env("FOUNDRY_DISABLE_NIGHTLY_WARNING", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin was piped");

    let output = time::timeout(timeout, async {
        let write = async move {
            stdin.write_all(source.as_bytes()).await?;
            stdin.shutdown().await
        };
        let wait = child.wait_with_output();
        let (_, output) = tokio::try_join!(write, wait)?;
        Ok::<_, io::Error>(output)
    })
    .await
    .map_err(|_| FormatterError::Timeout)??;

    if !output.status.success() {
        return Err(FormatterError::Failed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let formatted = String::from_utf8(output.stdout).map_err(FormatterError::InvalidUtf8)?;
    if !source.trim().is_empty() && formatted.trim().is_empty() {
        return Err(FormatterError::EmptyOutput);
    }
    Ok(formatted)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FormatterError {
    #[error("Forge formatting timed out")]
    Timeout,
    #[error("failed to run Forge formatter: {0}")]
    Io(#[from] io::Error),
    #[error("Forge formatter failed with status {status:?}: {stderr}")]
    Failed { status: Option<i32>, stderr: String },
    #[error("Forge formatter returned invalid UTF-8: {0}")]
    InvalidUtf8(#[source] FromUtf8Error),
    #[error("Forge formatter returned empty output")]
    EmptyOutput,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::{TestProject, process_exists};
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

    #[test]
    fn foundry_ignore_patterns_match_files_and_directories() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [fmt]
            ignore = ["src/Exact.sol", "generated/**/*.sol", "vendor/"]

            //- /src/Exact.sol

            //- /generated/nested/Generated.sol

            //- /vendor/Nested.sol

            //- /src/Formatted.sol
            "#,
        );

        assert!(is_ignored(&project.path("/src/Exact.sol"), project.root()));
        assert!(is_ignored(&project.path("/generated/nested/Generated.sol"), project.root()));
        assert!(is_ignored(&project.path("/vendor/Nested.sol"), project.root()));
        assert!(!is_ignored(&project.path("/src/Formatted.sol"), project.root()));
    }

    #[test]
    fn foundry_ignore_globs_do_not_cross_directories() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [fmt]
            ignore = ["src/*.sol"]

            //- /src/Direct.sol

            //- /src/nested/Nested.sol
            "#,
        );

        assert!(is_ignored(&project.path("/src/Direct.sol"), project.root()));
        assert!(!is_ignored(&project.path("/src/nested/Nested.sol"), project.root()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forge_receives_source_root_arguments_and_warning_environment() {
        let project = TestProject::new();
        let forge = write_executable(
            &project,
            "/fake-forge",
            r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
printf '%s' "$FOUNDRY_DISABLE_NIGHTLY_WARNING" > "$0.env"
printf 'contract Formatted {}'
"#,
        );

        let output = run(&forge, project.root(), "contract Unformatted{}").await.unwrap();

        assert_eq!(output, "contract Formatted {}");
        assert_eq!(project.read_file("/fake-forge.stdin"), "contract Unformatted{}");
        assert_eq!(project.read_file("/fake-forge.env"), "1");
        assert_eq!(
            project.read_file("/fake-forge.args"),
            format!("fmt\n--raw\n--root\n{}\n-\n", project.root().display())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_forge_reports_io_error() {
        let project = TestProject::new();

        let error = run(&project.path("/missing-forge"), project.root(), "").await.unwrap_err();

        assert!(
            matches!(error, FormatterError::Io(error) if error.kind() == io::ErrorKind::NotFound)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn nonzero_exit_reports_status_and_stderr() {
        let project = TestProject::new();
        let forge = write_executable(
            &project,
            "/fake-forge",
            "#!/bin/sh\nprintf 'format failed' >&2\nexit 7\n",
        );

        let error = run(&forge, project.root(), "").await.unwrap_err();

        assert!(matches!(
            error,
            FormatterError::Failed { status: Some(7), stderr } if stderr == "format failed"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_utf8_output_is_rejected() {
        let project = TestProject::new();
        let forge = write_executable(&project, "/fake-forge", "#!/bin/sh\nprintf '\\377'\n");

        let error = run(&forge, project.root(), "").await.unwrap_err();

        assert!(matches!(error, FormatterError::InvalidUtf8(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timeout_kills_forge_process() {
        let project = TestProject::new();
        let forge = write_executable(
            &project,
            "/fake-forge",
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.pid.tmp\"\nmv \"$0.pid.tmp\" \"$0.pid\"\nexec sleep 120\n",
        );

        let error =
            run_with_timeout(&forge, project.root(), "", Duration::from_secs(5)).await.unwrap_err();

        assert!(matches!(error, FormatterError::Timeout));
        assert_process_stopped(project.read_file("/fake-forge.pid").parse().unwrap()).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_kills_forge_process() {
        let project = TestProject::new();
        let forge = write_executable(
            &project,
            "/fake-forge",
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.pid.tmp\"\nmv \"$0.pid.tmp\" \"$0.pid\"\nexec sleep 120\n",
        );
        let root = project.root().to_path_buf();
        let task = tokio::spawn(async move {
            run_with_timeout(&forge, &root, "", Duration::from_secs(60)).await
        });
        let pid_path = project.path("/fake-forge.pid");
        time::timeout(Duration::from_secs(5), async {
            while !pid_path.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let pid = project.read_file("/fake-forge.pid").parse().unwrap();

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_process_stopped(pid).await;
    }

    fn write_executable(project: &TestProject, path: &str, contents: &str) -> PathBuf {
        project.write_file(path, contents);
        let path = project.path(path);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    async fn assert_process_stopped(pid: u32) {
        let stopped = time::timeout(Duration::from_secs(5), async {
            while process_exists(pid) {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(stopped.is_ok(), "process {pid} is still running");
    }
}
