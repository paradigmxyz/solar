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
    let mut args = Args::parse_from(args);
    args.populate_unstable().map_err(|e| e.exit())?;
    run_compiler_with(args, _run_compiler)
}

fn _run_compiler(Compiler { sess, args }: &Compiler) -> Result<()> {
    let is_yul = args.language.is_yul();
    if is_yul && !args.unstable.parse_yul {
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
            cli::ErrorFormat::Json | cli::ErrorFormat::PrettyJson => {
                let json = JsonEmitter::new(Box::new(std::io::stderr()), source_map.clone())
                    .pretty(matches!(args.error_format, cli::ErrorFormat::PrettyJson))
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
