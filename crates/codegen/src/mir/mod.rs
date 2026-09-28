//! Mid-level Intermediate Representation (MIR).
//!
//! MIR is an SSA-form IR that sits between HIR and EVM bytecode.

use solar_data_structures::newtype_index;

pub(crate) use crate::link::{Data, DataBytes, DataId, DataRef, DataSize};

pub(crate) mod analysis;
pub(crate) mod immutable;
pub mod lower;
pub(crate) mod memory;
pub mod pass;
pub(crate) mod pass_manager;
mod transform;

mod types;
mod typing;
pub(crate) use types::{
    FrameMode, FrameSlotKind, ImmutableEncoding, MemoryObjectKind, MemoryObjectLayout, MirType,
    SliceLocation, StructType, TypeSize, ValueLayout,
};

mod abi;
pub(crate) use abi::{
    AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamLayoutRef, AbiParamLocation, AbiParamType,
    AbiType, AbiWordValidator,
};

mod packed;
pub(crate) use packed::{
    PackedArraySource, PackedPart, PrefixedBytes, packed_element_bytes, prefix_over_length,
    restore_length, static_prefix_size,
};

mod storage;
pub use storage::{StorageField, StorageLayout, StorageLayoutRef};

mod value;
pub(crate) use value::{Immediate, Value};

mod inst;
pub(crate) use inst::{
    AbiEncodeMode, AddressCallKind, AllocationAlignment, AllocationFailure,
    AllocationInitialization, AllocationKind, AllocationSemantics, ConcatPart, EffectKind,
    Instruction, InstructionMetadata, MemoryRegion, StorageAlias,
};

mod arithmetic;
pub(crate) use arithmetic::{ArithmeticKind, CheckedOp};

mod checks;
pub(crate) use checks::{PanicCode, RevertKind, RevertPayload, RevertReason};

mod effects;
pub(crate) use effects::ControlEffects;
mod op_schema;
pub(crate) use op_schema::{InstKind, Op, OpTraits, ResultKind};

mod semantics;
pub(crate) use semantics::Semantics;

mod block;
pub(crate) use block::{BasicBlock, Terminator};

mod mangling;
pub(crate) use mangling::{Disambiguator, MangledSymbol};

mod function;
pub(crate) use function::{Function, FunctionAttributes};

mod module;
pub(crate) use module::LoweredModule;
pub use module::{MirPhase, Module};

mod builtin;
pub(crate) use builtin::{Builtin, Callee, RequireKind};

mod builder;
pub(crate) use builder::{ERROR_SELECTOR, FunctionBuilder};

mod display;

mod parser;

/// Validates the invariants of a MIR module.
pub fn validate(dcx: &solar_interface::diagnostics::DiagCtxt, module: &Module) {
    crate::mir::analysis::validate(dcx, module);
}

pub(crate) mod utils;

