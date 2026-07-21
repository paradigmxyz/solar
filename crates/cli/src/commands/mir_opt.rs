//! The `solar mir-opt` subcommand — run one or more MIR transformation passes
//! and print the resulting MIR.
//!
//! This is the Solar equivalent of LLVM's `opt`. It accepts either a Solidity
//! file (`.sol`) — which is parsed, lowered to MIR, and then transformed — or a
//! textual MIR file (`.mir`) — which is parsed directly. After running the
//! requested pass pipeline, it prints the resulting MIR. With `-Zpass-diff`,
//! it instead prints a line-oriented before-and-after diff for each pass.
//!
//! It is an unstable, internal tool used by the `Mir` test mode; it is not part
//! of the stable CLI surface.

use super::print_pass_diff;
use clap::ValueHint;
use solar_codegen::{
    lower,
    mir::{Module, validate},
    pass::{
        DEFAULT_CLEANUP_PIPELINE, DEFAULT_PIPELINE, PASS_REGISTRY, PassInfo, PipelineOptions,
        lookup_pass, run_default_pipeline, run_pass,
    },
};
use solar_config::CompileOpts;
use solar_data_structures::fmt::{self, FmtIteratorExt};
use solar_interface::Session;
use solar_sema::CompilerRef;
use std::{ops::ControlFlow, path::Path, process::ExitCode};

fn after_help() -> String {
    fn display_pass_help(pass: &PassInfo) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| write!(f, "  {:<20} {}", pass.name, pass.description))
    }

    fn display_pass_list<'a>(passes: &'a [PassInfo], separator: &'a str) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            write!(f, "{}", passes.iter().map(|pass| pass.name).format(separator))
        })
    }

    fmt::from_fn(|f| {
        write!(
            f,
            "\
Passes:
{}
  {:<20} No transform; just lower/parse and print

Default pipeline:
  {}

Default cleanup fixpoint:
  {}

Input formats:
  *.sol  Solidity contract — lowered through the normal compiler pipeline
  *.mir  Textual MIR — parsed directly via solar_codegen::mir::Module::parse",
            PASS_REGISTRY.iter().map(display_pass_help).format("\n"),
            "none",
            display_pass_list(DEFAULT_PIPELINE, " → "),
            display_pass_list(DEFAULT_CLEANUP_PIPELINE, " → ")
        )
    })
    .to_string()
}

#[derive(clap::Args)]
#[command(
    after_help = after_help(),
    arg_required_else_help = true
)]
pub(crate) struct MirOptArgs {
    /// Comma-separated list of passes to run in order.
    #[arg(
        long = "passes",
        visible_alias = "pass",
        value_name = "NAMES",
        value_delimiter = ',',
        value_parser = parse_pass,
        required_unless_present = "pipeline_default",
        conflicts_with = "pipeline_default"
    )]
    passes: Option<Vec<Option<&'static PassInfo>>>,
    /// If true, print MIR after every pass; otherwise only after the last.
    #[arg(long)]
    print_after_each: bool,
    /// Run the same pass pipeline as EvmCodegen::run_optimization_passes.
    #[arg(long, conflicts_with = "passes")]
    pipeline_default: bool,
    /// Path to input file. Extension determines whether it's .sol or .mir.
    #[arg(value_hint = ValueHint::FilePath)]
    input: String,
}

impl MirOptArgs {
    fn selected_passes(&self) -> Vec<Option<&'static PassInfo>> {
        self.passes.clone().expect("clap requires passes unless pipeline-default is set")
    }

    fn pipeline_label(&self, passes: &[Option<&PassInfo>]) -> String {
        if self.pipeline_default {
            "pipeline-default".to_string()
        } else {
            selected_pass_list_label(passes, ",")
        }
    }
}

fn parse_pass(name: &str) -> Result<Option<&'static PassInfo>, String> {
    match name {
        "none" => Ok(None),
        other => lookup_pass(other).map(Some).ok_or_else(|| format!("unknown pass: {other}")),
    }
}

