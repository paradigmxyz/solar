//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{
    Subcommands, parse_args, run_compiler_args, signal_handler,
    utils::{self, LogDestination},
};
use solar_interface::panic_hook;
use std::process::ExitCode;
use tokio::runtime::Runtime;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };

    if let Some(Subcommands::Lsp) = args.commands {
        let _guard = utils::init_logger(LogDestination::Stderr);
        let rt = Runtime::new().unwrap();
        return match rt.block_on(solar_lsp::run_server_stdio()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }

    let _guard = utils::init_logger(LogDestination::Stdout);
    match run_compiler_args(args.default_compile) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
