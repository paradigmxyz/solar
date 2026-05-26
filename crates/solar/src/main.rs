//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{parse_args, run_compiler_args, signal_handler, try_print_version, utils};
use solar_interface::panic_hook;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let _guard = utils::init_logger(Default::default());
    if try_print_version(std::env::args_os()) {
        return ExitCode::SUCCESS;
    }
    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    match run_compiler_args(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
