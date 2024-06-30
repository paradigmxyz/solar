//! The main entry point for the Sulk compiler.

#![cfg_attr(feature = "nightly", feature(panic_update_hook))]

use clap::Parser as _;
use cli::Args;
use std::{num::NonZeroUsize, path::Path, process::ExitCode};
use sulk_data_structures::sync::Lrc;
use sulk_interface::{
    diagnostics::{DiagCtxt, DynEmitter, HumanEmitter, JsonEmitter},
    Result, Session, SessionGlobals, SourceMap,
};

pub mod cli;
mod utils;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod sigsegv_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod sigsegv_handler {
    /// No-op function.
    pub fn install() {}
}

// Used in integration tests. See `../tests.rs`.
#[cfg(test)]
use sulk_tester as _;

// We use jemalloc for performance reasons.
#[cfg(not(debug_assertions))]
#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(debug_assertions)]
use tikv_jemallocator as _;

fn main() -> ExitCode {
    sigsegv_handler::install();
    let _ = utils::init_logger();
    utils::install_panic_hook();
    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    match run_compiler_args(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
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

        // Partition arguments into three categories:
        // - `stdin`: `-`, occurences after the first are ignored
        // - remappings: `path=mapped`
        // - paths: everything else
        let stdin = args.input.iter().any(|arg| *arg == Path::new("-"));
        let non_stdin_args = args.input.iter().filter(|arg| *arg != Path::new("-"));
        let arg_remappings = non_stdin_args
            .clone()
            .filter_map(|arg| arg.to_str().unwrap_or("").parse::<cli::ImportMap>().ok());
        let paths = non_stdin_args.filter(|arg| !arg.to_str().unwrap_or("").contains('='));

        let mut resolver = sulk_sema::Resolver::new(sess);
        let remappings = arg_remappings.chain(args.import_map.iter().cloned());
        for map in remappings {
            resolver.file_resolver.add_import_map(map.map, map.path);
        }
        for path in &args.import_path {
            let new = resolver.file_resolver.add_import_path(path.clone());
            if !new {
                let msg = format!("import path {} already specified", path.display());
                return Err(sess.dcx.err(msg).emit());
            }
        }
        resolver.add_files_from_args(stdin, paths)?;

        resolver.parse_and_resolve()?;

        sess.dcx.has_errors()?;

        Ok(())
    }

    fn finish_diagnostics(&self) -> Result {
        self.sess.dcx.print_error_count()
    }
}

fn run_compiler_with(args: Args, f: impl FnOnce(&Compiler) -> Result + Send) -> Result {
    utils::run_in_thread_pool_with_globals(args.threads, |jobs| {
        let ui_testing = args.unstable.ui_testing;
        let source_map = Lrc::new(SourceMap::empty());
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
        sess.stop_after = args.stop_after;
        sess.jobs = NonZeroUsize::new(jobs).unwrap();

        let compiler = Compiler { sess, args };

        SessionGlobals::with_source_map(compiler.sess.clone_source_map(), move || {
            let mut r = f(&compiler);
            r = compiler.finish_diagnostics().and(r);
            r
        })
    })
}
