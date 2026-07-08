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
    process::Command,
    task::JoinHandle,
    time,
};

const FLYCHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn run(config: FlycheckConfig) -> Result<DiagnosticMap, FlycheckError> {
    let output = command_output(&config, FLYCHECK_TIMEOUT).await?;
    let diagnostics = parser::parse(
        diagnostic_output(&output.stdout, &output.stderr, config.output),
        &config.cwd,
        config.output,
    )?;

    if !output.status.success() && diagnostics.is_empty() {
        return Err(FlycheckError::Failed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(diagnostics)
}

fn diagnostic_output<'a>(stdout: &'a [u8], stderr: &'a [u8], format: FlycheckOutput) -> &'a [u8] {
    if format == FlycheckOutput::ForgeLintJson && !stderr.is_empty() { stderr } else { stdout }
}

async fn command_output(
    config: &FlycheckConfig,
    timeout: Duration,
) -> Result<Output, FlycheckError> {
    let mut child = Command::new(&config.command)
        .args(&config.args)
        .current_dir(&config.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = read_pipe(child.stdout.take().expect("stdout was piped"));
    let stderr = read_pipe(child.stderr.take().expect("stderr was piped"));
    let status = match time::timeout(timeout, child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            let kill_result = child.kill().await;
            abort_pipe(stdout);
            abort_pipe(stderr);
            kill_result?;
            return Err(FlycheckError::Timeout);
        }
    };

    Ok(Output { status, stdout: collect_pipe(stdout).await?, stderr: collect_pipe(stderr).await? })
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

fn abort_pipe(pipe: JoinHandle<io::Result<Vec<u8>>>) {
    pipe.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::negotiate_capabilities, test_support::TestProject};
    use std::process::Command as StdCommand;

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

        let error = command_output(&config, Duration::from_secs(1)).await.unwrap_err();

        assert!(matches!(error, FlycheckError::Timeout));
        let pid = project.read_file("/flycheck-pid.txt").parse().unwrap();
        assert!(!process_exists(pid));
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        StdCommand::new("ps")
            .args(["-p", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FlycheckError {
    #[error("flycheck command timed out")]
    Timeout,
    #[error("failed to run flycheck command: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] parser::ParseError),
    #[error("flycheck command failed with status {status:?}: {stderr}")]
    Failed { status: Option<i32>, stderr: String },
}
