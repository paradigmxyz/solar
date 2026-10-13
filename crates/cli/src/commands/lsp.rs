use solar_config::LspArgs;
use std::process::ExitCode;

pub(super) fn run(args: LspArgs) -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(solar_interface::thread_stack_size())
        .build()
        .unwrap()
        .block_on(solar_lsp::launch(args.into()))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
