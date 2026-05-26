#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use clap::Parser as _;
use solar_interface::{Result, Session};
use solar_sema::{CompilerRef, ParsingContext};
use std::{
    ffi::{OsStr, OsString},
    ops::ControlFlow,
};

pub use solar_config::{self as config, Opts, UnstableOpts, version};

mod standard_json;

pub mod utils;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod signal_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod signal_handler {
    #[cfg(unix)]
    use libc as _;

    /// No-op function.
    pub fn install() {}
}

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn try_print_version(itr: impl IntoIterator<Item = OsString>) -> bool {
    let mut itr = itr.into_iter();
    let _ = itr.next();
    for arg in itr {
        let arg = arg.as_os_str();
        if arg == OsStr::new("--") {
            break;
        }
        if arg == OsStr::new("--version") {
            println!("solar {}", version::long_version());
            return true;
        }
        if arg == OsStr::new("-V") {
            println!("solar {}", version::SHORT_VERSION);
            return true;
        }
    }
    false
}

pub fn parse_args<I, T>(itr: I) -> Result<Opts, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut opts = Opts::try_parse_from(itr)?;
    opts.finish()?;
    Ok(opts)
}

pub fn run_compiler_args(opts: Opts) -> Result {
    if opts.standard_json {
        standard_json::run(opts)
            .map_err(|_e| solar_interface::diagnostics::ErrorGuaranteed::new_unchecked())?;
        return Ok(());
    }

    run_compiler_with(opts, run_default)
}

fn run_default(compiler: &mut CompilerRef<'_>) -> Result {
    run_pipeline(compiler, |pcx| {
        // Partition arguments into three categories:
        // - `stdin`: `-`, occurrences after the first are ignored
        // - remappings: `[context:]prefix=path`, already parsed as part of `Opts`
        // - paths: everything else
        let mut seen_stdin = false;
        let mut paths = Vec::new();
        for arg in pcx.sess.opts.input.clone() {
            if arg == "-" {
                if !seen_stdin {
                    pcx.load_stdin()?;
                }
                seen_stdin = true;
                continue;
            }

            if arg.contains('=') {
                continue;
            }

            paths.push(arg);
        }

        pcx.par_load_files(paths)
    })
    .map(|_| ())
}

pub(crate) fn run_pipeline(
    compiler: &mut CompilerRef<'_>,
    load_sources: impl FnOnce(&mut ParsingContext<'_>) -> Result,
) -> Result<ControlFlow<()>> {
    let sess = compiler.gcx().sess;
    if sess.opts.language.is_yul() && !sess.opts.unstable.parse_yul {
        return Err(sess.dcx.err("Yul is not supported yet").emit());
    }

    let mut pcx = compiler.parse();
    load_sources(&mut pcx)?;
    pcx.parse();

    if compiler.gcx().sources.is_empty() {
        let msg = "no files found";
        let note = "if you wish to use the standard input, please specify `-` explicitly";
        return Err(sess.dcx.err(msg).note(note).emit());
    }

    let ControlFlow::Continue(()) = compiler.lower_asts()? else {
        return Ok(ControlFlow::Break(()));
    };
    compiler.drop_asts();
    let ControlFlow::Continue(()) = compiler.analysis()? else {
        return Ok(ControlFlow::Break(()));
    };

    Ok(ControlFlow::Continue(()))
}

fn run_compiler_with(opts: Opts, f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send) -> Result {
    let mut sess = Session::new(opts);
    sess.infer_language();
    run_compiler_session_with(sess, f, true)
}

pub(crate) fn run_compiler_session_with(
    sess: Session,
    f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send,
    finish: bool,
) -> Result {
    sess.validate()?;
    let mut compiler = solar_sema::Compiler::new(sess);
    compiler.enter_mut(|compiler| {
        let mut r = f(compiler);
        if finish {
            r = r.and(finish_diagnostics(compiler.gcx().sess));
        }
        r
    })
}

fn finish_diagnostics(sess: &Session) -> Result {
    sess.dcx.print_error_count()
}
