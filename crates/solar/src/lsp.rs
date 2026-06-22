use solar_cli::LspArgs;
use std::process::ExitCode;

pub(crate) fn run(args: LspArgs) -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(solar_lsp::run_server_stdio(args))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
