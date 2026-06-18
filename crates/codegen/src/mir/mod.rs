//! Mid-level Intermediate Representation (MIR).
//!
//! MIR is an SSA-form IR that sits between HIR and EVM bytecode.

use solar_data_structures::newtype_index;

mod types;
pub use types::MirType;

mod value;
pub use value::{Immediate, Value};

mod inst;
pub use inst::{
    EffectKind, InstKind, InstTag, Instruction, InstructionMetadata, MemoryRegion, StorageAlias,
};

mod block;
pub use block::{BasicBlock, Terminator};

mod function;
pub use function::{Function, FunctionAttributes};

mod module;
pub use module::{DataSegment, IMMUTABLE_WORD_SIZE, ImmutableSlot, Module, StorageSlot};

mod builder;
pub use builder::FunctionBuilder;

mod display;

mod parser;
pub use parser::{ParseError, parse_function, parse_module};

newtype_index! {
    /// A unique identifier for a value in the MIR.
    pub struct ValueId;
}

newtype_index! {
    /// A unique identifier for an instruction in the MIR.
    pub struct InstId;
}

newtype_index! {
    /// A unique identifier for a basic block in the MIR.
    pub struct BlockId;
}

newtype_index! {
    /// A unique identifier for a function in the MIR.
    pub struct FunctionId;
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
/// Why not assert `print1 == print2`? The first parse allocates immediates as
/// `Value::Immediate` entries before allocating instruction results, which can
/// shift the `vN` numbering relative to the original lowered MIR. After one
/// round-trip the value numbers correspond directly to actual `ValueId`s, so
/// a *second* round-trip is stable.
#[cfg(test)]
mod round_trip {
    use super::{Module, parse_module};
    use crate::{analysis::validate_module, lower};
    use solar_interface::{ColorChoice, Session};
    use solar_sema::Compiler;
    use std::{
        ops::ControlFlow,
        path::{Path, PathBuf},
    };

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

    #[test]
    fn round_trip_all_sol_files() {
        let dir = ui_codegen_dir();
        assert!(dir.exists(), "ui codegen dir not found: {}", dir.display());

        let mut failures: Vec<String> = Vec::new();
        let mut count = 0usize;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("sol") {
                continue;
            }
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
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("sol") {
                continue;
            }
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
                let errors = validate_module(&module);
                if !errors.is_empty() {
                    result = Err(format!(
                        "contract `{}` has {} validation error(s):\n    {}",
                        contract.name,
                        errors.len(),
                        errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n    ")
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
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("mir") {
                continue;
            }
            count += 1;
            if let Err(e) = round_trip_mir(&path) {
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
                if let Err(e) = check_round_trip_module(&module) {
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
        // Tests don't need a SourceMap; reading a fixture as plain text is fine.
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
            let parsed1 = match parse_module(&text) {
                Ok(m) => m,
                Err(e) => {
                    result = Err(format!("first parse failed: {e}"));
                    return;
                }
            };
            let print1 = parsed1.to_text().to_string();
            let parsed2 = match parse_module(&print1) {
                Ok(m) => m,
                Err(e) => {
                    result = Err(format!("second parse failed: {e}"));
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
    fn check_round_trip_module(module: &Module) -> Result<(), String> {
        let print1 = module.to_text().to_string();
        let parsed1 = parse_module(&print1)
            .map_err(|e| format!("first parse: {e}\n--- print1 ---\n{print1}"))?;
        let print2 = parsed1.to_text().to_string();
        let parsed2 = parse_module(&print2).map_err(|e| {
            format!("second parse: {e}\n--- print1 ---\n{print1}\n--- print2 ---\n{print2}")
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
