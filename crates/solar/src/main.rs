//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{parse_args, run_compiler_args, sigsegv_handler, utils};
use solar_interface::panic_hook;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    sigsegv_handler::install();
    panic_hook::install();
    let _guard = utils::init_logger();
    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };

    // If --lsp flag is provided, start the LSP server
    #[cfg(feature = "cli")]
    if args.lsp {
        return run_lsp_server();
    }

    match run_compiler_args(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

#[cfg(feature = "cli")]
fn run_lsp_server() -> ExitCode {
    use solar_lsp::SolarLanguageServer;
    use tower_lsp::{LspService, Server};

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(SolarLanguageServer::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });

    ExitCode::SUCCESS
}
