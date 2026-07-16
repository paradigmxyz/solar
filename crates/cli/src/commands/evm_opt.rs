//! The `solar evm-opt` subcommand — run EVM backend IR passes and print the
//! resulting EVM IR.
//!
//! This is the backend-IR equivalent of `solar mir-opt`. It currently accepts
//! EVM IR files (`.evmir`) and prints the canonical parser/printer output.

use clap::ValueHint;
use solar_codegen::backend::evm::{
    EVM_IR_PASSES, EvmIrModule, EvmIrPass, EvmIrPassOptions, parse_evm_ir_module,
    verify_evm_ir_module,
};
use solar_interface::{Ident, Session, Symbol};
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
    passes: Vec<EvmIrPass>,
    /// If true, print EVM IR after every pass; otherwise only after the last.
    #[arg(long)]
    print_after_each: bool,
    /// Path to input file. Extension determines whether it's .evmir.
    #[arg(value_hint = ValueHint::FilePath)]
    input: String,
    #[arg(skip)]
    time_passes: bool,
}

fn parse_pass(name: &str) -> Result<EvmIrPass, String> {
    EvmIrPass::by_name(name).ok_or_else(|| format!("unknown EVM IR pass: {name}"))
}

fn after_help() -> String {
    format!(
        "Passes:\n  {}\n\nInput formats:\n  *.evmir  EVM IR",
        EVM_IR_PASSES.iter().map(|pass| pass.name()).collect::<Vec<_>>().join("\n  ")
    )
}

fn selected_pass_list_label(passes: &[EvmIrPass], separator: &str) -> String {
    passes.iter().map(|pass| pass.name()).collect::<Vec<_>>().join(separator)
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &EvmIrModule, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

fn run_pipeline(module: &mut EvmIrModule, name: &str, args: &EvmOptArgs) -> Result<(), String> {
    let options = EvmIrPassOptions { time_passes: args.time_passes };
    if args.print_after_each {
        for &pass in &args.passes {
            pass.run(module, options);
            verify_evm_ir_module(module)
                .map_err(|e| format!("EVM IR pass `{}` produced invalid IR: {e}", pass.name()))?;
            print_module(module, name, pass.name());
        }
    } else {
        for &pass in &args.passes {
            pass.run(module, options);
        }
        verify_evm_ir_module(module)
            .map_err(|e| format!("EVM IR pipeline produced invalid IR: {e}"))?;
        let label = selected_pass_list_label(&args.passes, ",");
        print_module(module, name, &label);
    }
    Ok(())
}

fn process_evmir(args: &EvmOptArgs) -> Result<(), String> {
    let sess = Session::builder().with_stderr_emitter().build();
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| format!("failed to read {}: {e}", args.input))?;
    let text = source.src.as_str();
    sess.enter(|| {
        let input_name = Ident::with_dummy_span(Symbol::intern(&args.input)).to_string();
        let mut module = parse_evm_ir_module(text).map_err(|err| format!("{err}"))?;
        verify_evm_ir_module(&module).map_err(|err| format!("{err}"))?;
        run_pipeline(&mut module, &input_name, args)
    })
}

pub(crate) fn run(mut args: EvmOptArgs, time_passes: bool) -> ExitCode {
    args.time_passes = time_passes;
    let ext = Path::new(&args.input).extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = match ext {
        "evmir" => process_evmir(&args),
        _ => Err(format!("unsupported input file extension `.{ext}` (expected .evmir)")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
