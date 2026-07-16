//! The `solar mir-opt` subcommand — run one or more MIR transformation passes
//! and print the resulting MIR.
//!
//! This is the Solar equivalent of LLVM's `opt`. It accepts either a Solidity
//! file (`.sol`) — which is parsed, lowered to MIR, and then transformed — or a
//! textual MIR file (`.mir`) — which is parsed directly. After running the
//! requested pass pipeline, it prints the resulting MIR.
//!
//! It is an unstable, internal tool used by the `Mir` test mode; it is not part
//! of the stable CLI surface.

use clap::ValueHint;
use solar_codegen::{
    lower,
    mir::{Module, parse_module},
    pass::{
        DEFAULT_CLEANUP_PIPELINE, DEFAULT_PIPELINE, PASS_REGISTRY, PassInfo, PipelineOptions,
        lookup_pass, run_default_pipeline, run_pass,
    },
};
use solar_data_structures::fmt::{self, FmtIteratorExt};
use solar_interface::{Ident, Session, Symbol};
use solar_sema::Compiler;
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
  *.mir  Textual MIR — parsed directly via solar_codegen::mir::parse_module",
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
    #[arg(skip)]
    time_passes: bool,
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

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &Module, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

/// Runs the pass pipeline on a single module and emits output.
/// Used for both .sol contracts and .mir input.
fn run_pipeline(module: &mut Module, name: &str, args: &MirOptArgs) -> Result<(), String> {
    if args.pipeline_default {
        run_default_pipeline(
            module,
            PipelineOptions {
                print_after_each: args.print_after_each,
                time_passes: args.time_passes,
                ..PipelineOptions::default()
            },
        );
        if !args.print_after_each {
            print_module(module, name, "pipeline-default");
        }
        return Ok(());
    }

    let passes = args.selected_passes();
    let options = PipelineOptions { time_passes: args.time_passes, ..PipelineOptions::default() };
    if args.print_after_each {
        for pass in &passes {
            if let Some(pass) = *pass {
                run_pass(module, pass, options);
            }
            print_module(module, name, pass_label(*pass));
        }
    } else {
        for &pass in &passes {
            if let Some(pass) = pass {
                run_pass(module, pass, options);
            }
        }
        let label = args.pipeline_label(&passes);
        print_module(module, name, &label);
    }
    Ok(())
}

/// Process a `.mir` input: read file, parse, run passes, print.
fn process_mir(args: &MirOptArgs) -> Result<(), String> {
    let sess = Session::builder().with_stderr_emitter().build();
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| format!("failed to read {}: {e}", args.input))?;
    let text = source.src.as_str();
    let mut result: Result<(), String> = Ok(());
    sess.enter(|| {
        let mut module = match parse_module(text) {
            Ok(m) => m,
            Err(e) => {
                result = Err(format!("{e}"));
                return;
            }
        };
        // Hand-written MIR is untrusted input: reject invalid modules with a
        // diagnostic instead of tripping the post-pass validator ICE.
        let errors = solar_codegen::analysis::validate_module(&module);
        if !errors.is_empty() {
            result = Err(format!(
                "invalid MIR input:\n{}",
                errors.iter().map(|e| format!("  {e}")).collect::<Vec<_>>().join("\n")
            ));
            return;
        }
        // Use a fixed name for .mir input — the parser interns whatever the
        // file declared (or "module" by default).
        let name = Ident::with_dummy_span(Symbol::intern(&args.input)).to_string();
        if let Err(e) = run_pipeline(&mut module, &name, args) {
            result = Err(e);
        }
    });
    result
}

/// Process a `.sol` input: full Solidity → MIR pipeline.
fn process_sol(args: &MirOptArgs) -> Result<(), String> {
    let sess = Session::builder().with_stderr_emitter().build();
    let mut compiler = Compiler::new(sess);

    let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
        let mut pcx = c.parse();
        pcx.load_files([Path::new(&args.input)])?;
        pcx.parse();
        Ok(())
    });
    if parse_result.is_err() {
        return Err("parse error".into());
    }

    let mut pipeline_err: Option<String> = None;
    let result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
        let ControlFlow::Continue(()) = c.lower_asts()? else { return Ok(()) };
        let ControlFlow::Continue(()) = c.analysis()? else { return Ok(()) };

        let gcx = c.gcx();
        for id in gcx.hir.contract_ids() {
            let contract = gcx.hir.contract(id);
            if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
                continue;
            }
            let mut module = lower::lower_contract(gcx, id);
            let name = gcx.contract_fully_qualified_name(id).to_string();
            if let Err(e) = run_pipeline(&mut module, &name, args) {
                pipeline_err = Some(e);
                break;
            }
        }
        Ok(())
    });

    if let Some(e) = pipeline_err {
        return Err(e);
    }
    if result.is_err() || compiler.sess().emitted_errors().is_some_and(|r| r.is_err()) {
        return Err("compilation failed".into());
    }
    Ok(())
}

/// Entry point for the `mir-opt` subcommand.
pub(super) fn run(mut args: MirOptArgs, time_passes: bool) -> ExitCode {
    args.time_passes = time_passes;
    // Dispatch on input file extension.
    let ext = Path::new(&args.input).extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = match ext {
        "sol" => process_sol(&args),
        "mir" => process_mir(&args),
        _ => Err(format!("unsupported input file extension `.{ext}` (expected .sol or .mir)")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
