use glob::{MatchOptions, Pattern, glob_with};
use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_interface::source_map::{FileLoader, SourceMap};
use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
    string::FromUtf8Error,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, process::Command, time};

pub(crate) async fn is_ignored(
    forge: &Path,
    path: &Path,
    root: &Path,
    timeout: Duration,
) -> Result<bool, FormatterError> {
    let ignores = resolved_formatter_ignores(forge, root, timeout).await?;
    Ok(matches_ignore(path, root, &ignores))
}

fn matches_ignore(path: &Path, root: &Path, ignores: &[String]) -> bool {
    let source_map = SourceMap::empty();
    let file_loader = source_map.file_loader();
    let normalized_root = root.normalize();
    let canonical_root =
        file_loader.canonicalize_path(root).unwrap_or_else(|_| normalized_root.clone());
    let path = canonicalize_or_normalize(file_loader, path, &normalized_root, &canonical_root);
    let options = MatchOptions { require_literal_separator: true, ..MatchOptions::new() };

    ignores.iter().any(|ignore| {
        let ignore = root.join(ignore.trim_end_matches(['/', '\\']));
        let lexical_ignore = normalize_under_root(&ignore, &normalized_root, &canonical_root);
        if Pattern::new(&lexical_ignore.to_string_lossy()).is_ok_and(|pattern| {
            path.ancestors()
                .take_while(|ancestor| ancestor.starts_with(&canonical_root))
                .any(|candidate| pattern.matches_path_with(candidate, options))
        }) {
            return true;
        }

        glob_with(&ignore.to_string_lossy(), options).is_ok_and(|paths| {
            paths.filter_map(Result::ok).any(|ignore| {
                let ignore = canonicalize_or_normalize(
                    file_loader,
                    &ignore,
                    &normalized_root,
                    &canonical_root,
                );
                path.ancestors()
                    .take_while(|ancestor| ancestor.starts_with(&canonical_root))
                    .any(|candidate| candidate == ignore)
            })
        })
    })
}

fn canonicalize_or_normalize(
    file_loader: &dyn FileLoader,
    path: &Path,
    root: &Path,
    canonical_root: &Path,
) -> PathBuf {
    file_loader
        .canonicalize_path(path)
        .unwrap_or_else(|_| normalize_under_root(path, root, canonical_root))
}

fn normalize_under_root(path: &Path, root: &Path, canonical_root: &Path) -> PathBuf {
    let path = path.normalize();
    // Keep lexical paths in the same root representation as canonicalized paths.
    path.strip_prefix(root).map_or_else(|_| path.clone(), |relative| canonical_root.join(relative))
}

