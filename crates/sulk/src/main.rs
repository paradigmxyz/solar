//! The main entry point for the Sulk compiler.

use clap::Parser as _;
use cli::Args;
use std::process::ExitCode;
use sulk_data_structures::{defer, sync::Lrc};
use sulk_interface::{
    diagnostics::{DiagCtxt, FatalError},
    Result, SessionGlobals, SourceMap,
};
use sulk_parse::{ParseSess, Parser};

pub mod cli;
mod utils;

// Used in integration tests.
#[cfg(test)]
use sulk_tester as _;

fn main() -> ExitCode {
    let early_dcx = DiagCtxt::with_tty_emitter(None);

    utils::init_logger(&early_dcx);
    utils::install_panic_hook();

    // TODO: Register logger
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
    run_compiler_with(args, |compiler| {
        let sess = &compiler.sess.parse_sess;
        let args = &compiler.sess.args;

        let file = sess.source_map().load_file(&args.input).map_err(|e| {
            let msg = format!("couldn't read {}: {}", args.input.display(), e);
            sess.dcx.err(msg).emit()
        })?;

        let mut parser = Parser::from_source_file(sess, &file);
        let file = parser.parse_file().map_err(|e| e.emit())?;
        let _ = file;
        // eprintln!("file: {file:#?}");

        Ok(())
    })
}

pub struct Session {
    pub parse_sess: ParseSess,
    pub args: Args,
}

impl Session {
    #[inline]
    pub fn dcx(&self) -> &DiagCtxt {
        &self.parse_sess.dcx
    }

    #[inline]
    pub fn source_map(&self) -> &SourceMap {
        self.parse_sess.source_map()
    }
}

pub struct Compiler {
    pub sess: Session,
}

impl Compiler {
    fn finish_diagnostics(&self) {
        self.sess.dcx().print_error_count();
    }
}

fn run_compiler_with<R: Send>(args: Args, f: impl FnOnce(&Compiler) -> R + Send) -> R {
    utils::run_in_thread_with_globals(|| {
        let color = match args.color {
            clap::ColorChoice::Always => sulk_interface::ColorChoice::Always,
            clap::ColorChoice::Auto => sulk_interface::ColorChoice::Auto,
            clap::ColorChoice::Never => sulk_interface::ColorChoice::Never,
        };
        let parse_sess = ParseSess::with_tty_emitter_and_color(Lrc::new(SourceMap::empty()), color);

        let sess = Session { parse_sess, args };
        let compiler = Compiler { sess };

        SessionGlobals::with_source_map(compiler.sess.parse_sess.clone_source_map(), move || {
            let r = {
                let _finish_diagnostics = defer(|| compiler.finish_diagnostics());
                f(&compiler)
            };
            drop(compiler);
            r
        })
    })
}
