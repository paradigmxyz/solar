//! The main entry point for the Sulk compiler.

#![cfg_attr(feature = "nightly", feature(panic_update_hook))]

use clap::Parser as _;
use cli::Args;
use std::{path::Path, process::ExitCode};
use sulk_data_structures::{defer, sync::Lrc};
use sulk_interface::{
    diagnostics::{DiagCtxt, DynEmitter, FatalError, HumanEmitter, JsonEmitter},
    Result, Session, SessionGlobals, SourceMap,
};

pub mod cli;
mod utils;

// Used in integration tests. See `../tests.rs`.
#[cfg(test)]
use sulk_tester as _;

fn main() -> ExitCode {
    let early_dcx = DiagCtxt::with_tty_emitter(None);

    utils::init_logger(&early_dcx);
    utils::install_panic_hook();

    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };

    FatalError::catch_with_exit_code(|| run_compiler_args(args))
}

pub fn parse_args<I, T>(itr: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args = Args::try_parse_from(itr)?;
    args.populate_unstable()?;
    Ok(args)
}

pub fn run_compiler_args(args: Args) -> Result<()> {
    run_compiler_with(args, Compiler::run_default)
}

pub struct Compiler {
    pub sess: Session,
    pub args: Args,
}

impl Compiler {
    pub fn run_default(&self) -> Result<()> {
        let Self { sess, args } = self;

        if sess.language.is_yul() && !args.unstable.parse_yul {
            return Err(sess.dcx.err("Yul is not supported yet").emit());
        }

        let mut resolver = sulk_sema::Resolver::new(sess);
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

    fn finish_diagnostics(&self) {
        self.sess.dcx.print_error_count();
    }
}

fn run_compiler_with<R: Send>(args: Args, f: impl FnOnce(&Compiler) -> R + Send) -> R {
    utils::run_in_thread_with_globals(|| {
        let ui_testing = args.unstable.ui_testing;
        let source_map = Lrc::new(SourceMap::new());
        let emitter: Box<DynEmitter> = match args.error_format {
            cli::ErrorFormat::Human => {
                let color = match args.color {
                    clap::ColorChoice::Always => sulk_interface::ColorChoice::Always,
                    clap::ColorChoice::Auto => sulk_interface::ColorChoice::Auto,
                    clap::ColorChoice::Never => sulk_interface::ColorChoice::Never,
                };
                let human = HumanEmitter::stderr(color)
                    .source_map(Some(source_map.clone()))
                    .ui_testing(ui_testing);
                Box::new(human)
            }
            cli::ErrorFormat::Json | cli::ErrorFormat::RichJson => {
                let json = JsonEmitter::new(Box::new(std::io::stderr()), source_map.clone())
                    .pretty(args.pretty_json)
                    .rustc_like(matches!(args.error_format, cli::ErrorFormat::RichJson))
                    .ui_testing(ui_testing);
                Box::new(json)
            }
        };
        let dcx = DiagCtxt::new(emitter).set_flags(|flags| {
            flags.deduplicate_diagnostics &= !ui_testing;
            flags.track_diagnostics &= !ui_testing;
            flags.track_diagnostics |= args.unstable.track_diagnostics;
        });

        let mut sess = Session::new(dcx, source_map);
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
