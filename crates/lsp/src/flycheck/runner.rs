use crate::{
    diagnostics::DiagnosticMap,
    flycheck::{FlycheckConfig, parser},
};
use std::{process::Stdio, time::Duration};
use tokio::{process::Command, time};

const FLYCHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn run(config: FlycheckConfig) -> Result<DiagnosticMap, FlycheckError> {
    let output = time::timeout(FLYCHECK_TIMEOUT, command_output(&config))
        .await
        .map_err(|_| FlycheckError::Timeout)??;
    let diagnostics = parser::parse(&output.stdout, &config.cwd, config.output)?;

    if !output.status.success() && diagnostics.is_empty() {
        return Err(FlycheckError::Failed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(diagnostics)
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
