use crate::{
    diagnostics::DiagnosticMap,
    flycheck::{FlycheckConfig, config::FlycheckOutput, parser},
};
use std::{process::Stdio, time::Duration};
use tokio::{process::Command, time};

const FLYCHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn run(config: FlycheckConfig) -> Result<DiagnosticMap, FlycheckError> {
    let output = time::timeout(FLYCHECK_TIMEOUT, command_output(&config))
        .await
        .map_err(|_| FlycheckError::Timeout)??;
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

async fn command_output(config: &FlycheckConfig) -> Result<std::process::Output, std::io::Error> {
    Command::new(&config.command)
        .args(&config.args)
        .current_dir(&config.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
