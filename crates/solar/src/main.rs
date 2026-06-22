//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{
    Subcommands, mir_opt, parse_args, run_compiler_args, signal_handler,
    utils::{self, LogDestination},
};
use solar_interface::panic_hook;
use std::process::ExitCode;

mod lsp;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let _guard = utils::init_logger(LogDestination::Stderr);

    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };

    let solar_cli::Args { commands, default_compile } = args;
    match commands {
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::MirOpt(args)) => mir_opt::run(args),
        None => match run_compiler_args(default_compile) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
    }
}
