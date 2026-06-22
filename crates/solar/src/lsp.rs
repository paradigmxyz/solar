use solar_cli::LspArgs;
use std::process::ExitCode;

pub(crate) fn run(_args: LspArgs) -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap()
        .block_on(solar_lsp::run_server_stdio())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
