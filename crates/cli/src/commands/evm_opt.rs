//! The `solar evm-opt` subcommand — run EVM backend IR passes and print the
//! resulting EVM IR.
//!
//! This is the backend-IR equivalent of `solar mir-opt`. It currently accepts
//! EVM IR files (`.evmir`) and prints the canonical parser/printer output.

use clap::ValueHint;
use solar_codegen::backend::evm::ir;
use solar_config::CompileOpts;
use solar_sema::CompilerRef;
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
    passes: Vec<Option<&'static ir::PassInfo>>,
    /// If true, print EVM IR after every pass; otherwise only after the last.
    #[arg(long)]
    print_after_each: bool,
    /// Path to input file. Extension determines whether it's .evmir.
    #[arg(value_hint = ValueHint::FilePath)]
    input: String,
}

fn parse_pass(name: &str) -> Result<Option<&'static ir::PassInfo>, String> {
    match name {
        "none" => Ok(None),
        other => {
            ir::lookup_pass(other).map(Some).ok_or_else(|| format!("unknown EVM IR pass: {other}"))
        }
    }
}

fn after_help() -> String {
    format!(
        "Passes:\n  {}\n  {:<20} No transform; validate and print the module\n\nInput formats:\n  *.evmir  EVM IR",
        ir::PASS_REGISTRY
            .iter()
            .map(|pass| format!("{:<20} {}", pass.name, pass.description))
            .collect::<Vec<_>>()
            .join("\n  "),
        "none",
    )
}

fn pass_label(pass: Option<&ir::PassInfo>) -> &'static str {
    match pass {
        Some(pass) => pass.name,
        None => "none",
    }
}

fn selected_pass_list_label(passes: &[Option<&ir::PassInfo>], separator: &str) -> String {
    passes.iter().copied().map(pass_label).collect::<Vec<_>>().join(separator)
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &ir::Module, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

fn run_pipeline(
    compiler: &CompilerRef<'_>,
    module: &mut ir::Module,
    name: &str,
    args: &EvmOptArgs,
) {
    let sess = compiler.sess();
    let dcx = &sess.dcx;
    let pipeline_label = selected_pass_list_label(&args.passes, ",");
    for (index, &pass) in args.passes.iter().enumerate() {
        if let Some(pass) = pass {
            ir::run_pass(compiler.gcx(), module, pass);
        }
        if args.print_after_each || index + 1 == args.passes.len() {
            ir::validate(dcx, module);
            if dcx.has_errors().is_err() {
                break;
            }
            let label = if args.print_after_each { pass_label(pass) } else { &pipeline_label };
            print_module(module, name, label);
        }
    }
}

fn process_evmir(compiler: &mut CompilerRef<'_>, args: &EvmOptArgs) -> solar_interface::Result {
    let sess = compiler.sess();
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| sess.dcx.err(format!("failed to read {}: {e}", args.input)).emit())?;
    let mut module = ir::Module::parse(sess, &source)?;
    ir::validate(&sess.dcx, &module);
    if sess.dcx.has_errors().is_ok() {
        run_pipeline(compiler, &mut module, &args.input, args);
    }
    Ok(())
}

pub(crate) fn run(args: EvmOptArgs, mut opts: CompileOpts) -> ExitCode {
    opts.input.push(args.input.clone());
    let ext = Path::new(&args.input).extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = super::compile::run_compiler_with(opts, |compiler| match ext {
        "evmir" => process_evmir(compiler, &args),
        _ => Err(compiler
            .sess()
            .dcx
            .err(format!("unsupported input file extension `.{ext}` (expected .evmir)"))
            .emit()),
    });

    if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
