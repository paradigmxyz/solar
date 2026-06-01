//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::{mir_opt, parse_args, run_compiler_args, signal_handler, utils};
use solar_interface::panic_hook;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

fn main() -> ExitCode {
    signal_handler::install();
    panic_hook::install();
    let _guard = utils::init_logger(Default::default());

    // Unstable `mir-opt` subcommand: run MIR passes on a .sol/.mir file (the
    // Solar equivalent of LLVM `opt`). Used by the `Mir` test mode.
    let mut argv = std::env::args_os();
    let prog = argv.next();
    let rest: Vec<std::ffi::OsString> = argv.collect();
    if rest.first().is_some_and(|a| a == "mir-opt") {
        return mir_opt::run(&rest[1..]);
    }

    let args = match parse_args(prog.into_iter().chain(rest)) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    match run_compiler_args(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
