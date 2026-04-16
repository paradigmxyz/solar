//! `solar-mir-opt` — run a single MIR transformation pass on a Solidity file
//! and print the resulting MIR.
//!
//! This is the Solar equivalent of LLVM's `opt`. It takes a Solidity contract,
//! lowers it to MIR, runs the requested pass, and prints the result. It's
//! meant for inspecting what individual passes do in isolation, complementary
//! to `solar --emit=mir` (which runs the full pipeline).
//!
//! ## Usage
//!
//! ```text
//! solar-mir-opt --pass <name> <file.sol>
//! ```
//!
//! Supported passes:
//!   - `dce`             — Dead Code Elimination
//!   - `cfg-simplify`    — CFG Simplification
//!   - `jump-threading`  — Jump Threading
//!   - `none`            — No transform; just lower and print
//!
//! ## Future
//!
//! Once a MIR text parser exists, this binary will also accept `.mir`
//! input files. The `--pass` plumbing won't change.

#![allow(unused_crate_dependencies)]

use solar_codegen::{
    lower,
    mir::module_to_text,
    pass::{CfgSimplifyPass, DcePass, JumpThreadingPass, PassManager, TransformPass},
};
use solar_interface::Session;
use solar_sema::Compiler;
use std::{ops::ControlFlow, path::Path, process::ExitCode};

const HELP: &str = "\
solar-mir-opt — run a single MIR pass on a Solidity contract

Usage:
    solar-mir-opt --pass <name> <file.sol>
    solar-mir-opt -h | --help

Options:
    --pass <name>    Name of the pass to run (see below)
    -h, --help       Print this help message

Passes:
    dce              Dead Code Elimination (fixed-point)
    cfg-simplify     CFG Simplification (fixed-point)
    jump-threading   Jump Threading (fixed-point)
    none             No transform; just lower and print
";

struct Args {
    pass: String,
    input: String,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().collect();
    let mut pass: Option<String> = None;
    let mut input: Option<String> = None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--pass" => {
                pass = argv.get(i + 1).cloned();
                if pass.is_none() {
                    return Err("--pass requires an argument".into());
                }
                i += 2;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unknown flag: {arg}"));
            }
            _ => {
                if input.is_some() {
                    return Err("only one input file is supported".into());
                }
                input = Some(argv[i].clone());
                i += 1;
            }
        }
    }
    let pass = pass.ok_or_else(|| "missing --pass <name>".to_string())?;
    let input = input.ok_or_else(|| "missing input file".to_string())?;
    Ok(Args { pass, input })
}

/// Returns a fresh boxed pass for the given name, or `None` if `name` is unknown.
/// Returns `Some(None)` for the special `none` pass (no transform).
fn make_pass(name: &str) -> Option<Option<Box<dyn TransformPass>>> {
    match name {
        "dce" => Some(Some(Box::new(DcePass))),
        "cfg-simplify" => Some(Some(Box::new(CfgSimplifyPass))),
        "jump-threading" => Some(Some(Box::new(JumpThreadingPass))),
        "none" => Some(None),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprint!("{HELP}");
            return ExitCode::FAILURE;
        }
    };

    // Validate the pass name early so users get a fast error.
    if make_pass(&args.pass).is_none() {
        eprintln!("error: unknown pass: {}", args.pass);
        eprintln!();
        eprint!("{HELP}");
        return ExitCode::FAILURE;
    }

    let sess = Session::builder().with_stderr_emitter().build();
    let mut compiler = Compiler::new(sess);

    // Parse the input file.
    let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
        let mut pcx = c.parse();
        pcx.load_files([Path::new(&args.input)])?;
        pcx.parse();
        Ok(())
    });
    if parse_result.is_err() {
        return ExitCode::FAILURE;
    }

    // Lower, analyze, and run the requested pass on each contract.
    let result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
        let ControlFlow::Continue(()) = c.lower_asts()? else {
            return Ok(());
        };
        let ControlFlow::Continue(()) = c.analysis()? else {
            return Ok(());
        };

        let gcx = c.gcx();
        for id in gcx.hir.contract_ids() {
            let contract = gcx.hir.contract(id);
            if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
                continue;
            }

            let mut module = lower::lower_contract(gcx, id);

            // Run the requested pass on every function in the module.
            // `none` skips the pass manager entirely so the output reflects
            // the raw lowered MIR.
            if let Some(Some(boxed_pass)) = make_pass(&args.pass) {
                let mut pm = PassManager::new();
                pm.add_transform(boxed_pass);
                for func in module.functions.iter_mut() {
                    pm.run(func);
                }
            }

            let name = gcx.contract_fully_qualified_name(id);
            println!("// === {} (after {}) ===", name, args.pass);
            println!("{}", module_to_text(&module));
        }
        Ok(())
    });

    if result.is_err() || compiler.sess().emitted_errors().is_some_and(|r| r.is_err()) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
