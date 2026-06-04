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

use clap::{Parser, ValueEnum, ValueHint};
use solar_codegen::{
    lower,
    mir::{Module, module_to_text, parse_module},
    pass::{
        CfgSimplifyPass, CsePass, DcePass, InstSimplifyPass, JumpThreadingPass, LicmPass,
        MemoryDsePass, PassManager, SccpTransformPass, StorageScalarPromotionPass, TransformPass,
    },
    transform::{DeadFunctionEliminator, MirInliner},
};
use solar_interface::{Ident, Session, Symbol};
use solar_sema::Compiler;
use std::{ffi::OsString, ops::ControlFlow, path::Path, process::ExitCode};

const AFTER_HELP: &str = "\
Passes:
  inline           Internal MIR function inlining
  function-dce     Dead internal function elimination
  dce              Dead Code Elimination (fixed-point)
  inst-simplify    Local MIR instruction simplification
  cse              Common Subexpression Elimination (fixed-point)
  sccp             Sparse Conditional Constant Propagation
  licm             Loop-Invariant Code Motion
  cfg-simplify     CFG Simplification (fixed-point)
  jump-threading   Jump Threading (fixed-point)
  memory-dse       Local dead memory-store elimination
  storage-promotion
                   Promote simple loop-carried storage updates to memory
  none             No transform; just lower/parse and print

Input formats:
  *.sol  Solidity contract — lowered through the normal compiler pipeline
  *.mir  Textual MIR — parsed directly via solar_codegen::mir::parse_module
";

/// The canonical pass list run by `EvmCodegen::run_optimization_passes`.
/// Keep in sync with `crates/codegen/src/backend/evm/codegen.rs`.
const DEFAULT_PIPELINE: &[PassName] = &[
    PassName::Inline,
    PassName::FunctionDce,
    PassName::Sccp,
    PassName::InstSimplify,
    PassName::Cse,
    PassName::StoragePromotion,
    PassName::Licm,
    PassName::JumpThreading,
    PassName::CfgSimplify,
    PassName::MemoryDse,
    PassName::Dce,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum PassName {
    Inline,
    FunctionDce,
    Dce,
    InstSimplify,
    Cse,
    Sccp,
    Licm,
    CfgSimplify,
    JumpThreading,
    MemoryDse,
    StoragePromotion,
    None,
}

impl PassName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::FunctionDce => "function-dce",
            Self::Dce => "dce",
            Self::InstSimplify => "inst-simplify",
            Self::Cse => "cse",
            Self::Sccp => "sccp",
            Self::Licm => "licm",
            Self::CfgSimplify => "cfg-simplify",
            Self::JumpThreading => "jump-threading",
            Self::MemoryDse => "memory-dse",
            Self::StoragePromotion => "storage-promotion",
            Self::None => "none",
        }
    }
}

#[derive(Parser)]
#[command(
    name = "solar mir-opt",
    about = "Run one or more MIR passes on a Solidity or MIR file",
    arg_required_else_help = true,
    after_help = AFTER_HELP,
)]
struct Args {
    /// Comma-separated list of passes to run in order.
    #[arg(
        long = "passes",
        visible_alias = "pass",
        value_name = "NAMES",
        value_delimiter = ',',
        required_unless_present = "pipeline_default",
        conflicts_with = "pipeline_default"
    )]
    passes: Option<Vec<PassName>>,
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

impl Args {
    fn passes(&self) -> &[PassName] {
        if self.pipeline_default {
            DEFAULT_PIPELINE
        } else {
            self.passes.as_deref().expect("clap requires passes unless pipeline-default is set")
        }
    }

    fn pipeline_label(&self) -> String {
        if self.pipeline_default {
            "pipeline-default".to_string()
        } else {
            self.passes().iter().map(|p| p.as_str()).collect::<Vec<_>>().join(",")
        }
    }
}

enum MirOptPass {
    Inline(MirInliner),
    FunctionDce(DeadFunctionEliminator),
    Function(Box<dyn TransformPass>),
}

/// Returns a fresh pass for the given name. The special `none` pass returns `None` (skipped).
fn make_pass(name: PassName) -> Option<MirOptPass> {
    match name {
        PassName::Inline => Some(MirOptPass::Inline(MirInliner::default())),
        PassName::FunctionDce => Some(MirOptPass::FunctionDce(DeadFunctionEliminator::new())),
        PassName::Dce => Some(MirOptPass::Function(Box::new(DcePass))),
        PassName::InstSimplify => Some(MirOptPass::Function(Box::new(InstSimplifyPass))),
        PassName::Cse => Some(MirOptPass::Function(Box::new(CsePass))),
        PassName::Sccp => Some(MirOptPass::Function(Box::new(SccpTransformPass))),
        PassName::Licm => Some(MirOptPass::Function(Box::new(LicmPass))),
        PassName::CfgSimplify => Some(MirOptPass::Function(Box::new(CfgSimplifyPass))),
        PassName::JumpThreading => Some(MirOptPass::Function(Box::new(JumpThreadingPass))),
        PassName::MemoryDse => Some(MirOptPass::Function(Box::new(MemoryDsePass))),
        PassName::StoragePromotion => {
            Some(MirOptPass::Function(Box::new(StorageScalarPromotionPass)))
        }
        PassName::None => None,
    }
}

/// Runs a single pass over the module.
fn run_pass(module: &mut Module, pass_name: PassName) {
    if let Some(pass) = make_pass(pass_name) {
        match pass {
            MirOptPass::Inline(mut pass) => {
                pass.run(module);
            }
            MirOptPass::FunctionDce(mut pass) => {
                pass.run(module);
            }
            MirOptPass::Function(boxed) => {
                let mut pm = PassManager::new();
                pm.add_transform(boxed);
                for func in module.functions.iter_mut() {
                    pm.run(func);
                }
            }
        }
    }
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
        for &pass in args.passes() {
            run_pass(module, pass);
            print_module(module, name, pass.as_str());
        }
    } else {
        for &pass in args.passes() {
            run_pass(module, pass);
        }
        // For --pipeline-default, use a stable label so the output header doesn't depend on the
        // underlying pass list (which may evolve).
        let label = args.pipeline_label();
        print_module(module, name, &label);
    }
    Ok(())
}

/// Process a `.mir` input: read file, parse, run passes, print.
fn process_mir(args: &Args) -> Result<(), String> {
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

/// Entry point for the `mir-opt` subcommand. `argv` is the arguments following
/// `mir-opt` (i.e. excluding the program name and the subcommand itself).
pub fn run(argv: &[OsString]) -> ExitCode {
    let args = Args::try_parse_from(
        std::iter::once(OsString::from("solar mir-opt")).chain(argv.iter().cloned()),
    )
    .unwrap_or_else(|e| e.exit());

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
