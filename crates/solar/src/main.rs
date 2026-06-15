//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{Subcommands, parse_args, run_compiler_args, run_lsp_stdio, signal_handler, utils};
use solar_interface::panic_hook;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    let solar_cli::Args { command, compile, .. } = args;
    if matches!(command, Some(Subcommands::Lsp)) {
        let _guard = utils::init_logger(utils::LogDestination::Stderr);
        return match run_lsp_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }

    let _guard = utils::init_logger(Default::default());
    match run_compiler_args(compile) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
