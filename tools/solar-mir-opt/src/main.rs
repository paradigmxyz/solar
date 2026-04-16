//! `solar-mir-opt` — run one or more MIR transformation passes and print the
//! resulting MIR.
//!
//! This is the Solar equivalent of LLVM's `opt`. It accepts either a Solidity
//! file (`.sol`) — which is parsed, lowered to MIR, and then transformed —
//! or a textual MIR file (`.mir`) — which is parsed directly. After running
//! the requested pass pipeline, it prints the resulting MIR.
//!
//! ## Usage
//!
//! ```text
//! solar-mir-opt --passes <name1,name2,...> [--print-after-each] <input>
//! solar-mir-opt --pass <name> <input>          # alias for --passes <name>
//! solar-mir-opt -h | --help
//! ```
//!
//! ## Supported passes
//!
//! - `dce`             — Dead Code Elimination (fixed-point)
//! - `cfg-simplify`    — CFG Simplification (fixed-point)
//! - `jump-threading`  — Jump Threading (fixed-point)
//! - `none`            — No transform; just lower/parse and print
//!
//! Multiple passes can be chained: `--passes jump-threading,cfg-simplify,dce`.
//! With `--print-after-each`, the binary prints the MIR state after each pass
//! in the chain (useful for inspecting intermediate states).
//!
//! ## Input formats
//!
//! - `.sol` — Solidity contract; lowered through the normal compiler pipeline
//! - `.mir` — Textual MIR; parsed via `solar_codegen::mir::parse_module`

#![allow(unused_crate_dependencies)]

use solar_codegen::{
    lower,
    mir::{Module, module_to_text, parse_module},
    pass::{CfgSimplifyPass, DcePass, JumpThreadingPass, PassManager, TransformPass},
};
use solar_interface::{Ident, Session, Symbol};
use solar_sema::Compiler;
use std::{ops::ControlFlow, path::Path, process::ExitCode};

const HELP: &str = "\
solar-mir-opt — run one or more MIR passes on a Solidity or MIR file

Usage:
    solar-mir-opt --passes <names> [--print-after-each] <input>
    solar-mir-opt --pass <name> <input>          # alias for --passes <name>
    solar-mir-opt --pipeline-default <input>     # canonical codegen pipeline
    solar-mir-opt -h | --help

Options:
    --passes <names>     Comma-separated list of passes to run in order
    --pass <name>        Alias for --passes <name>
    --pipeline-default   Run the same passes as EvmCodegen::run_optimization_passes
                         (jump-threading → cfg-simplify → dce). Mutually
                         exclusive with --pass / --passes.
    --print-after-each   Print MIR after each pass in the chain
                         (default: print only after the last pass)
    -h, --help           Print this help message

Passes:
    dce              Dead Code Elimination (fixed-point)
    cfg-simplify     CFG Simplification (fixed-point)
    jump-threading   Jump Threading (fixed-point)
    none             No transform; just lower/parse and print

Input formats:
    *.sol  Solidity contract — lowered through the normal compiler pipeline
    *.mir  Textual MIR — parsed directly via solar_codegen::mir::parse_module
";

/// The canonical pass list run by `EvmCodegen::run_optimization_passes`.
/// Keep in sync with `crates/codegen/src/codegen/evm.rs`.
const DEFAULT_PIPELINE: &[&str] = &["jump-threading", "cfg-simplify", "dce"];

