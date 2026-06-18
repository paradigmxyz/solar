//! The `solar evm-opt` subcommand — run EVM backend IR passes and print the
//! resulting EVM IR.
//!
//! This is the backend-IR equivalent of `solar mir-opt`. It currently accepts
//! textual EVM IR files (`.evmir`) and prints the canonical parser/printer
//! output. Pass plumbing is intentionally small until backend IR passes grow
//! beyond parser/layout smoke tests.

use clap::ValueHint;
use solar_codegen::backend::evm::{EvmIrModule, parse_evm_ir_module};
use solar_interface::{Ident, Session, Symbol};
use std::{path::Path, process::ExitCode};

#[derive(clap::Args)]
#[command(
    after_help = "\
Passes:
  none                 No transform; just parse and print

Input formats:
  *.evmir  Textual EVM IR",
    arg_required_else_help = true
)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvmIrPass {
    None,
}

impl EvmIrPass {
    const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }

    const fn run(self, _module: &mut EvmIrModule) -> bool {
        match self {
            Self::None => false,
        }
    }
}

fn parse_pass(name: &str) -> Result<EvmIrPass, String> {
    match name {
        "none" => Ok(EvmIrPass::None),
        other => Err(format!("unknown EVM IR pass: {other}")),
    }
}

fn selected_pass_list_label(passes: &[EvmIrPass], separator: &str) -> String {
    passes.iter().map(|pass| pass.name()).collect::<Vec<_>>().join(separator)
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &EvmIrModule, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    print!("{}", module.to_text());
}

fn run_pipeline(module: &mut EvmIrModule, name: &str, args: &EvmOptArgs) {
    if args.print_after_each {
        for &pass in &args.passes {
            pass.run(module);
            print_module(module, name, pass.name());
        }
    } else {
        for &pass in &args.passes {
            pass.run(module);
        }
        let label = selected_pass_list_label(&args.passes, ",");
        print_module(module, name, &label);
    }
}

fn process_evmir(args: &EvmOptArgs) -> Result<(), String> {
    let sess = Session::builder().with_stderr_emitter().build();
    let source = sess
        .source_map()
        .load_file(Path::new(&args.input))
        .map_err(|e| format!("failed to read {}: {e}", args.input))?;
    let text = source.src.as_str();
    let mut result: Result<(), String> = Ok(());
    sess.enter(|| {
        let mut module = match parse_evm_ir_module(text) {
            Ok(m) => m,
            Err(e) => {
                result = Err(format!("{e}"));
                return;
            }
        };
        let name = Ident::with_dummy_span(Symbol::intern(&args.input)).to_string();
        run_pipeline(&mut module, &name, args);
    });
    result
}

pub(crate) fn run(args: EvmOptArgs) -> ExitCode {
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