newtype_index! {
    /// A function argument index in the MIR.
    pub(crate) struct ArgIdx;

    /// A unique identifier for a value in the MIR.
    pub(crate) struct ValueId;

    /// A unique identifier for an instruction in the MIR.
    pub(crate) struct InstId;

    /// A unique identifier for a basic block in the MIR.
    pub(crate) struct BlockId;

    /// A fixed aggregate type declared in a MIR module.
    pub(crate) struct StructId;

    /// A unique identifier for a function in the MIR.
    pub(crate) struct FunctionId;

    /// A unique identifier for an immutable in the MIR module.
    pub(crate) struct ImmutableId;
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
    use super::{Function, FunctionId, InstKind, MirPhase, Module, Value};
    use crate::mir::{
        analysis, lower,
        transform::dce::DeadCodeEliminator,
        utils::interp::{Limits, Machine, Memory, mix64},
    };
    use alloy_primitives::U256;
    use snapbox::{assert_data_eq, str};
    use solar_interface::{
        ColorChoice, Session,
        diagnostics::DiagCtxt,
        source_map::{FileName, SourceMap},
    };
    use solar_sema::Compiler;
    use std::{
        ops::ControlFlow,
        panic::{AssertUnwindSafe, catch_unwind},
        path::{Path, PathBuf},
        sync::Arc,
    };

    fn parse_module(sess: &Session, input: &str) -> solar_interface::Result<Module> {
        super::parser::parse_module(sess, input)
    }

    #[test]
    fn i1_literals_are_canonical() {
        for (literal, valid) in [("0", true), ("1", true), ("2", false), ("0xff", false)] {
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            sess.enter(|| {
                let input = format!(
                    "@module BoolLiterals\nfn @f() -> i1 {{\n  bb0:\n    ret i1 {literal}\n}}\n"
                );
                assert_eq!(parse_module(&sess, &input).is_ok(), valid, "{literal}");
            });
        }
    }

    #[test]
    fn cast_source_types() {
        for (cast, valid) in [
            ("zext i1 0 to i256", true),
            ("zext i1 1 to i256", true),
            ("zext i1 2 to i256", false),
            ("zext i1 arg0 to i256", true),
            ("zext i160 arg0 to i256", false),
            ("zext i1 undef to i256", true),
            ("ptrtoint memptr arg0 to i256", false),
        ] {
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            sess.enter(|| {
                let input = format!(
                    "@module Casts\nfn @f(arg0: i1) -> i256 {{\n  bb0:\n    v0 = {cast}\n    ret v0\n}}\n"
                );
                assert_eq!(parse_module(&sess, &input).is_ok(), valid, "{cast}");
            });
        }
    }

    #[test]
    fn scalar_integer_types() {
        for (ty, valid) in [
            ("i1", true),
            ("i256", true),
            ("i8", true),
            ("i128", true),
            ("i160", true),
            ("i7", true),
            ("i512", true),
            ("i4294967295", true),
            ("i0", false),
            ("i4294967296", false),
            ("i", false),
            ("iabc", false),
            ("bool", false),
            ("word", false),
            ("u256", false),
        ] {
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            sess.enter(|| {
                let input = format!(
                    "@module IntegerTypes\nfn @f(arg0: {ty}) -> {ty} {{\n  bb0:\n    ret arg0\n}}\n"
                );
                assert_eq!(parse_module(&sess, &input).is_ok(), valid, "{ty}");
            });
        }
    }

    #[test]
    fn integer_literals_fit_their_width() {
        for (ty, literal, valid) in [
            ("i7", "127", true),
            ("i7", "128", false),
            ("i8", "255", true),
            ("i8", "256", false),
            ("i160", "0xffffffffffffffffffffffffffffffffffffffff", true),
            ("i160", "0x10000000000000000000000000000000000000000", false),
            ("i512", "42", true),
        ] {
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            sess.enter(|| {
                let input = format!(
                    "@module IntegerLiterals\nfn @f() -> {ty} {{\n  bb0:\n    ret {ty} {literal}\n}}\n"
                );
                assert_eq!(parse_module(&sess, &input).is_ok(), valid, "{ty} {literal}");
            });
        }
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
        let mut dirs = Vec::new();
        dirs.push(root.to_path_buf());
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
            if let Err(error) = check_signatures(&parsed1, &parsed2) {
                result = Err(error);
                return;
            }
            let print2 = parsed2.to_text().to_string();
            let parsed3 = match parse_module(&sess, &print2) {
                Ok(m) => m,
                Err(_) => {
                    result =
                        Err(format!("third parse failed: {}", sess.emitted_diagnostics().unwrap()));
                    return;
                }
            };
            let print3 = parsed3.to_text().to_string();
            if print2 != print3 {
                let diff = first_diff(&print2, &print3)
                    .map(|(i, a, b)| format!("line {i}: `{a}` vs `{b}`"))
                    .unwrap_or_else(|| "(length mismatch)".to_string());
                result = Err(format!("not idempotent: {diff}"));
            }
        });
        result
    }

    fn check_signatures(original: &Module, parsed: &Module) -> Result<(), String> {
        if original.struct_types != parsed.struct_types {
            return Err("struct declarations changed during round-trip".into());
        }
        if original.functions.len() != parsed.functions.len() {
            return Err("function count changed during round-trip".into());
        }
        for (before, after) in original.functions.iter().zip(&parsed.functions) {
            if before.selector.is_none()
                && (before.return_type() != after.return_type()
                    || before.return_abi() != after.return_abi())
            {
                return Err(format!("return types of `{}` changed during round-trip", before.name));
            }
        }
        Ok(())
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
        check_signatures(module, &parsed1)?;
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

    #[test]
    fn candidate_restrictions() {
        const MODULE: &str = "@module Candidates
fn @helper(arg0: i256) -> i256 {
  bb0:
    ret arg0
}

fn @f(arg0: i256) -> i256 {
  bb0:
    v0 = icall @helper, arg0
    ret v0
}
";
        let cases = [
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = icall @helper, arg0\n    ret v0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = add arg0, 1 !metadata(unchecked)\n    ret v0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    ret undef i256\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    ret err\n}\n",
            "fn @f() -> i256 {\n  bb0:\n    ret arg0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = icall fn0, arg0\n    ret v0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = icall @missing, arg0\n    ret v0\n}\n",
            "fn @f(arg0: i256) -> i256 [entry] {\n  bb0:\n    ret arg0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = abi_encode [word], args arg0\n    ret arg0\n}\n",
            "fn @f(arg0: i256) -> i256 {\n  bb0:\n    ret arg0\n}\nfn @g() {\n  bb0:\n    stop\n}\n",
            "@module Candidates\nfn @f(arg0: i256) -> i256 {\n  bb0:\n    ret arg0\n}\n",
        ];
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, MODULE).unwrap();
            let results = cases
                .iter()
                .map(|case| match parse_candidate(&sess, &module, case) {
                    Ok(candidate) => module.candidate_text(&candidate).to_string(),
                    Err(error) => error,
                })
                .collect::<Vec<_>>();
            assert_data_eq!(
                results.join("\n"),
                str![[r#"
fn @f(arg0: i256) -> i256 {
  bb0:
    v0 = icall @helper, arg0
    ret v0
}

error: candidates may not carry `!metadata`
  ╭▸ <candidate>:3:22
  │
3 │     v0 = add arg0, 1 !metadata(unchecked)
  ╰╴                     ━


error: candidates may not use `undef`
  ╭▸ <candidate>:3:9
  │
3 │     ret undef i256
  ╰╴        ━━━━━


error: candidates may not use `err`
  ╭▸ <candidate>:3:9
  │
3 │     ret err
  ╰╴        ━━━


error: candidates must declare the arguments they use
  ╭▸ <candidate>:3:9
  │
3 │     ret arg0
  ╰╴        ━━━━


error: candidates must reference functions by `@name`
  ╭▸ <candidate>:3:16
  │
3 │     v0 = icall fn0, arg0
  ╰╴               ━━━


error: unknown function reference `missing`
  ╭▸ <candidate>:3:17
  │
3 │     v0 = icall @missing, arg0
  ╰╴                ━━━━━━━


error: candidates may not declare `entry`
  ╭▸ <candidate>:1:28
  │
1 │ fn @f(arg0: i256) -> i256 [entry] {
  ╰╴                           ━━━━━


error: candidates may not use ABI layouts
  ╭▸ <candidate>:3:21
  │
3 │     v0 = abi_encode [word], args arg0
  ╰╴                    ━


error: expected the end of the candidate after its function
  ╭▸ <candidate>:5:1
  │
5 │ fn @g() {
  ╰╴━━


error: expected `fn`
  ╭▸ <candidate>:1:1
  │
1 │ @module Candidates
  ╰╴━


"#]]
            );
        });
    }

    #[test]
    fn candidate_round_trip_all_mir_files() {
        let dir = ui_codegen_dir().join("mir");
        let mut failures = Vec::new();
        let mut round_trips = 0usize;
        for path in fixture_paths(&dir, "mir") {
            #[allow(clippy::disallowed_methods)]
            let raw = std::fs::read_to_string(&path).unwrap();
            let text = raw.lines().filter(|l| !l.starts_with("//@")).collect::<Vec<_>>().join("\n");
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            sess.enter(|| {
                let Ok(module) = parse_module(&sess, &text) else { return };
                if module.phase() != MirPhase::Lowered {
                    return;
                }
                let valid = validate(&module, None).is_ok();
                for (id, func) in module.iter_functions() {
                    match check_candidate_round_trip(&sess, &module, id, func, valid) {
                        Ok(true) => round_trips += 1,
                        Ok(false) => {}
                        Err(error) => failures.push(format!("{name} `{}`: {error}", func.name)),
                    }
                }
            });
        }
        assert!(round_trips > 0, "no lowered MIR function round-tripped as a candidate");
        assert!(
            failures.is_empty(),
            "{} candidate round-trip failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    /// Prints `func` as a candidate and parses it back twice. The second print must be stable,
    /// and a candidate of a valid module must validate in place of `func`.
    ///
    /// Returns whether `func` round-tripped: only functions of invalid modules and functions with
    /// undefined or error values, or implicit arguments, may not.
    fn check_candidate_round_trip(
        sess: &Session,
        module: &Module,
        id: FunctionId,
        func: &Function,
        valid: bool,
    ) -> Result<bool, String> {
        let expressible = func.arg_indices().count() == func.params.len()
            && func
                .live_values()
                .all(|value| !matches!(func.value(value), Value::Undef(_) | Value::Error(_)));
        let print1 = module.candidate_text(func).to_string();
        let candidate = match parse_candidate(sess, module, &print1) {
            Ok(candidate) => candidate,
            Err(_) if !valid || !expressible => return Ok(false),
            Err(error) => return Err(format!("first parse: {error}\n--- print1 ---\n{print1}")),
        };
        let print2 = module.candidate_text(&candidate).to_string();
        let candidate = parse_candidate(sess, module, &print2)
            .map_err(|error| format!("second parse: {error}\n--- print2 ---\n{print2}"))?;
        let print3 = module.candidate_text(&candidate).to_string();
        if print2 != print3 {
            let diff = first_diff(&print2, &print3)
                .map(|(i, a, b)| format!("line {i}: `{a}` vs `{b}`"))
                .unwrap_or_else(|| "(length mismatch)".to_string());
            return Err(format!("not idempotent: {diff}\n--- print2 ---\n{print2}"));
        }
        if valid {
            validate(module, Some((id, &candidate)))
                .map_err(|error| format!("invalid candidate: {error}\n--- print2 ---\n{print2}"))?;
        }
        Ok(true)
    }

    /// Parses `input` as a candidate for `module`, rendering diagnostics privately.
    fn parse_candidate(sess: &Session, module: &Module, input: &str) -> Result<Function, String> {
        let source_map = Arc::new(SourceMap::empty());
        let dcx = DiagCtxt::with_buffer_emitter(Some(Arc::clone(&source_map)), ColorChoice::Never)
            .with_flags(|flags| flags.track_diagnostics = false);
        let fork = sess.with_diagnostics(dcx);
        let file = source_map.new_source_file(FileName::Custom("candidate".into()), input).unwrap();
        module
            .parse_function(&fork, &file)
            .map_err(|_| fork.dcx.emitted_diagnostics().unwrap().to_string())
    }

    /// Validates `module` at its phase, or only `candidate` in place of one of its functions.
    fn validate(module: &Module, candidate: Option<(FunctionId, &Function)>) -> Result<(), String> {
        let dcx = DiagCtxt::with_buffer_emitter(None, ColorChoice::Never);
        let result = match candidate {
            Some((id, function)) => {
                analysis::validate_function_at_phase(&dcx, module, id, function, module.phase())
            }
            None => analysis::validate_phase(&dcx, module, module.phase()),
        };
        result.map_err(|_| dcx.emitted_diagnostics().unwrap().to_string())
    }

    /// Candidates are untrusted, so every mutation of a lowered fixture function must end as a
    /// parse or validation error or as a run, never as a panic.
    #[test]
    fn candidate_mutations() {
        const MUTATIONS: u64 = 32;
        let dir = ui_codegen_dir().join("mir");
        let mut failures = Vec::new();
        let mut parsed = 0usize;
        for path in fixture_paths(&dir, "mir") {
            #[allow(clippy::disallowed_methods)]
            let raw = std::fs::read_to_string(&path).unwrap();
            let text = raw.lines().filter(|l| !l.starts_with("//@")).collect::<Vec<_>>().join("\n");
            let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            sess.enter(|| {
                let Ok(module) = parse_module(&sess, &text) else { return };
                if module.phase() != MirPhase::Lowered || validate(&module, None).is_err() {
                    return;
                }
                for (id, func) in module.iter_functions() {
                    let text = module.candidate_text(func).to_string();
                    for mutation in 0..MUTATIONS {
                        let seed = mix64(id.index() as u64 * MUTATIONS + mutation)
                            ^ mix64(name.len() as u64);
                        let mutated = mutate(&text, seed);
                        let checked = catch_unwind(AssertUnwindSafe(|| {
                            check_candidate(&sess, &module, id, func, &mutated, seed)
                        }));
                        match checked {
                            Ok(true) => parsed += 1,
                            Ok(false) => {}
                            Err(_) => failures.push(format!("{name}:\n{mutated}")),
                        }
                    }
                }
            });
        }
        assert!(parsed > 0, "no mutated candidate parsed");
        assert!(
            failures.is_empty(),
            "{} mutated candidate(s) panicked:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// Takes `text` through the pass's checks as a candidate for function `id` and runs it when
    /// it validates. Returns whether it parsed.
    fn check_candidate(
        sess: &Session,
        module: &Module,
        id: FunctionId,
        func: &Function,
        text: &str,
        seed: u64,
    ) -> bool {
        let Ok(candidate) = parse_candidate(sess, module, text) else { return false };
        if candidate.params != func.params
            || candidate.return_components() != func.return_components()
        {
            return true;
        }
        let mut body = func.clone();
        body.replace_body(candidate);
        if validate(module, Some((id, &body))).is_err() {
            return true;
        }
        DeadCodeEliminator::new().run_to_fixpoint(&mut body);
        let machine = Machine::with_replacement(module, id, &body);
        for input in 0..4 {
            let args = func
                .params
                .iter()
                .enumerate()
                .map(|(index, _)| U256::from(mix64(seed ^ (input * 16 + index as u64)) % 512))
                .collect::<Vec<_>>();
            let memory = Memory::new(seed ^ input, 4);
            let _ = machine.run(id, &args, memory, Limits { fuel: 10_000, depth: 16 }, &mut ());
        }
        true
    }

    /// Mutates candidate text: deletes, repeats, or swaps lines, or replaces a word with another
    /// word of the text, an operation's name, or a token that is often invalid.
    fn mutate(text: &str, seed: u64) -> String {
        let mut state = seed;
        let mut next = |bound: usize| {
            state = mix64(state);
            (state % bound as u64) as usize
        };
        let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        let words = text.split_whitespace().collect::<Vec<_>>();
        for _ in 0..1 + next(3) {
            let line = 1 + next(lines.len().saturating_sub(2).max(1));
            match next(5) {
                0 => {
                    lines.remove(line);
                }
                1 => lines.insert(line, lines[line].clone()),
                2 => {
                    let other = 1 + next(lines.len().saturating_sub(2).max(1));
                    lines.swap(line, other);
                }
                _ => {
                    let mut tokens = lines[line].split(' ').map(str::to_owned).collect::<Vec<_>>();
                    let token = next(tokens.len());
                    tokens[token] = match next(3) {
                        0 => words[next(words.len())].to_owned(),
                        1 => InstKind::MNEMONICS[next(InstKind::MNEMONICS.len())].to_owned(),
                        _ => [
                            "bb9",
                            "v99",
                            "arg9",
                            "0x10000000000000000000000000000000000000000000000000000000000000000",
                            "i1",
                            "memptr",
                            "[",
                            "]",
                            ",",
                            "=",
                            "@main",
                            "undef",
                            "!metadata()",
                        ][next(13)]
                        .to_owned(),
                    };
                    lines[line] = tokens.join(" ");
                }
            }
        }
        lines.join("\n")
    }
}
