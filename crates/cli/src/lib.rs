#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use clap::Parser as _;
use solar_config::ErrorFormat;
use solar_interface::{
    Result, Session, SourceMap,
    diagnostics::{DiagCtxt, DynEmitter, HumanEmitter, JsonEmitter},
};
use solar_sema::CompilerRef;
use std::{ops::ControlFlow, process::ExitCode, sync::Arc};

pub use solar_config::{self as config, Opts, UnstableOpts, version};

pub mod utils;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod sigsegv_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod sigsegv_handler {
    #[cfg(unix)]
    use libc as _;

    /// No-op function.
    pub fn install() {}
}

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn parse_args<I, T>(itr: I) -> Result<Opts, ExitCode>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match parse_args_clap(itr) {
        Ok(opts) => Ok(opts),
        Err(e) => {
            // For errors, emit the first line of the error message as an error diagnostic,
            // and the rest to `stderr`.
            if e.use_stderr() {
                fn split(s: &str) -> (&str, &str) {
                    if let Some(l) = s.find('\n') { s.split_at(l) } else { (&s[..], "") }
                }

                let rendered = e.render();
                let unstyled = rendered.to_string();
                let styled = rendered.ansi().to_string();

                utils::early_dcx().err(split(&unstyled).0.trim().replace("error: ", "")).emit();

                let mut stream =
                    anstream::AutoStream::new(std::io::stderr(), anstream::ColorChoice::Auto);
                let _ = std::io::Write::write_all(
                    &mut stream,
                    split(&styled).1.trim_start().as_bytes(),
                );

                return Err(ExitCode::FAILURE);
            }

            // For --help and similar, just print directly to `stdout`.
            let _ = e.print();
            Err(ExitCode::SUCCESS)
        }
    }
}

pub fn parse_args_clap<I, T>(itr: I) -> Result<Opts, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut opts = Opts::try_parse_from(itr)?;
    opts.finish()?;
    Ok(opts)
}

pub fn run_compiler_args(opts: Opts) -> Result {
    run_compiler_with(opts, run_default)
}

fn run_default(compiler: &mut CompilerRef<'_>) -> Result {
    let sess = compiler.gcx().sess;
    if sess.opts.language.is_yul() && !sess.opts.unstable.parse_yul {
        return Err(sess.dcx.err("Yul is not supported yet").emit());
    }

    let mut pcx = compiler.parse();

    // Partition arguments into three categories:
    // - `stdin`: `-`, occurrences after the first are ignored
    // - remappings: `[context:]prefix=path`, already parsed as part of `Opts`
    // - paths: everything else
    let mut seen_stdin = false;
    let mut paths = Vec::new();
    for arg in sess.opts.input.iter().map(String::as_str) {
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

    pcx.par_load_files(paths)?;

    pcx.parse();
    let ControlFlow::Continue(()) = compiler.lower_asts()? else { return Ok(()) };
    compiler.drop_asts();
    let ControlFlow::Continue(()) = compiler.analysis()? else { return Ok(()) };

    Ok(())
}

fn run_compiler_with(opts: Opts, f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send) -> Result {
    let ui_testing = opts.unstable.ui_testing;
    let source_map = Arc::new(SourceMap::empty());
    let emitter: Box<DynEmitter> = match opts.error_format {
        ErrorFormat::Human => {
            let color = match opts.color {
                clap::ColorChoice::Always => solar_interface::ColorChoice::Always,
                clap::ColorChoice::Auto => solar_interface::ColorChoice::Auto,
                clap::ColorChoice::Never => solar_interface::ColorChoice::Never,
            };
            let human = HumanEmitter::stderr(color)
                .source_map(Some(source_map.clone()))
                .ui_testing(ui_testing);
            Box::new(human)
        }
        ErrorFormat::Json | ErrorFormat::RustcJson => {
            // `io::Stderr` is not buffered.
            let writer = Box::new(std::io::BufWriter::new(std::io::stderr()));
            let json = JsonEmitter::new(writer, source_map.clone())
                .pretty(opts.pretty_json_err)
                .rustc_like(matches!(opts.error_format, ErrorFormat::RustcJson))
                .ui_testing(ui_testing);
            Box::new(json)
        }
        format => todo!("{format:?}"),
    };
    let dcx = DiagCtxt::new(emitter).set_flags(|flags| {
        flags.deduplicate_diagnostics &= !ui_testing;
        flags.track_diagnostics &= !ui_testing;
        flags.track_diagnostics |= opts.unstable.track_diagnostics;
        flags.can_emit_warnings |= !opts.no_warnings;
    });

    let mut sess = Session::builder().dcx(dcx).source_map(source_map).opts(opts).build();
    sess.infer_language();
    sess.validate()?;

    let mut compiler = solar_sema::Compiler::new(sess);
    compiler.enter_mut(|compiler| {
        let mut r = f(compiler);
        r = r.and(finish_diagnostics(compiler.gcx().sess));
        r
    })
}

fn finish_diagnostics(sess: &Session) -> Result {
    sess.dcx.print_error_count()
}
