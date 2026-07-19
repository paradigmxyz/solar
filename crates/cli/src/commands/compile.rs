use solar_config::CompileOpts;
use solar_interface::{Result, Session};
use solar_sema::{CompilerRef, ParsingContext};
use std::{ops::ControlFlow, process::ExitCode};

pub(super) fn run(opts: CompileOpts) -> ExitCode {
    match run_compiler_args(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

pub fn run_compiler_args(opts: CompileOpts) -> Result {
    if opts.standard_json {
        crate::standard_json::run(opts)
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

    // Code generation (MIR, EVM IR, and bytecode) is experimental and not part of the
    // stable, solc-compatible pipeline yet, so it is gated behind `-Zcodegen`.
    let needs_codegen = sess.opts.emit.iter().any(|e| e.is_codegen())
        || sess.opts.unstable.dump.as_ref().is_some_and(|dump| dump.kind.is_codegen());
    if needs_codegen && !sess.opts.unstable.codegen {
        return Err(sess
            .dcx
            .err("code generation is experimental")
            .help("pass `-Zcodegen` to emit or dump MIR, EVM IR, or bytecode")
            .emit());
    }

    crate::emit::emit_requested(compiler)?;

    Ok(ControlFlow::Continue(()))
}

pub(crate) fn run_compiler_with(
    opts: CompileOpts,
    f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send,
) -> Result {
    run_compiler_session_with(new_session(opts), f, true)
}

pub(crate) fn run_session_with(
    opts: CompileOpts,
    f: impl FnOnce(&Session) -> Result + Send,
) -> Result {
    let sess = new_session(opts);
    sess.validate()?;
    let result = sess.enter(|| f(&sess));
    finish_session(&sess, result)
}

fn new_session(opts: CompileOpts) -> Session {
    let mut sess = Session::new(opts);
    sess.infer_language();
    sess
}

pub(crate) fn run_compiler_session_with(
    sess: Session,
    f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send,
    finish: bool,
) -> Result {
    sess.validate()?;
    let mut compiler = solar_sema::Compiler::new(sess);
    compiler.enter_mut(|compiler| {
        let result = f(compiler);
        if !finish {
            return result;
        }
        finish_session(compiler.gcx().sess, result)
    })
}

fn finish_session(sess: &Session, result: Result) -> Result {
    let diagnostics = sess.dcx.print_error_count();
    result?;
    diagnostics
}
