//! The main entry point for the Sulk compiler.

#![cfg_attr(feature = "nightly", feature(panic_update_hook))]

use clap::Parser as _;
use cli::Args;
use std::{path::Path, process::ExitCode};
use sulk_data_structures::{defer, sync::Lrc};
use sulk_interface::{
    diagnostics::{DiagCtxt, FatalError},
    Result, Session, SessionGlobals, SourceMap,
};

pub mod cli;
mod utils;

// Used in integration tests.
#[cfg(test)]
use sulk_tester as _;

fn main() -> ExitCode {
    let early_dcx = DiagCtxt::with_tty_emitter(None);

    utils::init_logger(&early_dcx);
    utils::install_panic_hook();

    FatalError::catch_with_exit_code(|| {
        let args = std::env::args_os()
            .enumerate()
            .map(|(i, arg)| {
                arg.into_string().unwrap_or_else(|arg| {
                    early_dcx.fatal(format!("argument {i} is not valid Unicode: {arg:?}")).emit()
                })
            })
            .collect::<Vec<_>>();
        run_compiler(&args)
    })
}

pub fn run_compiler(args: &[String]) -> Result<()> {
    let args = Args::parse_from(args);
    run_compiler_with(args, _run_compiler)
}

fn _run_compiler(Compiler { sess, args }: &Compiler) -> Result<()> {
    let is_yul = args.language.is_yul();
    let is_testing = || std::env::var_os("__SULK_IN_INTEGRATION_TEST").is_some_and(|s| s != "0");
    if is_yul && !is_testing() {
        return Err(sess.dcx.err("Yul is not supported yet").emit());
    }

    let mut resolver = sulk_resolve::Resolver::new(sess);
    for map in &args.import_map {
        resolver.file_resolver.add_import_map(map.map.clone(), map.path.clone());
    }
    for path in &args.import_path {
        let new = resolver.file_resolver.add_import_path(path.clone());
        if !new {
            let msg = format!("import path {} already specified", path.display());
            return Err(sess.dcx.err(msg).emit());
        }
    }

    let stdin = args.input.iter().any(|arg| *arg == Path::new("-"));
    let paths = args.input.iter().filter(|arg| *arg != Path::new("-"));
    resolver.parse_and_resolve(stdin, paths)?;

    sess.dcx.has_errors()?;

    Ok(())
}

pub struct Compiler {
    pub sess: Session,
    pub args: Args,
}

impl Compiler {
    fn finish_diagnostics(&self) {
        self.sess.dcx.print_error_count();
    }
}

fn run_compiler_with<R: Send>(args: Args, f: impl FnOnce(&Compiler) -> R + Send) -> R {
    utils::run_in_thread_with_globals(|| {
        let source_map = SourceMap::new();
        let color = match args.color {
            clap::ColorChoice::Always => sulk_interface::ColorChoice::Always,
            clap::ColorChoice::Auto => sulk_interface::ColorChoice::Auto,
            clap::ColorChoice::Never => sulk_interface::ColorChoice::Never,
        };
        let mut sess = Session::with_tty_emitter_and_color(Lrc::new(source_map), color);
        sess.evm_version = args.evm_version;
        sess.language = args.language;

        let compiler = Compiler { sess, args };

        SessionGlobals::with_source_map(compiler.sess.clone_source_map(), move || {
            let r = {
                let _finish_diagnostics = defer(|| compiler.finish_diagnostics());
                f(&compiler)
            };
            drop(compiler);
            r
        })
    })
}
