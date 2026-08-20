use solar_config::LspArgs;
use std::process::ExitCode;

pub(super) fn run(args: LspArgs) -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(solar_lsp::launch(solar_lsp::LaunchConfig::from(args)))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
