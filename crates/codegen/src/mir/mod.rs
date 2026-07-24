//! Mid-level Intermediate Representation (MIR).
//!
//! MIR is an SSA-form IR that sits between HIR and EVM bytecode.

use solar_data_structures::newtype_index;

mod types;
pub(crate) use types::{MemoryObjectKind, MemoryObjectLayout, MirType, SliceLocation};

mod abi;
pub(crate) use abi::{AbiLayout, AbiLayoutRef, AbiType};

mod storage;
pub use storage::{StorageField, StorageLayout, StorageLayoutRef};

mod value;
pub(crate) use value::{Immediate, Value};

mod inst;
pub(crate) use inst::{
    AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationKind,
    AllocationSemantics, EffectKind, InstKind, Instruction, InstructionMetadata, MemoryRegion,
    StorageAlias,
};

mod block;
pub(crate) use block::{BasicBlock, Terminator};

mod function;
pub(crate) use function::{Function, FunctionAttributes};

mod module;
pub(crate) use module::IMMUTABLE_WORD_SIZE;
pub use module::{MirPhase, Module};

mod builder;
pub(crate) use builder::FunctionBuilder;

mod display;

mod parser;

/// Validates the invariants of a MIR module.
pub fn validate(dcx: &solar_interface::diagnostics::DiagCtxt, module: &Module) {
    crate::analysis::validate(dcx, module);
}

pub(crate) mod utils;

newtype_index! {
    /// A unique identifier for a value in the MIR.
    pub(crate) struct ValueId;

    /// A unique identifier for an instruction in the MIR.
    pub(crate) struct InstId;

    /// A unique identifier for a basic block in the MIR.
    pub(crate) struct BlockId;

    /// A unique identifier for a function in the MIR.
    pub(crate) struct FunctionId;
}

impl BlockId {
    /// The first block in every function.
    pub(crate) const ENTRY: Self = Self::new(0);
}

/// Property tests verifying that the MIR printer/parser pair is self-consistent.
///
/// For each fixture under `tests/ui/codegen/`:
/// 1. Obtain a `Module` (either by lowering Solidity or by parsing `.mir` text).
/// 2. Print it (`print1`).
/// 3. Parse `print1` (`parsed1`).
/// 4. Print `parsed1` (`print2`).
/// 5. Parse `print2` (`parsed2`).
/// 6. Print `parsed2` (`print3`).
/// 7. Assert `print2 == print3` — i.e., the parser+printer pair is **idempotent**.
///
/// Why not assert `print1 == print2`? Raw `.mir` fixtures may use arbitrary
/// `vN` labels, and the first print canonicalizes them to result-instruction
/// indices. A *second* round-trip must be stable.
#[cfg(test)]
mod round_trip {
    use super::Module;
    use crate::lower;
    use solar_interface::{ColorChoice, Session};
    use solar_sema::Compiler;
    use std::{
        ops::ControlFlow,
        path::{Path, PathBuf},
    };

    fn parse_module(sess: &Session, input: &str) -> solar_interface::Result<Module> {
        super::parser::parse_module(sess, input)
    }