fn pass_label(pass: Option<&PassInfo>) -> &'static str {
    match pass {
        Some(pass) => pass.name,
        None => "none",
    }
}

fn selected_pass_list_label(passes: &[Option<&PassInfo>], separator: &str) -> String {
    passes.iter().copied().map(pass_label).format(separator).to_string()
}

/// Runs the pass pipeline on a single module and emits output.
/// Used for both .sol contracts and .mir input.
fn run_pipeline(
    module: &mut Module,
    name: &str,
    args: &MirOptArgs,
    pass_diff: bool,
    time_passes: bool,
) {
    if args.pipeline_default {
        let mut options = PipelineOptions::default();
        options.print_after_each = args.print_after_each;
        options.time_passes = time_passes;
        run_default_pipeline(module, options);
        if !args.print_after_each {
            print_module(module, name, "pipeline-default");
        }
        return;
    }

    let passes = args.selected_passes();
    let mut options = PipelineOptions::default();
    options.time_passes = time_passes;
    let pipeline_label = args.pipeline_label(&passes);
    for (index, &pass) in passes.iter().enumerate() {
        let before = pass_diff.then(|| module.to_text().to_string());
        if let Some(pass) = pass {
            run_pass(module, pass, options);
        }
        if let Some(before) = before {
            let after = module.to_text().to_string();
            print_pass_diff(name, pass_label(pass), &before, &after);
        } else if args.print_after_each || index + 1 == passes.len() {
            let label = if args.print_after_each { pass_label(pass) } else { &pipeline_label };
            print_module(module, name, label);
        }
    }
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &Module, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

/// Process a `.mir` input: read file, parse, run passes, print.
fn process_mir(sess: &Session, args: &MirOptArgs) -> solar_interface::Result {
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| sess.dcx.err(format!("failed to read {}: {e}", args.input)).emit())?;
    let mut module = Module::parse(sess, &source)?;
    // Hand-written MIR is untrusted input: reject invalid modules with a
    // diagnostic instead of tripping the post-pass validator ICE.
    validate(&sess.dcx, &module);
    if sess.dcx.has_errors().is_ok() {
        run_pipeline(
            &mut module,
            &args.input,
            args,
            sess.opts.unstable.pass_diff,
            sess.opts.unstable.time_passes,
        );
    }
    Ok(())
}

/// Process a `.sol` input: full Solidity → MIR pipeline.
fn process_sol(compiler: &mut CompilerRef<'_>, args: &MirOptArgs) -> solar_interface::Result {
    {
        let mut pcx = compiler.parse();
        pcx.load_files([Path::new(&args.input)])?;
        pcx.parse();
    }

    let ControlFlow::Continue(()) = compiler.lower_asts()? else { return Ok(()) };
    let ControlFlow::Continue(()) = compiler.analysis()? else { return Ok(()) };

    let gcx = compiler.gcx();
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
            continue;
        }
        let mut module = lower::lower_contract(gcx, id);
        let name = gcx.contract_fully_qualified_name(id).to_string();
        run_pipeline(
            &mut module,
            &name,
            args,
            gcx.sess.opts.unstable.pass_diff,
            gcx.sess.opts.unstable.time_passes,
        );
    }
    Ok(())
}

/// Entry point for the `mir-opt` subcommand.
pub(super) fn run(args: MirOptArgs, mut opts: CompileOpts) -> ExitCode {
    opts.input.push(args.input.clone());
    // Dispatch on input file extension.
    let ext = Path::new(&args.input).extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = match ext {
        "sol" => super::compile::run_compiler_with(opts, |compiler| process_sol(compiler, &args)),
        "mir" => super::compile::run_session_with(opts, |sess| process_mir(sess, &args)),
        _ => super::compile::run_session_with(opts, |sess| {
            Err(sess
                .dcx
                .err(format!("unsupported input file extension `.{ext}` (expected .sol or .mir)"))
                .emit())
        }),
    };

    if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