struct Args {
    /// Passes to run, in order. Each must satisfy `make_pass`.
    passes: Vec<String>,
    /// If true, print MIR after every pass; otherwise only after the last.
    print_after_each: bool,
    /// If true, the user invoked `--pipeline-default`. Used as the display
    /// label so output reads `(after pipeline-default)` instead of the
    /// concatenated pass list.
    pipeline_default: bool,
    /// Path to input file. Extension determines whether it's .sol or .mir.
    input: String,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().collect();
    let mut passes: Option<Vec<String>> = None;
    let mut pipeline_default = false;
    let mut print_after_each = false;
    let mut input: Option<String> = None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--passes" => {
                let raw = argv.get(i + 1).ok_or("--passes requires an argument")?;
                passes = Some(raw.split(',').map(str::trim).map(String::from).collect());
                i += 2;
            }
            "--pass" => {
                let raw = argv.get(i + 1).ok_or("--pass requires an argument")?;
                if passes.is_some() {
                    return Err("cannot use --pass and --passes together".into());
                }
                passes = Some(vec![raw.clone()]);
                i += 2;
            }
            "--pipeline-default" => {
                pipeline_default = true;
                i += 1;
            }
            "--print-after-each" => {
                print_after_each = true;
                i += 1;
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

    // --pipeline-default and --pass[es] are mutually exclusive.
    if pipeline_default && passes.is_some() {
        return Err("cannot use --pipeline-default with --pass / --passes".into());
    }

    let passes = if pipeline_default {
        DEFAULT_PIPELINE.iter().map(|s| (*s).to_string()).collect()
    } else {
        let p =
            passes.ok_or_else(|| "missing --pass / --passes / --pipeline-default".to_string())?;
        if p.is_empty() {
            return Err("at least one pass is required".into());
        }
        p
    };

    let input = input.ok_or_else(|| "missing input file".to_string())?;
    Ok(Args { passes, print_after_each, pipeline_default, input })
}

/// Returns a fresh boxed pass for the given name. The special `none` pass
/// returns `Ok(None)` (skipped). Unknown names return `Err`.
fn make_pass(name: &str) -> Result<Option<Box<dyn TransformPass>>, String> {
    match name {
        "dce" => Ok(Some(Box::new(DcePass))),
        "cfg-simplify" => Ok(Some(Box::new(CfgSimplifyPass))),
        "jump-threading" => Ok(Some(Box::new(JumpThreadingPass))),
        "none" => Ok(None),
        other => Err(format!("unknown pass: {other}")),
    }
}

/// Runs a single pass on every function in the module.
fn run_pass(module: &mut Module, pass_name: &str) -> Result<(), String> {
    if let Some(boxed) = make_pass(pass_name)? {
        let mut pm = PassManager::new();
        pm.add_transform(boxed);
        for func in module.functions.iter_mut() {
            pm.run(func);
        }
    }
    Ok(())
}

/// Prints a module with a header indicating which pass(es) produced it.
fn print_module(module: &Module, name: &str, after: &str) {
    println!("// === {name} (after {after}) ===");
    println!("{}", module_to_text(module));
}

/// Runs the pass pipeline on a single module and emits output.
/// Used for both .sol contracts and .mir input.
fn run_pipeline(module: &mut Module, name: &str, args: &Args) -> Result<(), String> {
    if args.print_after_each {
        for pass in &args.passes {
            run_pass(module, pass)?;
            print_module(module, name, pass);
        }
    } else {
        for pass in &args.passes {
            run_pass(module, pass)?;
        }
        // For --pipeline-default, use a stable label so the output header
        // doesn't depend on the underlying pass list (which may evolve).
        let label = if args.pipeline_default {
            "pipeline-default".to_string()
        } else {
            args.passes.join(",")
        };
        print_module(module, name, &label);
    }
    Ok(())
}

/// Process a `.mir` input: read file, parse, run passes, print.
fn process_mir(args: &Args) -> Result<(), String> {
    let text = std::fs::read_to_string(&args.input)
        .map_err(|e| format!("failed to read {}: {e}", args.input))?;

    let sess = Session::builder().with_stderr_emitter().build();
    let mut result: Result<(), String> = Ok(());
    sess.enter(|| {
        let mut module = match parse_module(&text) {
            Ok(m) => m,
            Err(e) => {
                result = Err(format!("{e}"));
                return;
            }
        };
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
fn process_sol(args: &Args) -> Result<(), String> {
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

    // Validate all pass names up front so users get fast feedback.
    for name in &args.passes {
        if let Err(e) = make_pass(name) {
            eprintln!("error: {e}");
            eprintln!();
            eprint!("{HELP}");
            return ExitCode::FAILURE;
        }
    }

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