    /// Path to `tests/ui/codegen/` (the workspace's UI test directory).
    fn ui_codegen_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("ui")
            .join("codegen")
    }

    /// Returns the (line index, line A, line B) of the first divergence between
    /// two strings, or `None` if they're equal. Used to keep failure messages
    /// readable when the printed MIR is large.
    fn first_diff<'a>(a: &'a str, b: &'a str) -> Option<(usize, &'a str, &'a str)> {
        a.lines()
            .zip(b.lines())
            .enumerate()
            .find(|(_, (la, lb))| la != lb)
            .map(|(i, (la, lb))| (i + 1, la, lb))
    }

    fn fixture_paths(root: &Path, extension: &str) -> Vec<PathBuf> {
        let mut dirs = vec![root.to_path_buf()];
        let mut paths = Vec::new();
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some(extension) {
                    paths.push(path);
                }
            }
        }
        paths.sort_unstable();
        paths
    }

    #[test]
    fn round_trip_all_sol_files() {
        let dir = ui_codegen_dir();
        assert!(dir.exists(), "ui codegen dir not found: {}", dir.display());

        let mut failures: Vec<String> = Vec::new();
        let mut count = 0usize;
        for path in fixture_paths(&dir, "sol") {
            count += 1;
            if let Err(e) = round_trip_sol(&path) {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                failures.push(format!("{name}: {e}"));
            }
        }
        assert!(count > 0, "no .sol fixtures found in {}", dir.display());
        assert!(
            failures.is_empty(),
            "{} round-trip failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    #[test]
    fn validate_all_lowered_sol_modules() {
        // Sanity check: every .sol fixture under tests/ui/codegen/ should
        // lower to well-formed MIR (the validator finds zero errors).
        let dir = ui_codegen_dir();
        let mut failures: Vec<String> = Vec::new();
        let mut count = 0usize;
        for path in fixture_paths(&dir, "sol") {
            count += 1;
            if let Err(e) = validate_sol(&path) {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                failures.push(format!("{name}: {e}"));
            }
        }
        assert!(count > 0, "no .sol fixtures found");
        assert!(
            failures.is_empty(),
            "{} validation failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    fn validate_sol(path: &Path) -> Result<(), String> {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let mut compiler = Compiler::new(sess);

        let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<()> {
            let mut pcx = c.parse();
            pcx.load_files([path])?;
            pcx.parse();
            Ok(())
        });
        if parse_result.is_err() {
            return Err("parse failed".into());
        }

        let mut result: Result<(), String> = Ok(());
        let _ = compiler.enter_mut(|c| -> solar_interface::Result<()> {
            let ControlFlow::Continue(()) = c.lower_asts()? else { return Ok(()) };
            let ControlFlow::Continue(()) = c.analysis()? else { return Ok(()) };
            let gcx = c.gcx();
            for id in gcx.hir.contract_ids() {
                let contract = gcx.hir.contract(id);
                if contract.kind.is_interface() || contract.kind.is_abstract_contract() {
                    continue;
                }
                let module = lower::lower_contract(gcx, id);
                let errors_before = gcx.dcx().err_count();
                super::validate(gcx.dcx(), &module);
                if gcx.dcx().err_count() != errors_before {
                    result = Err(format!(
                        "contract `{}` has invalid MIR:\n{}",
                        contract.name,
                        gcx.dcx().emitted_diagnostics().unwrap()
                    ));
                    return Ok(());
                }
            }
            Ok(())
        });
        result
    }

    #[test]
    fn round_trip_all_mir_files() {
        let dir = ui_codegen_dir().join("mir");
        assert!(dir.exists(), "mir test dir not found: {}", dir.display());

        let mut failures: Vec<String> = Vec::new();
        let mut count = 0usize;
        for path in fixture_paths(&dir, "mir") {
            count += 1;
            if let Err(e) = round_trip_mir(&path) {
                if e.starts_with("first parse failed:") && path.with_extension("stderr").is_file() {
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                failures.push(format!("{name}: {e}"));
            }
        }
        assert!(count > 0, "no .mir fixtures found in {}", dir.display());
        assert!(
            failures.is_empty(),
            "{} round-trip failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    /// Round-trips one Solidity file: lower → print → parse → print → parse →
    /// print and asserts the last two prints match.
    fn round_trip_sol(path: &Path) -> Result<(), String> {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let mut compiler = Compiler::new(sess);

        let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<()> {
            let mut pcx = c.parse();
            pcx.load_files([path])?;
            pcx.parse();
            Ok(())
        });
        if parse_result.is_err() {
            return Err("parse failed".into());
        }

        let mut result: Result<(), String> = Ok(());
        let _ = compiler.enter_mut(|c| -> solar_interface::Result<()> {
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
                let module = lower::lower_contract(gcx, id);
                if let Err(e) = check_round_trip_module(gcx.sess, &module) {
                    result = Err(format!("contract `{}`: {e}", contract.name));
                    return Ok(());
                }
            }
            Ok(())
        });
        result
    }

    /// Round-trips one `.mir` file. Skips the lowering step.
    fn round_trip_mir(path: &Path) -> Result<(), String> {
        #[allow(clippy::disallowed_methods)]
        let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        // Strip `//@compile-flags:` annotations the test harness reads — they're
        // not valid MIR and the parser would treat them as comments anyway, but
        // be explicit so we don't accidentally rely on parser behavior.
        let text: String =
            raw.lines().filter(|l| !l.starts_with("//@")).collect::<Vec<_>>().join("\n");

        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let mut result: Result<(), String> = Ok(());
        sess.enter(|| {
            let parsed1 = match parse_module(&sess, &text) {
                Ok(m) => m,
                Err(_) => {
                    result =
                        Err(format!("first parse failed: {}", sess.emitted_diagnostics().unwrap()));
                    return;
                }
            };
            let print1 = parsed1.to_text().to_string();
            let parsed2 = match parse_module(&sess, &print1) {
                Ok(m) => m,
                Err(_) => {
                    result = Err(format!(
                        "second parse failed: {}",
                        sess.emitted_diagnostics().unwrap()
                    ));
                    return;
                }
            };
            let print2 = parsed2.to_text().to_string();
            if print1 != print2 {
                let diff = first_diff(&print1, &print2)
                    .map(|(i, a, b)| format!("line {i}: `{a}` vs `{b}`"))
                    .unwrap_or_else(|| "(length mismatch)".to_string());
                result = Err(format!("not idempotent: {diff}"));
            }
        });
        result
    }

    /// Common idempotency check: print → parse → print → parse → print, last two
    /// must match. Caller must already be inside an active `Session::enter`.
    fn check_round_trip_module(sess: &Session, module: &Module) -> Result<(), String> {
        let print1 = module.to_text().to_string();
        let parsed1 = parse_module(sess, &print1).map_err(|_| {
            format!(
                "first parse: {}\n--- print1 ---\n{print1}",
                sess.emitted_diagnostics().unwrap()
            )
        })?;
        let print2 = parsed1.to_text().to_string();
        let parsed2 = parse_module(sess, &print2).map_err(|_| {
            format!(
                "second parse: {}\n--- print1 ---\n{print1}\n--- print2 ---\n{print2}",
                sess.emitted_diagnostics().unwrap()
            )
        })?;
        let print3 = parsed2.to_text().to_string();

        if print2 != print3 {
            let diff = first_diff(&print2, &print3)
                .map(|(i, a, b)| format!("line {i}: `{a}` vs `{b}`"))
                .unwrap_or_else(|| "(length mismatch)".to_string());
            return Err(format!(
                "not idempotent: {diff}\n--- print2 ---\n{print2}\n--- print3 ---\n{print3}"
            ));
        }
        Ok(())
    }
}
