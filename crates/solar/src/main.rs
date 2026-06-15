//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{
    Subcommands, parse_cli_args, run_compiler_args, run_lsp_stdio, signal_handler, utils,
};
use solar_interface::panic_hook;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let solar_cli::Args { command, compile } = match parse_cli_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    let result = match command {
        Some(Subcommands::Lsp) => {
            let _guard = utils::init_logger(utils::LogDestination::Stderr);
            run_lsp_stdio()
        }
        None => {
            let _guard = utils::init_logger(Default::default());
            run_compiler_args(compile)
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
