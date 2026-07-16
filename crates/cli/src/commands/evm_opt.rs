//! The `solar evm-opt` subcommand — run EVM backend IR passes and print the
//! resulting EVM IR.
//!
//! This is the backend-IR equivalent of `solar mir-opt`. It currently accepts
//! EVM IR files (`.evmir`) and prints the canonical parser/printer output.

use clap::ValueHint;
use solar_codegen::backend::evm::{
    EVM_IR_PASSES, EvmIrModule, EvmIrPass, parse_evm_ir_module, verify_evm_ir_module,
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
    if args.print_after_each {
        for &pass in &args.passes {
            pass.run(module);
            verify_evm_ir_module(module)
                .map_err(|e| format!("EVM IR pass `{}` produced invalid IR: {e}", pass.name()))?;
            print_module(module, name, pass.name());
        }
    } else {
        for &pass in &args.passes {
            pass.run(module);
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
        for (name, section) in evm_ir_sections(text, &input_name) {
            let mut module = parse_evm_ir_module(section).map_err(|err| format!("{err}"))?;
            verify_evm_ir_module(&module).map_err(|err| format!("{err}"))?;
            run_pipeline(&mut module, name, args)?;
        }
        Ok(())
    })
}

fn evm_ir_sections<'a>(input: &'a str, input_name: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut sections = Vec::new();
    let mut name = input_name;
    let mut offset = 0;
    let mut section_start = 0;
    for line in input.split_inclusive('\n') {
        let end = offset + line.len();
        if let Some(next_name) =
            line.trim().strip_prefix("// === ").and_then(|line| line.strip_suffix(" ==="))
        {
            let section = &input[section_start..offset];
            if section.lines().any(|line| line.trim_start().starts_with("bb")) {
                sections.push((name, section));
            }
            name = next_name;
            section_start = end;
        }
        offset = end;
    }
    let section = &input[section_start..];
    if section.lines().any(|line| line.trim_start().starts_with("bb")) {
        sections.push((name, section));
    }
    if sections.is_empty() {
        sections.push((input_name, input));
    }
    sections
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_evm_ir_sections_parse_and_verify() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/ui/codegen/emit_abi_bin.evmir.stdout");
        #[allow(clippy::disallowed_methods)]
        let input = std::fs::read_to_string(path).unwrap();
        let sections = evm_ir_sections(&input, "emit_abi_bin");
        assert_eq!(sections.len(), 4);
        for (_, section) in sections {
            let module = parse_evm_ir_module(section).unwrap();
            verify_evm_ir_module(&module).unwrap();
        }
    }
}
