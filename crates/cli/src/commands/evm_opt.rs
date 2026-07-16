//! The `solar evm-opt` subcommand — run EVM backend IR passes and print the
//! resulting EVM IR.
//!
//! This is the backend-IR equivalent of `solar mir-opt`. It currently accepts
//! EVM IR files (`.evmir`) and prints the canonical parser/printer output.

use clap::ValueHint;
use solar_codegen::backend::evm::ir;
use solar_config::CompileOpts;
use solar_interface::Session;
use std::{path::Path, process::ExitCode};

#[derive(clap::Args)]
#[command(after_help = after_help(), arg_required_else_help = true)]
pub(crate) struct EvmOptArgs {
    /// Comma-separated list of passes to run in order.
    #[arg(
        long = "passes",
        visible_alias = "pass",
        value_name = "NAMES",
        value_delimiter = ',',
        value_parser = parse_pass,
        default_value = "none"
    )]
    passes: Vec<ir::Pass>,
    /// If true, print EVM IR after every pass; otherwise only after the last.
    #[arg(long)]
    print_after_each: bool,
    /// Path to input file. Extension determines whether it's .evmir.
    #[arg(value_hint = ValueHint::FilePath)]
    input: String,
}

fn parse_pass(name: &str) -> Result<ir::Pass, String> {
    ir::Pass::by_name(name).ok_or_else(|| format!("unknown EVM IR pass: {name}"))
}

fn after_help() -> String {
    format!(
        "Passes:\n  {}\n\nInput formats:\n  *.evmir  EVM IR",
        ir::PASSES.iter().map(|pass| pass.name()).collect::<Vec<_>>().join("\n  ")
    )
}

fn selected_pass_list_label(passes: &[ir::Pass], separator: &str) -> String {
    passes.iter().map(|pass| pass.name()).collect::<Vec<_>>().join(separator)
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &ir::Module, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

fn run_pipeline(sess: &Session, module: &mut ir::Module, name: &str, args: &EvmOptArgs) {
    let dcx = &sess.dcx;
    let options = ir::PassOptions { time_passes: sess.opts.unstable.time_passes };
    let pipeline_label = selected_pass_list_label(&args.passes, ",");
    for (index, &pass) in args.passes.iter().enumerate() {
        pass.run(module, options);
        if args.print_after_each || index + 1 == args.passes.len() {
            ir::Verifier::new(dcx).verify_module(module);
            if dcx.has_errors().is_err() {
                break;
            }
            let label = if args.print_after_each { pass.name() } else { &pipeline_label };
            print_module(module, name, label);
        }
    }
}

fn process_evmir(sess: &Session, args: &EvmOptArgs) -> solar_interface::Result {
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| sess.dcx.err(format!("failed to read {}: {e}", args.input)).emit())?;
    let mut module = ir::Module::parse(source.src.as_str())
        .map_err(|err| sess.dcx.err(format!("{err}")).emit())?;
    ir::Verifier::new(&sess.dcx).verify_module(&module);
    if sess.dcx.has_errors().is_ok() {
        run_pipeline(sess, &mut module, &args.input, args);
    }
    Ok(())
}

pub(crate) fn run(args: EvmOptArgs, mut opts: CompileOpts) -> ExitCode {
    opts.input.push(args.input.clone());
    let ext = Path::new(&args.input).extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = super::compile::run_session_with(opts, |sess| match ext {
        "evmir" => process_evmir(sess, &args),
        _ => Err(sess
            .dcx
            .err(format!("unsupported input file extension `.{ext}` (expected .evmir)"))
            .emit()),
    });

    if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
