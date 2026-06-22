use solar_cli::LspArgs;
use std::process::ExitCode;

pub(crate) fn run(args: LspArgs) -> ExitCode {
    let server_args = solar_lsp::ServerArgs { stdio: args.stdio };
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(solar_lsp::run_server_stdio(server_args))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