async fn resolved_formatter_ignores(
    forge: &Path,
    root: &Path,
    timeout: Duration,
) -> Result<Vec<String>, FormatterError> {
    let mut command = Command::new(forge);
    command
        .args(["config", "--json", "--root"])
        .arg(root)
        .env("FOUNDRY_DISABLE_NIGHTLY_WARNING", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = time::timeout(timeout, command.output())
        .await
        .map_err(|_| FormatterError::ConfigTimeout)??;

    if !output.status.success() {
        return Err(FormatterError::ConfigFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let config = serde_json::from_slice::<ResolvedForgeConfig>(&output.stdout)
        .map_err(FormatterError::InvalidConfig)?;
    Ok(config.fmt.ignore)
}

#[derive(Deserialize)]
struct ResolvedForgeConfig {
    #[serde(default)]
    fmt: ResolvedFormatterConfig,
}

#[derive(Default, Deserialize)]
struct ResolvedFormatterConfig {
    #[serde(default)]
    ignore: Vec<String>,
}

pub(crate) async fn run(
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
    #[error("Forge config resolution timed out")]
    ConfigTimeout,
    #[error("failed to run Forge formatter: {0}")]
    Io(#[from] io::Error),
    #[error("Forge formatter failed with status {status:?}: {stderr}")]
    Failed { status: Option<i32>, stderr: String },
    #[error("Forge config failed with status {status:?}: {stderr}")]
    ConfigFailed { status: Option<i32>, stderr: String },
    #[error("Forge returned invalid config: {0}")]
    InvalidConfig(#[source] serde_json::Error),
    #[error("Forge formatter returned invalid UTF-8: {0}")]
    InvalidUtf8(#[source] FromUtf8Error),
    #[error("Forge formatter returned empty output")]
    EmptyOutput,
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use crate::test_support::{TestProject, process_exists};
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn foundry_ignore_patterns_match_normalized_and_canonical_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Exact.sol

            //- /src/Dot.sol

            //- /src/Parent.sol

            //- /src/Direct.sol

            //- /src/nested/Nested.sol

            //- /src/Target.sol

            //- /generated/Generated.sol

            //- /generated/nested/Generated.sol

            //- /vendor/Nested.sol

            //- /workspace/src/Target.sol
            "#,
        );
        symlink(project.path("/src/Target.sol"), project.path("/Alias.sol")).unwrap();
        symlink(project.path("/generated"), project.path("/linked")).unwrap();
        symlink(project.path("/workspace"), project.path("/alias")).unwrap();
        let ignores = ["src/Exact.sol", "generated/**/*.sol", "vendor/", "./src/Dot.sol"];

        for (path, root, ignores, expected) in [
            ("/src/Exact.sol", "/", &ignores[..], true),
            ("/generated/nested/Generated.sol", "/", &ignores, true),
            ("/vendor/Nested.sol", "/", &ignores, true),
            ("/src/Dot.sol", "/", &ignores, true),
            ("/src/Direct.sol", "/", &ignores, false),
            ("/src/Parent.sol", "/", &["src/../src/Parent.sol"], true),
            ("/src/Direct.sol", "/", &["src/*.sol"], true),
            ("/src/Unsaved.sol", "/", &["src/*.sol"], true),
            ("/src/Unsaved.sol", "/", &["src/Unsaved.sol"], true),
            ("/src/nested/Nested.sol", "/", &["src/*.sol"], false),
            ("/Alias.sol", "/", &["src/Target.sol"], true),
            ("/generated/Generated.sol", "/", &["linked/Generated.sol"], true),
            ("/generated/Generated.sol", "/", &["linked/*.sol"], true),
            ("/generated/nested/Generated.sol", "/", &["linked/*.sol"], false),
            ("/generated/nested/Generated.sol", "/", &["linked/*"], true),
            ("/linked/Unsaved.sol", "/", &["linked/*.sol"], true),
            ("/linked/Unsaved.sol", "/", &["linked/"], true),
            ("/alias/src/Target.sol", "/alias", &["src/Target.sol"], true),
        ] {
            let ignores = ignores.iter().map(|ignore| ignore.to_string()).collect::<Vec<_>>();
            assert_eq!(
                matches_ignore(&project.path(path), &project.path(root), &ignores),
                expected,
                "{path} with {ignores:?}"
            );
        }
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

        let output = run(&forge, project.root(), "contract Unformatted{}", Duration::from_secs(30))
            .await
            .unwrap();

        assert_eq!(output, "contract Formatted {}");
        assert_eq!(project.read_file("/fake-forge.stdin"), "contract Unformatted{}");
        assert_eq!(project.read_file("/fake-forge.env"), "1");
        assert_eq!(
            project.read_file("/fake-forge.args"),
            format!("fmt\n--raw\n--root\n{}\n-\n", project.root().display())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn formatter_failures_report_their_cause() {
        let project = TestProject::new();
        type Check = fn(&FormatterError) -> bool;
        for (script, check) in [
            (
                None,
                (|error| matches!(error, FormatterError::Io(error) if error.kind() == io::ErrorKind::NotFound))
                    as Check,
            ),
            (Some("printf 'format failed' >&2\nexit 7"), |error| {
                matches!(
                    error,
                    FormatterError::Failed { status: Some(7), stderr } if stderr == "format failed"
                )
            }),
            (Some("printf '\\377'"), |error| matches!(error, FormatterError::InvalidUtf8(_))),
        ] {
            let forge = match script {
                Some(script) => {
                    write_executable(&project, "/fake-forge", &format!("#!/bin/sh\n{script}\n"))
                }
                None => project.path("/missing-forge"),
            };

            let error = run(&forge, project.root(), "", Duration::from_secs(30)).await.unwrap_err();

            assert!(check(&error), "{script:?}: {error:?}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timeout_and_cancellation_kill_forge_process() {
        let project = TestProject::new();
        let forge = write_executable(
            &project,
            "/fake-forge",
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.pid.tmp\"\nmv \"$0.pid.tmp\" \"$0.pid\"\nexec sleep 120\n",
        );
        let pid_path = project.path("/fake-forge.pid");

        let error = run(&forge, project.root(), "", Duration::from_secs(5)).await.unwrap_err();

        assert!(matches!(error, FormatterError::Timeout));
        assert_process_stopped(project.read_file("/fake-forge.pid").parse().unwrap()).await;

        fs::remove_file(&pid_path).unwrap();
        let root = project.root().to_path_buf();
        let task =
            tokio::spawn(async move { run(&forge, &root, "", Duration::from_secs(60)).await });
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

    pub(crate) fn write_executable(project: &TestProject, path: &str, contents: &str) -> PathBuf {
        project.write_file(path, contents);
        let path = project.path(path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
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
