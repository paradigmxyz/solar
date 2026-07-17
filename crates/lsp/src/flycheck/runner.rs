use crate::{
    diagnostics::DiagnosticMap,
    flycheck::{FlycheckConfig, config::FlycheckOutput, parser},
};
use std::{
    io,
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::oneshot,
    task::JoinHandle,
    time,
};

const FLYCHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn run(
    config: FlycheckConfig,
    cancel: oneshot::Receiver<()>,
) -> Result<DiagnosticMap, FlycheckError> {
    let output = command_output(&config, FLYCHECK_TIMEOUT, cancel).await?;
    let diagnostics = match parser::parse(
        diagnostic_output(&output.stdout, &output.stderr, config.output),
        &config.cwd,
        config.output,
    ) {
        Ok(diagnostics) => diagnostics,
        Err(_) if !output.status.success() => return Err(command_failed(&output)),
        Err(error) => return Err(error.into()),
    };

    if !output.status.success() && diagnostics.is_empty() {
        return Err(command_failed(&output));
    }

    Ok(diagnostics)
}

fn command_failed(output: &Output) -> FlycheckError {
    FlycheckError::Failed {
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

fn diagnostic_output<'a>(stdout: &'a [u8], stderr: &'a [u8], format: FlycheckOutput) -> &'a [u8] {
    if format == FlycheckOutput::ForgeLintJson && !stderr.is_empty() { stderr } else { stdout }
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

    #[test]
    fn forge_lint_json_diagnostics_are_read_from_stderr() {
        assert_eq!(
            diagnostic_output(b"", br#"{"message":"stdout"}"#, FlycheckOutput::ForgeLintJson),
            br#"{"message":"stdout"}"#
        );
    }

    #[test]
    fn solc_json_diagnostics_are_read_from_stdout() {
        assert_eq!(
            diagnostic_output(
                br#"{"message":"stdout"}"#,
                br#"{"message":"stderr"}"#,
                FlycheckOutput::SolcJson
            ),
            br#"{"message":"stdout"}"#
        );
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

        let error = run(config, cancelled).await.unwrap_err();

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
