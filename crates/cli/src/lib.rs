#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use clap::Parser as _;
use solar_config::CompilerOutput;
use solar_interface::{Result, Session};
use solar_sema::{CompilerRef, ParsingContext};
use std::ops::ControlFlow;

pub use solar_config::{self as config, CompileOpts, UnstableOpts, version};

mod emit;
pub mod standard_json;

pub mod mir_opt;
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

mod args;
pub use args::{Args, Subcommands};

mod lsp;
pub use lsp::LspArgs;

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn parse_args<I, T>(itr: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args = Args::try_parse_from(itr)?;
    args.compile.finish()?;
    Ok(args)
}

pub fn run_compiler_args(opts: CompileOpts) -> Result {
    if opts.standard_json {
        standard_json::run(opts)
            .map_err(|_e| solar_interface::diagnostics::ErrorGuaranteed::new_unchecked())?;
        return Ok(());
    }

    run_compiler_with(opts, run_default)
}

fn run_default(compiler: &mut CompilerRef<'_>) -> Result {
    run_pipeline(
        compiler,
        |pcx| {
            // Partition arguments into three categories:
            // - `stdin`: `-`, occurrences after the first are ignored
            // - remappings: `[context:]prefix=path`, already parsed as part of `CompileOpts`
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
        },
        |_| {},
    )
    .map(|_| ())
}

pub(crate) fn run_pipeline(
    compiler: &mut CompilerRef<'_>,
    load_sources: impl FnOnce(&mut ParsingContext<'_>) -> Result,
    after_parsing: impl FnOnce(&CompilerRef<'_>),
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

    compiler.sources_mut().topo_sort();
    after_parsing(compiler);

    let ControlFlow::Continue(()) = compiler.lower_asts()? else {
        return Ok(ControlFlow::Break(()));
    };
    compiler.drop_asts();
    let ControlFlow::Continue(()) = compiler.analysis()? else {
        return Ok(ControlFlow::Break(()));
    };

    // Code generation (MIR and bytecode) is experimental and not part of the
    // stable, solc-compatible pipeline yet, so it is gated behind `-Zcodegen`.
    let needs_codegen = sess.opts.emit.iter().any(|e| {
        matches!(e, CompilerOutput::Mir | CompilerOutput::Bin | CompilerOutput::BinRuntime)
    });
    if needs_codegen && !sess.opts.unstable.codegen {
        return Err(sess
            .dcx
            .err("code generation is experimental")
            .help("pass `-Zcodegen` to emit MIR or bytecode")
            .emit());
    }

    emit::emit_requested(compiler)?;

    Ok(ControlFlow::Continue(()))
}

fn run_compiler_with(
    opts: CompileOpts,
    f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send,
) -> Result {
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
