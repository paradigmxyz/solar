//! Lean definitions of the program after every stage of the backend, for `-Zemit-lean-stages`.
//!
//! The kernel-checked translation validation in `solar-tv` proves each stage of one
//! compilation against the next: lowered MIR passes, stack scheduling, EVM IR passes and
//! assembly. Its programs are the compiler's own artifacts, written here as Lean terms of its
//! semantics (`SolarTV.MIR.Lowered.Module` and `SolarTV.EVM.Program.Program`), one file per
//! artifact in a directory per contract:
//!
//! - `Stage0.lean`: the module right after the pass that advanced it to the lowered phase, before
//!   any lowered pass;
//! - `Mir<N>.lean`: the module after each later MIR pass whose output differs from the last
//!   artifact, and before stack scheduling when the backend's own preparation changes it;
//! - `Sched.lean`: the runtime EVM IR as stack scheduling emits it;
//! - `Evm<N>.lean`: the runtime EVM IR after each EVM IR pass, and revert legalization, whose
//!   output differs from the last artifact;
//! - `Bytecode.lean`, `Manifest.lean` and `manifest.json`: the runtime bytecode and what binds the
//!   stages to the source, the compiler and its options, written once the code is assembled
//!   ([`write_manifest`]).
//!
//! An artifact is compared with the last one through its Lean term, so a pass that reports a
//! change but leaves the program as the semantics sees it adds no stage, and one that changes
//! it without reporting it does. The first line of every file names its stage
//! (`-- solar-lean-stage: kind=.. index=.. pass=..`), which the manifest reads.
//!
//! The translation fails closed: an instruction, terminator, type or shape of function the Lean
//! semantics cannot represent is an error naming it, never a skipped instruction. MIR keeps the
//! compiler's numbers for blocks, values and functions; EVM IR keeps its block numbers, its
//! layout order, and writes `push bbN; jumpi` and `push bbN; jump` as the semantics' fused
//! `jumpi N` and `jump N`, a label pushed as a value (an internal call's return address) as
//! `pushLabel N`, a jump to the stack's top (the callee's return) as `jumpDyn` and a
//! conditional jump to it as `jumpiDyn`, and a jump through a table of blocks by the index on the
//! stack (`indexed_jump`) as `jumpTable`. Functions the runtime entry cannot reach, such as a
//! constructor, are written without blocks, which the semantics never enters. A parameter can
//! have several values: the function binds all of them, in order, and every call repeats the
//! argument once for each.
//!
//! A call of a function that returns several words binds each further result to the value that
//! reads it, as stack scheduling returns them on the stack and elides MIR's reads of the
//! multi-return buffer (`call_projection`), so those reads are left out of the block. A call
//! whose results escape that protocol is an error. Where codegen also publishes the results to
//! the callee's return buffer, the call names the buffer, written once codegen fixes it.
//!
//! Program data after the code, such as the creation code of a contract the runtime code
//! creates, is copied with `CODECOPY` from the code, its offset and sizes written as
//! `⟪dataoffset K O⟫` and `⟪datasize K A L⟫` until assembly fixes them ([`write_manifest`]), and
//! the bytecode the stages are checked against is the code before it. `K` names the data by what
//! it holds ([`data_key`]), not by its number, since MIR, the EVM IR passes and final assembly
//! each number data their own way, and final assembly shares equal creation code and bytes
//! another entry contains. The manifest lists where each entry of it is and its size.
//! `CREATE` and `CREATE2` take the new address from the oracle.
//!
//! An immutable is a value fixed at deployment: the deployment code writes it over each of its
//! placeholders, a `PUSH<N>` of zeros, before the runtime code runs. EVM IR's placeholder push
//! reads the word of those `N` bytes, `.immutable id N .word`, the low `N` bytes of the
//! immutable's deployment word in the semantics. MIR's `loadimmutable` reads the same placeholder
//! and decodes it as codegen does after the push: as it is (a full word or a zero-extended
//! value), sign-extended from its top byte (`.signExtend`), or shifted to the left of the word
//! (`.shiftLeft`, fixed bytes). The manifest lists each placeholder: its immutable, the offset of
//! its bytes in the runtime bytecode and their count, all zero there; the bytecode a deployment
//! runs is the manifest's linked at them.
//!
//! The words the backend places in memory without MIR showing them are written into the MIR
//! stages once codegen fixes them (`backend_words`), as the MIR interpreter reads them from
//! the backend memory records: the free memory pointer a runtime entry stores when it starts (a
//! function's `entryHeap`), the heap floor each `heap_floor` of a function reads, where each
//! `alloc` lives, and the return buffer a call returning several words publishes to, if any.
//! Until then a stage file holds a placeholder for each, `⟪entry N⟫`, `⟪floor N⟫`, `⟪alloc N I⟫`
//! and `⟪call N I⟫` for function `N` and instruction `I`, which no Lean term parses, and the
//! manifest refuses a stage that still has one.
//!
//! Exporting only reads the module and the program, so the generated code is the same with and
//! without it.

use crate::backend::evm::{
    call_projection,
    schedule_map::{Location, ScheduleMap},
};
use alloy_primitives::U256;
use sha2::{Digest, Sha256};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use solar_evmir::{
    ir::{self, PushValue, TerminatorKind},
    op::{self, StackOp},
};
use solar_interface::{Result, diagnostics::ErrorGuaranteed};
use solar_mir::{
    AllocationPlacement, AllocationSemantics, AssumePredicate, BlockId, Callee, Data, DataBytes,
    DataId, DataRef, DataSize, Function, FunctionId, Immediate, ImmutableEncoding, ImmutableId,
    InstId, InstKind, MirPhase, MirType, Module, PtrLocation, Semantics, Terminator, Value,
    ValueId, immutable::immutable_push_type_size,
};
use solar_sema::Gcx;
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

/// The prefix of the first line of every stage file.
const HEADER: &str = "-- solar-lean-stage:";

/// The files of a contract's stage directory that [`clear`] removes before a compilation.
fn is_stage_file(name: &str) -> bool {
    let numbered = |prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".lean"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    matches!(
        name,
        "Stage0.lean"
            | "Sched.lean"
            | "Bytecode.lean"
            | "Manifest.lean"
            | "manifest.json"
            | "Schedule.json"
    ) || numbered("Mir")
        || numbered("Evm")
}

/// The directory of a contract's stages, when `-Zemit-lean-stages` is set.
fn contract_dir(gcx: Gcx<'_>, contract: &str) -> Option<PathBuf> {
    gcx.sess.opts.unstable.emit_lean_stages.as_deref().map(|dir| dir.join(contract))
}

/// The contract whose runtime code a scheduled EVM IR program is, from its name.
fn runtime_contract(program: &ir::Module) -> Option<String> {
    program.name.as_str().strip_suffix("_runtime").map(ToOwned::to_owned)
}

fn error(gcx: Gcx<'_>, message: impl std::fmt::Display) -> ErrorGuaranteed {
    gcx.dcx().err(format!("-Zemit-lean-stages: {message}")).emit()
}

/// Writes the module after a MIR pass: the first lowered module when `pass` advanced it to the
/// lowered phase from `before`, and a later one when it differs from the last artifact.
pub(crate) fn mir_pass(gcx: Gcx<'_>, pass: &str, before: MirPhase, module: &Module) -> Result {
    let contract = module.name.as_str();
    let Some(dir) = contract_dir(gcx, contract) else { return Ok(()) };
    if module.phase() != MirPhase::Lowered {
        return Ok(());
    }
    let shifts = gcx.sess.opts.evm_version.has_bitwise_shifting();
    let body = mir_module(module, shifts).map_err(|message| {
        error(gcx, format_args!("after MIR pass `{pass}` on `{contract}`: {message}"))
    })?;
    if before != MirPhase::Lowered {
        clear(gcx, &dir)?;
        return write_stage(gcx, &dir, Stage::mir(0, pass), contract, &body);
    }
    match next_index(gcx, &dir, "Mir", "Stage0.lean", &body)? {
        Some(index) => write_stage(gcx, &dir, Stage::mir(index, pass), contract, &body),
        None => Ok(()),
    }
}

/// Writes the module stack scheduling compiles when the backend's preparation changed it.
pub(crate) fn mir_prepared(gcx: Gcx<'_>, module: &Module) -> Result {
    mir_pass(gcx, "backend-prepare", MirPhase::Lowered, module)
}

/// How codegen passes the further results of internal calls that return several words.
pub(crate) struct MultiReturns<'a> {
    /// The address of the buffer each function's results are published to.
    pub(crate) buffers: &'a FxHashMap<FunctionId, U256>,
    /// The calls that bind their further results to the words on the stack.
    pub(crate) stack: &'a FxHashSet<(FunctionId, InstId)>,
    /// The calls that publish their further results to the buffer of their callee.
    pub(crate) published: &'a FxHashMap<(FunctionId, InstId), FunctionId>,
}

/// Writes the words codegen placed into the MIR stages of `module`: the free memory pointer each
/// runtime entry stores when it starts, the heap floor of each function, and the buffer that
/// calls returning several words publish their further results to, replacing their placeholders.
/// A `heap_floor` of a function codegen placed no floor for, or a call returning several words
/// that codegen neither binds on the stack nor publishes, is an error.
pub(crate) fn backend_words(
    gcx: Gcx<'_>,
    module: &Module,
    entry_heaps: &FxHashMap<FunctionId, U256>,
    heap_floors: &FxHashMap<FunctionId, U256>,
    allocations: &FxHashMap<(FunctionId, InstId), AllocationPlacement>,
    returns: &MultiReturns<'_>,
) -> Result {
    let contract = module.name.as_str();
    let Some(dir) = contract_dir(gcx, contract) else { return Ok(()) };
    if !dir.join("Stage0.lean").exists() {
        return Ok(());
    }
    let mut files = vec!["Stage0.lean".to_string()];
    files.extend(numbered(&dir, "Mir").into_iter().map(|n| format!("Mir{n}.lean")));
    for file in files {
        let path = dir.join(&file);
        // The stage files are this exporter's output, not sources the source map loads.
        #[allow(clippy::disallowed_methods)]
        let text = fs::read_to_string(&path).map_err(|e| {
            error(gcx, format_args!("cannot read the stage `{}`: {e}", path.display()))
        })?;
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find('⟪') {
            out.push_str(&rest[..start]);
            let Some(len) = rest[start..].find('⟫') else { break };
            let marker = &rest[start + '⟪'.len_utf8()..start + len];
            rest = &rest[start + len + '⟫'.len_utf8()..];
            if marker.starts_with("dataoffset ") || marker.starts_with("datasize ") {
                // NOTE: assembly fixes these after codegen (`write_manifest`).
                let _ = write!(out, "⟪{marker}⟫");
                continue;
            }
            let malformed = || error(gcx, format_args!("`{file}` has a malformed placeholder"));
            let mut fields = marker.split(' ');
            let (Some(kind), Some(func)) = (
                fields.next(),
                fields
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|&n| n < module.functions.len()),
            ) else {
                return Err(malformed());
            };
            let func = FunctionId::from_usize(func);
            match kind {
                "alloc" => {
                    let Some(inst) = fields.next().and_then(|n| n.parse::<usize>().ok()) else {
                        return Err(malformed());
                    };
                    let placement = allocations.get(&(func, InstId::from_usize(inst)));
                    let Some(placement) = placement else {
                        return Err(error(
                            gcx,
                            format_args!(
                                "in `{file}` of `{contract}`: codegen placed no allocation \
                                 for instruction {inst} of `@{}`",
                                module.function(func).name
                            ),
                        ));
                    };
                    match placement {
                        AllocationPlacement::Static(address) => {
                            let _ = write!(out, "(.static {})", word(address));
                        }
                        AllocationPlacement::Dynamic(size) => {
                            let _ = write!(out, "(.dynamic {})", word(size));
                        }
                    }
                }
                "call" => {
                    let Some(inst) = fields.next().and_then(|n| n.parse::<usize>().ok()) else {
                        return Err(malformed());
                    };
                    let site = (func, InstId::from_usize(inst));
                    if let Some(callee) = returns.published.get(&site) {
                        let Some(buffer) = returns.buffers.get(callee) else {
                            return Err(error(
                                gcx,
                                format_args!(
                                    "in `{file}` of `{contract}`: codegen placed no return \
                                     buffer for `@{}`",
                                    module.function(*callee).name
                                ),
                            ));
                        };
                        // the further results stored to the buffer after the call returns
                        let _ = write!(out, " (some {})", word(buffer));
                    } else if !returns.stack.contains(&site) {
                        return Err(error(
                            gcx,
                            format_args!(
                                "in `{file}` of `{contract}`: codegen passes the results of \
                                 instruction {inst} of `@{}` neither on the stack nor through \
                                 its return buffer",
                                module.function(func).name
                            ),
                        ));
                    }
                }
                "entry" => {
                    if let Some(heap) = entry_heaps.get(&func) {
                        let _ = write!(out, ",\n      entryHeap := some {}", word(heap));
                    }
                }
                "floor" => {
                    let Some(floor) = heap_floors.get(&func) else {
                        return Err(error(
                            gcx,
                            format_args!(
                                "in `{file}` of `{contract}`: codegen placed no heap floor for \
                                 `@{}`",
                                module.function(func).name
                            ),
                        ));
                    };
                    out.push_str(&word(floor));
                }
                _ => return Err(malformed()),
            }
        }
        out.push_str(rest);
        write(gcx, &path, out.as_bytes())?;
    }
    Ok(())
}

/// Writes the runtime program stack scheduling emitted.
pub(crate) fn scheduled(gcx: Gcx<'_>, program: &ir::Module) -> Result {
    let Some(contract) = runtime_contract(program) else { return Ok(()) };
    let Some(dir) = contract_dir(gcx, &contract) else { return Ok(()) };
    let body = evm_program(program).map_err(|message| {
        error(gcx, format_args!("scheduled runtime EVM IR of `{contract}`: {message}"))
    })?;
    write_stage(gcx, &dir, Stage::scheduled(), &contract, &body)
}

/// Writes where stack scheduling put each MIR value of a contract's runtime, `Schedule.json`:
/// for every function, its entry block in the scheduled EVM IR and whether it is entered with a
/// return address, and for every MIR block, where its code starts in the scheduled EVM IR (an
/// EVM IR block number and an instruction index, before the exporter fuses a pushed label
/// with its jump) and what each word of its entry stack holds, from the bottom of the function's
/// words to the top: `"vN"` for MIR value `N`, `"ret"` for the return address and `"junk"`
/// for a word no later code reads. The cut-point proofs of looping stages read the relation
/// between a MIR loop header and its scheduled block from it; it is the scheduler's own record
/// (`ScheduleMap`), written after the fact, so it changes nothing the scheduler emits, and a
/// wrong entry only fails a proof.
pub(crate) fn schedule_map(gcx: Gcx<'_>, program: &ir::Module, map: &ScheduleMap) -> Result {
    let Some(contract) = runtime_contract(program) else { return Ok(()) };
    let Some(dir) = contract_dir(gcx, &contract) else { return Ok(()) };
    let location = |l: &Location| match l {
        Location::Value(v) => format!("\"v{}\"", v.index()),
        Location::Ret => "\"ret\"".to_string(),
        Location::Junk => "\"junk\"".to_string(),
    };
    let mut out = String::from("{\n  \"functions\": [");
    for (n, f) in map.functions.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        let entry = f.entry.map_or("null".to_string(), |b| b.index().to_string());
        let unsupported = f.unsupported.map_or("null".to_string(), json_string);
        let _ = write!(
            out,
            "\n    {{\"function\": {}, \"entry\": {entry}, \"returns\": {}, \"rebuilt\": {}, \
             \"unsupported\": {unsupported}, \"blocks\": [",
            f.function.index(),
            f.returns,
            f.rebuilt.is_some(),
        );
        let mut first = true;
        for (block, info) in f.blocks.iter_enumerated() {
            let Some(info) = info else { continue };
            if !first {
                out.push(',');
            }
            first = false;
            let start =
                info.start.map_or("null".to_string(), |(b, i)| format!("[{}, {i}]", b.index()));
            let continues = info.continues.map_or("null".to_string(), |b| b.index().to_string());
            let layout: Vec<String> = info.layout.iter().map(location).collect();
            let _ = write!(
                out,
                "\n      {{\"block\": {}, \"start\": {start}, \"layout\": [{}], \"floating\": {}, \
                 \"continues\": {continues}}}",
                block.index(),
                layout.join(", "),
                info.floating,
            );
        }
        out.push_str("\n    ]}");
    }
    out.push_str("\n  ]\n}\n");
    write(gcx, &dir.join("Schedule.json"), out.as_bytes())
}

/// A JSON string.
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes the runtime program after an EVM IR pass when it differs from the last artifact.
pub(crate) fn evm_pass(gcx: Gcx<'_>, pass: &str, program: &ir::Module) -> Result {
    let Some(contract) = runtime_contract(program) else { return Ok(()) };
    let Some(dir) = contract_dir(gcx, &contract) else { return Ok(()) };
    let body = evm_program(program).map_err(|message| {
        error(gcx, format_args!("after EVM IR pass `{pass}` on `{contract}`: {message}"))
    })?;
    match next_index(gcx, &dir, "Evm", "Sched.lean", &body)? {
        Some(index) => write_stage(gcx, &dir, Stage::evm(index, pass), &contract, &body),
        None => Ok(()),
    }
}

/// One stage file.
struct Stage<'a> {
    kind: &'static str,
    index: usize,
    pass: &'a str,
    file: String,
    def: String,
    ty: &'static str,
    import: &'static str,
    about: String,
}

impl<'a> Stage<'a> {
    fn mir(index: usize, pass: &'a str) -> Self {
        let (file, def, about) = if index == 0 {
            (
                "Stage0".to_string(),
                "stage0".to_string(),
                format!("The first lowered MIR module, after `{pass}` advanced the phase."),
            )
        } else {
            (
                format!("Mir{index}"),
                format!("mir{index}"),
                format!("The lowered MIR module after `{pass}`."),
            )
        };
        Self {
            kind: "mir",
            index,
            pass,
            file,
            def,
            ty: "MIR.Lowered.Module",
            import: "SolarTV.MIR.Program",
            about,
        }
    }

    fn scheduled() -> Self {
        Self {
            kind: "sched",
            index: 0,
            pass: "stack-schedule",
            file: "Sched".to_string(),
            def: "sched".to_string(),
            ty: "EVM.Program.Program",
            import: "SolarTV.EVMIR.Program",
            about: "The runtime EVM IR as stack scheduling emits it.".to_string(),
        }
    }

    fn evm(index: usize, pass: &'a str) -> Self {
        Self {
            kind: "evm",
            index,
            pass,
            file: format!("Evm{index}"),
            def: format!("evm{index}"),
            ty: "EVM.Program.Program",
            import: "SolarTV.EVMIR.Program",
            about: format!("The runtime EVM IR after `{pass}`."),
        }
    }
}

/// The Lean namespace of a contract's definitions.
fn namespace(contract: &str) -> String {
    let plain = contract.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && contract.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if plain {
        format!("SolarTV.Export.{contract}")
    } else {
        format!("SolarTV.Export.«{contract}»")
    }
}

fn write_stage(gcx: Gcx<'_>, dir: &Path, stage: Stage<'_>, contract: &str, body: &str) -> Result {
    let ns = namespace(contract);
    let text = format!(
        "{HEADER} kind={kind} index={index} pass={pass}\n\
         import {import}\n\
         \n\
         /-! Generated by `solar -Zemit-lean-stages` for `{contract}`; do not edit. -/\n\
         \n\
         namespace {ns}\n\
         \n\
         /-- {about} -/\n\
         def {def} : {ty} :=\n\
         {body}\n\
         \n\
         end {ns}\n",
        kind = stage.kind,
        index = stage.index,
        pass = stage.pass,
        import = stage.import,
        about = stage.about,
        def = stage.def,
        ty = stage.ty,
    );
    write(gcx, &dir.join(format!("{}.lean", stage.file)), text.as_bytes())
}

fn write(gcx: Gcx<'_>, path: &Path, bytes: &[u8]) -> Result {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| error(gcx, format_args!("cannot create `{}`: {e}", parent.display())))?;
    }
    fs::write(path, bytes)
        .map_err(|e| error(gcx, format_args!("cannot write `{}`: {e}", path.display())))
}

/// Removes the stage files an earlier compilation left in a contract's directory.
fn clear(gcx: Gcx<'_>, dir: &Path) -> Result {
    let Ok(entries) = fs::read_dir(dir) else { return Ok(()) };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_str().is_some_and(is_stage_file) {
            fs::remove_file(entry.path()).map_err(|e| {
                error(gcx, format_args!("cannot remove `{}`: {e}", entry.path().display()))
            })?;
        }
    }
    Ok(())
}

/// The numbers of the files `<prefix><N>.lean` of a directory, in order.
fn numbered(dir: &Path, prefix: &str) -> Vec<usize> {
    let mut found: Vec<usize> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.strip_prefix(prefix)?.strip_suffix(".lean")?.parse().ok()
        })
        .collect();
    found.sort_unstable();
    found
}

/// The body of a stage file: its definition's term.
fn body_of(text: &str) -> Option<&str> {
    let (_, rest) = text.split_once(" :=\n")?;
    let (body, _) = rest.rsplit_once("\n\nend SolarTV.Export.")?;
    Some(body)
}

/// The number of the next artifact of a family when `body` differs from its last one, which is
/// its highest-numbered file or else `first`.
fn next_index(
    gcx: Gcx<'_>,
    dir: &Path,
    prefix: &str,
    first: &str,
    body: &str,
) -> Result<Option<usize>> {
    let numbers = numbered(dir, prefix);
    let last = match numbers.last() {
        Some(n) => dir.join(format!("{prefix}{n}.lean")),
        None => dir.join(first),
    };
    // The stage files are this exporter's output, not sources the source map loads.
    #[allow(clippy::disallowed_methods)]
    let text = fs::read_to_string(&last).map_err(|e| {
        error(gcx, format_args!("cannot read the last stage `{}`: {e}", last.display()))
    })?;
    let Some(previous) = body_of(&text) else {
        return Err(error(gcx, format_args!("`{}` is not a stage file", last.display())));
    };
    Ok((previous != body).then(|| numbers.last().map_or(1, |n| n + 1)))
}

// ---------------------------------------------------------------------------------------------
// MIR.

/// The Lean `EVM.Op`s lowered MIR runs as pure word operations, by mnemonic, with their arity
/// and whether their result may be an `i1`.
const PURE_OPS: &[(&str, usize, bool)] = &[
    ("add", 2, false),
    ("sub", 2, false),
    ("mul", 2, false),
    ("div", 2, false),
    ("sdiv", 2, false),
    ("mod", 2, false),
    ("smod", 2, false),
    ("addmod", 3, false),
    ("mulmod", 3, false),
    ("exp", 2, false),
    ("signextend", 2, false),
    ("and", 2, true),
    ("or", 2, true),
    ("xor", 2, true),
    ("not", 1, false),
    ("byte", 2, false),
    ("shl", 2, false),
    ("shr", 2, false),
    ("sar", 2, false),
    ("lt", 2, true),
    ("gt", 2, true),
    ("slt", 2, true),
    ("sgt", 2, true),
    ("eq", 2, true),
    ("iszero", 1, true),
];

/// The Lean `Effect`s of opcodes by mnemonic: the effect, its arity, whether it has a result,
/// and whether that result may be an `i160`.
const EFFECTS: &[(&str, usize, bool, bool)] = &[
    ("mload", 1, true, false),
    ("mstore", 2, false, false),
    ("mstore8", 2, false, false),
    ("msize", 0, true, false),
    ("sload", 1, true, false),
    ("sstore", 2, false, false),
    ("tload", 1, true, false),
    ("tstore", 2, false, false),
    ("calldataload", 1, true, false),
    ("calldatasize", 0, true, false),
    ("callvalue", 0, true, false),
    ("caller", 0, true, true),
    ("address", 0, true, true),
    ("returndatasize", 0, true, false),
    ("keccak256", 2, true, false),
    ("calldatacopy", 3, false, false),
    ("mcopy", 3, false, false),
    ("returndatacopy", 3, false, false),
    ("gas", 0, true, false),
    ("codecopy", 3, false, false),
    ("create", 3, true, true),
    ("create2", 4, true, true),
];

/// The external calls by mnemonic, as the Lean `Effect.call` of the `CallKind` of the same
/// name, with their arity. Each returns its success flag, which may be an `i1`.
const CALLS: &[(&str, usize)] =
    &[("call", 7), ("callcode", 7), ("delegatecall", 6), ("staticcall", 6)];

/// The environment reads the Lean semantics takes from its oracle (`.env`), as functions of the
/// world and their operands, by mnemonic: their arity and whether their result may be an
/// `i160`. Each is fixed for the frame but for what its calls change.
const ENV_READS: &[(&str, usize, bool)] = &[
    ("origin", 0, true),
    ("gasprice", 0, false),
    ("coinbase", 0, true),
    ("timestamp", 0, false),
    ("number", 0, false),
    ("prevrandao", 0, false),
    ("gaslimit", 0, false),
    ("chainid", 0, false),
    ("basefee", 0, false),
    ("blobbasefee", 0, false),
    ("blockhash", 1, false),
    ("blobhash", 1, false),
    ("balance", 1, false),
    ("selfbalance", 0, false),
    ("extcodesize", 1, false),
    ("extcodehash", 1, false),
    ("codesize", 0, false),
];

/// A word as a Lean numeral.
fn word(value: &U256) -> String {
    if *value < U256::from(0x10000) { value.to_string() } else { format!("{value:#x}") }
}

/// The width of immutable `id`'s placeholder and how `loadimmutable` decodes the word it pushes,
/// as codegen loads it (`emit_load_immutable`): a full word or a zero-extended value as it is, a
/// narrower signed one sign-extended from its top byte, and narrower fixed bytes shifted to the
/// left of the word.
fn immutable_read(module: &Module, id: ImmutableId, shifts: bool) -> Result<String, String> {
    let encoding = module
        .immutable_type(id)
        .immutable_encoding()
        .ok_or_else(|| format!("immutable {} has no word encoding", id.index()))?;
    let bytes = immutable_push_type_size(encoding, module.optimization, shifts).bytes();
    if !(1..=32).contains(&bytes) {
        return Err(format!("an immutable placeholder of {bytes} bytes has no Lean semantics"));
    }
    let read = match encoding {
        // push N-1; signextend
        ImmutableEncoding::Signed(_) if bytes < 32 => format!("(.signExtend {})", bytes - 1),
        // push 8 * (32 - N); shl
        ImmutableEncoding::LeftAligned(_) if bytes < 32 => {
            format!("(.shiftLeft {})", (32 - u32::from(bytes)) * 8)
        }
        _ => ".word".to_string(),
    };
    Ok(format!("{bytes} {read}"))
}

/// The Lean term of lowered MIR `module`, compiled for an EVM version with bitwise shifts when
/// `shifts` is set.
fn mir_module(module: &Module, shifts: bool) -> Result<String, String> {
    let entry = module.dispatch_entry().ok_or("the module has no runtime entry")?;
    if !module.function(entry).params.is_empty() {
        return Err(format!("the runtime entry `@{}` has parameters", module.function(entry).name));
    }
    let reachable = module.runtime_reachable_functions();
    let mut functions = Vec::new();
    for (id, func) in module.functions.iter_enumerated() {
        if !reachable.contains(id) {
            functions.push(format!(
                "    -- @{}: not reachable from the runtime entry, so it has no blocks here\n    \
                 {{ blocks := [] }}",
                func.name
            ));
            continue;
        }
        let text = mir_function(module, id, func, shifts)
            .map_err(|message| format!("in `@{}`: {message}", func.name))?;
        functions.push(format!("    -- @{}\n{text}", func.name));
    }
    Ok(format!(
        "  {{ entry := {},\n    functions := [\n{}] }}",
        entry.index(),
        functions.join(",\n")
    ))
}

fn mir_function(
    module: &Module,
    id: FunctionId,
    func: &Function,
    shifts: bool,
) -> Result<String, String> {
    let fn_id = id;
    let params = mir_params(func).into_iter().flatten().map(|v| v.to_string()).collect::<Vec<_>>();
    let (projections, elided) = multi_returns(module, func)?;
    let mut blocks = Vec::new();
    for (id, block) in func.blocks.iter_enumerated() {
        let at = |message: String| format!("bb{}: {message}", id.index());
        let mut phis = Vec::new();
        let mut insts = Vec::new();
        for &inst in &block.instructions {
            if elided.contains(&inst) {
                // NOTE: the call binds the result this reads of the multi-return buffer, as
                // stack scheduling binds it to the word the callee leaves on the stack.
                continue;
            }
            let instruction = func.inst(inst);
            let kind = &instruction.kind;
            let unsupported =
                || at(format!("instruction `{}` has no Lean semantics", kind.mnemonic()));
            let result = func.inst_result_value(inst);
            if let InstKind::Assume { predicate, lhs, rhs, .. } = *kind {
                // NOTE: a fact a pass relied on is checked, not trusted: where it fails the Lean
                // run has no end, so a stage whose input reaches a failing fact is not proved.
                let (negated, op) = match predicate {
                    AssumePredicate::Eq => (false, "eq"),
                    AssumePredicate::Ne => (true, "eq"),
                    AssumePredicate::Ult => (false, "lt"),
                    AssumePredicate::Ule => (true, "gt"),
                };
                let lhs = mir_operand(func, lhs).map_err(at)?;
                let rhs = mir_operand(func, rhs).map_err(at)?;
                // assume pred lhs, rhs
                insts.push(format!(".assume {negated} .{op} [{lhs}, {rhs}]"));
                continue;
            }
            if let InstKind::LoadImmutable(immutable) = *kind {
                let dst = mir_result(instruction.result_ty, result, true, true)
                    .map_err(|m| at(format!("`loadimmutable`: {m}")))?;
                let read = immutable_read(module, immutable, shifts)
                    .map_err(|m| at(format!("`loadimmutable`: {m}")))?;
                // v = loadimmutable N: its placeholder's word, decoded as codegen decodes it
                insts.push(format!(
                    ".effect (some {dst}) (.immutable {} {read}) []",
                    immutable.index()
                ));
                continue;
            }
            if let InstKind::DataSize(size) = *kind {
                let dst = mir_result(instruction.result_ty, result, false, false)
                    .map_err(|m| at(format!("`datasize`: {m}")))?;
                // v = datasize D: the size assembly fixes
                insts.push(format!(".word {dst} (.const {})", data_size(&module.data, size)));
                continue;
            }
            match kind.semantics() {
                Some(Semantics::Allocate(_, semantics)) => {
                    if semantics != AllocationSemantics::INTERNAL {
                        return Err(at(format!(
                            "`alloc` with {semantics:?} has no Lean semantics"
                        )));
                    }
                    let dst = mir_result(instruction.result_ty, result, false, false)
                        .map_err(|m| at(format!("`alloc`: {m}")))?;
                    // v = alloc size: where codegen placed it
                    insts.push(format!(".alloc {dst} ⟪alloc {} {}⟫", fn_id.index(), inst.index()));
                }
                Some(Semantics::HeapFloor) => {
                    let dst = mir_result(instruction.result_ty, result, false, false)
                        .map_err(|m| at(format!("`heap_floor`: {m}")))?;
                    // v = heap_floor: the floor codegen places for the function
                    insts.push(format!(".heapFloor {dst} ⟪floor {}⟫", fn_id.index()));
                }
                Some(Semantics::Phi(incoming)) => {
                    if !insts.is_empty() {
                        return Err(at("a phi after another instruction".to_string()));
                    }
                    if id == BlockId::ENTRY {
                        return Err(at("a phi in the entry block".to_string()));
                    }
                    // NOTE: a phi copies its word, whatever the width of its integer type.
                    let ty = instruction
                        .result_ty
                        .map(|ty| if matches!(ty, MirType::Int(_)) { MirType::I256 } else { ty });
                    let dst = mir_result(ty, result, false, false)
                        .map_err(|m| at(format!("`phi`: {m}")))?;
                    let arms = incoming
                        .iter()
                        .map(|&(block, value)| {
                            Ok(format!("({}, {})", block.index(), mir_operand(func, value)?))
                        })
                        .collect::<Result<Vec<_>, String>>()
                        .map_err(at)?;
                    phis.push(format!("({dst}, [{}])", arms.join(", ")));
                }
                Some(Semantics::DataCopy(data, dest, size)) => {
                    let dest = mir_operand(func, dest).map_err(at)?;
                    let size = mir_operand(func, size).map_err(at)?;
                    // datacopy D, dest, size: codecopy dest, dataoffset D, size
                    insts.push(format!(
                        ".effect none .codecopy [{dest}, .const {}, {size}]",
                        data_offset(&module.data, data)
                    ));
                }
                Some(Semantics::NotEqual(a, b)) => {
                    let dst = mir_result(instruction.result_ty, result, true, false)
                        .map_err(|m| at(format!("`ne`: {m}")))?;
                    let a = mir_operand(func, a).map_err(at)?;
                    let b = mir_operand(func, b).map_err(at)?;
                    insts.push(format!(".ne {dst} ({a}) ({b})"));
                }
                Some(Semantics::Word(value)) => {
                    let dst = mir_result(instruction.result_ty, result, true, true)
                        .map_err(|m| at(format!("`{}`: {m}", kind.mnemonic())))?;
                    let value = mir_operand(func, value).map_err(at)?;
                    // v = zext a, or a pointer cast: the word of a, whose bits above its width
                    // are zero
                    insts.push(format!(".word {dst} ({value})"));
                }
                Some(Semantics::LowBits(value, bits)) => {
                    let dst = mir_result(instruction.result_ty, result, bits == 1, bits == 160)
                        .map_err(|m| at(format!("`{}`: {m}", kind.mnemonic())))?;
                    let value = mir_operand(func, value).map_err(at)?;
                    if bits >= 256 {
                        // v = ptrtoint a to i256: the word of a
                        insts.push(format!(".word {dst} ({value})"));
                    } else {
                        // v = trunc a to iN: the low N bits of a
                        insts.push(format!(".low {dst} {bits} ({value})"));
                    }
                }
                Some(Semantics::Select(condition, yes, no)) => {
                    let dst = mir_result(instruction.result_ty, result, true, true)
                        .map_err(|m| at(format!("`select`: {m}")))?;
                    let condition = mir_operand(func, condition).map_err(at)?;
                    let yes = mir_operand(func, yes).map_err(at)?;
                    let no = mir_operand(func, no).map_err(at)?;
                    // v = select c, a, b
                    insts.push(format!(".select {dst} ({condition}) ({yes}) ({no})"));
                }
                Some(Semantics::Call(Callee::Function(callee), args)) => {
                    let dst = match result {
                        Some(_) => format!(
                            "(some {})",
                            mir_result(instruction.result_ty, result, true, true)
                                .map_err(|m| at(format!("`icall`: {m}")))?
                        ),
                        None => "none".to_string(),
                    };
                    let args = mir_call_args(module, func, *callee, args).map_err(at)?;
                    match projections.get(&inst) {
                        // dst, extra1, .., extraN = icall @callee, args: published to the
                        // callee's return buffer where codegen publishes them
                        Some(extras) => insts.push(format!(
                            ".call {dst} {} [{}] [{}]⟪call {} {}⟫",
                            callee.index(),
                            args.join(", "),
                            extras
                                .iter()
                                .map(|extra| match extra {
                                    Some(value) => format!("some {}", value.index()),
                                    None => "none".to_string(),
                                })
                                .collect::<Vec<_>>()
                                .join(", "),
                            fn_id.index(),
                            inst.index()
                        )),
                        // dst = icall @callee, args
                        None => insts.push(format!(
                            ".call {dst} {} [{}]",
                            callee.index(),
                            args.join(", ")
                        )),
                    }
                }
                Some(Semantics::Opcode(name, operands)) => {
                    let args = operands
                        .iter()
                        .map(|&v| mir_operand(func, v))
                        .collect::<Result<Vec<_>, String>>()
                        .map_err(at)?;
                    if let Some(&(_, arity, i1)) = PURE_OPS.iter().find(|op| op.0 == name) {
                        if args.len() != arity {
                            return Err(unsupported());
                        }
                        let dst = mir_result(instruction.result_ty, result, i1, false)
                            .map_err(|m| at(format!("`{name}`: {m}")))?;
                        insts.push(format!(".pure {dst} .{name} [{}]", args.join(", ")));
                    } else if let Some(&(_, arity, has_result, i160)) =
                        EFFECTS.iter().find(|effect| effect.0 == name)
                    {
                        if args.len() != arity || result.is_some() != has_result {
                            return Err(unsupported());
                        }
                        let dst = if has_result {
                            let dst = mir_result(instruction.result_ty, result, false, i160)
                                .map_err(|m| at(format!("`{name}`: {m}")))?;
                            format!("(some {dst})")
                        } else {
                            "none".to_string()
                        };
                        insts.push(format!(".effect {dst} .{name} [{}]", args.join(", ")));
                    } else if let Some(&(_, arity)) = CALLS.iter().find(|call| call.0 == name) {
                        if args.len() != arity {
                            return Err(unsupported());
                        }
                        let dst = mir_result(instruction.result_ty, result, true, false)
                            .map_err(|m| at(format!("`{name}`: {m}")))?;
                        // v = name gas, address, .., out_size: the oracle's success flag
                        insts.push(format!(
                            ".effect (some {dst}) (.call .{name}) [{}]",
                            args.join(", ")
                        ));
                    } else if let Some(&(_, arity, i160)) =
                        ENV_READS.iter().find(|read| read.0 == name)
                    {
                        if args.len() != arity {
                            return Err(unsupported());
                        }
                        let dst = mir_result(instruction.result_ty, result, false, i160)
                            .map_err(|m| at(format!("`{name}`: {m}")))?;
                        // v = name args, read from the oracle
                        insts.push(format!(
                            ".effect (some {dst}) (.env \"{name}\") [{}]",
                            args.join(", ")
                        ));
                    } else if let Some(topics) = log_topics(name) {
                        if args.len() != topics + 2 || result.is_some() {
                            return Err(unsupported());
                        }
                        // logN offset, size, topics
                        insts.push(format!(".effect none (.log {topics}) [{}]", args.join(", ")));
                    } else {
                        return Err(unsupported());
                    }
                }
                _ => return Err(unsupported()),
            }
        }
        let term = mir_terminator(module, func, block.terminator.as_ref()).map_err(at)?;
        blocks.push(format!(
            "      -- bb{}\n      ⟨[{}],\n        [{}],\n        {term}⟩",
            id.index(),
            phis.join(", "),
            insts.join(",\n         ")
        ));
    }
    // NOTE: `⟪entry N⟫` becomes the entry heap codegen stores when the function starts as a
    // runtime entry, or nothing when it stores none (`backend_words`).
    Ok(format!(
        "    {{ params := [{}],\n      blocks := [\n{}]⟪entry {}⟫ }}",
        params.join(", "),
        blocks.join(",\n"),
        fn_id.index()
    ))
}

/// The internal calls of `func` that return several words, with the value each further result
/// binds, if any, and the reads of the multi-return buffer that bind them, which stack scheduling
/// elides (`call_projection`). A call whose results escape that protocol is an error.
#[expect(clippy::type_complexity)]
fn multi_returns(
    module: &Module,
    func: &Function,
) -> Result<(FxHashMap<InstId, Vec<Option<ValueId>>>, FxHashSet<InstId>), String> {
    let mut projections = FxHashMap::default();
    let mut elided = FxHashSet::default();
    let mut use_counts = None;
    for (block_id, block) in func.blocks.iter_enumerated() {
        for (index, &inst) in block.instructions.iter().enumerate() {
            let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(inst).kind
            else {
                continue;
            };
            let arity = module.function(callee).return_components().len();
            if arity <= 1 {
                continue;
            }
            let use_counts = use_counts.get_or_insert_with(|| {
                let mut counts = index_vec![0u32; func.num_values()];
                for block in &func.blocks {
                    for &inst in &block.instructions {
                        func.inst(inst).kind.visit_operands(|operand| counts[operand] += 1);
                    }
                    if let Some(term) = &block.terminator {
                        term.visit_operands(|operand| counts[operand] += 1);
                    }
                }
                counts
            });
            let Some(projection) = call_projection(func, block_id, index, arity)
                .filter(|projection| !projection.escapes(func, use_counts))
            else {
                return Err(format!(
                    "bb{}: a call of `@{}` reads its {arity} results through memory",
                    block_id.index(),
                    module.function(callee).name
                ));
            };
            elided.extend(projection.elided);
            projections.insert(inst, projection.extras.into_vec());
        }
    }
    Ok((projections, elided))
}

/// The name of program data that every stage and final assembly share: a hash of the contract
/// whose code it is, or of its bytes.
pub(crate) fn data_key(data: &Data) -> String {
    let digest = match &data.bytes {
        DataBytes::Known(bytes) => Sha256::new().chain_update(b"bytes:").chain_update(bytes),
        DataBytes::Deferred(code) => {
            Sha256::new().chain_update(b"code:").chain_update(format!("{code:?}"))
        }
    }
    .finalize();
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// The placeholder of the offset of program data in the runtime code, which assembly fixes.
fn data_offset(data: &IndexVec<DataId, Data>, at: DataRef) -> String {
    format!("⟪dataoffset {} {}⟫", data_key(&data[at.id]), at.offset)
}

/// The placeholder of a size derived from program data's length, which assembly fixes.
fn data_size(data: &IndexVec<DataId, Data>, size: DataSize) -> String {
    let key = data_key(&data[size.data]);
    format!("⟪datasize {key} {} {}⟫", size.addend, u8::from(size.aligned))
}

/// The value numbers a call binds for each parameter: every value of the parameter, once each,
/// in increasing order.
fn mir_params(func: &Function) -> Vec<Vec<usize>> {
    if func.params.is_empty() {
        return Vec::new();
    }
    func.arg_uses()
        .iter_enumerated()
        .map(|(index, values)| {
            if values.is_empty() {
                // NOTE: an unread parameter has no value, so a number past every value binds it.
                vec![func.num_values() + index.index()]
            } else {
                // NOTE: `arg_uses` lists a value once for every use, so a value can repeat.
                let mut values = values.iter().map(|value| value.index()).collect::<Vec<_>>();
                values.sort_unstable();
                values.dedup();
                values
            }
        })
        .collect()
}

/// The operands of a call of `callee`: each argument once for every value of its parameter.
fn mir_call_args(
    module: &Module,
    func: &Function,
    callee: FunctionId,
    args: &[ValueId],
) -> Result<Vec<String>, String> {
    let params = mir_params(module.function(callee));
    if params.len() != args.len() {
        return Err(format!(
            "a call of `@{}` with {} arguments for {} parameters",
            module.function(callee).name,
            args.len(),
            params.len()
        ));
    }
    let mut operands = Vec::new();
    for (values, &arg) in params.iter().zip(args) {
        let operand = mir_operand(func, arg)?;
        operands.extend(std::iter::repeat_n(operand, values.len()));
    }
    Ok(operands)
}

/// The topic count of a `logN` mnemonic.
fn log_topics(name: &str) -> Option<usize> {
    let topics = name.strip_prefix("log")?.parse().ok()?;
    (topics <= 4).then_some(topics)
}

/// The value number of an instruction's result, checking that its type keeps word semantics.
fn mir_result(
    ty: Option<MirType>,
    result: Option<ValueId>,
    i1: bool,
    i160: bool,
) -> Result<usize, String> {
    let result = result.ok_or("the instruction has no result")?;
    match ty {
        Some(MirType::I256 | MirType::Ptr(PtrLocation::Memory)) => {}
        Some(MirType::I1) if i1 => {}
        Some(MirType::I160) if i160 => {}
        Some(ty) => return Err(format!("result type `{ty:?}` has no word semantics here")),
        None => return Err("the result has no type".to_string()),
    }
    Ok(result.index())
}

fn mir_operand(func: &Function, value: ValueId) -> Result<String, String> {
    Ok(match func.value(value) {
        Value::Inst(_) | Value::Arg(_) => format!(".value {}", value.index()),
        Value::Immediate(immediate) => format!(".const {}", word(&immediate_word(immediate))),
        Value::Undef(_) => return Err(format!("operand v{} is undefined", value.index())),
        Value::Error(_) => return Err(format!("operand v{} is an error", value.index())),
    })
}

fn immediate_word(immediate: &Immediate) -> U256 {
    match *immediate {
        Immediate::I1(bit) => U256::from(u8::from(bit)),
        Immediate::Int(value, _) | Immediate::I256(value) | Immediate::Pointer(value, _) => value,
    }
}

fn mir_terminator(
    module: &Module,
    func: &Function,
    terminator: Option<&Terminator>,
) -> Result<String, String> {
    let operand = |value| mir_operand(func, value);
    Ok(match terminator.ok_or("the block has no terminator")? {
        Terminator::Jump(target) => format!(".jump {}", target.index()),
        Terminator::Branch { condition, then_block, else_block } => format!(
            ".branch ({}) {} {}",
            operand(*condition)?,
            then_block.index(),
            else_block.index()
        ),
        Terminator::Switch { value, default, cases } => {
            let cases = cases
                .iter()
                .map(|&(case, target)| Ok(format!("({}, {})", operand(case)?, target.index())))
                .collect::<Result<Vec<_>, String>>()?;
            format!(".switch ({}) {} [{}]", operand(*value)?, default.index(), cases.join(", "))
        }
        Terminator::ReturnData { offset, size } => {
            format!(".ret ({}) ({})", operand(*offset)?, operand(*size)?)
        }
        Terminator::Revert { offset, size } => {
            format!(".revert ({}) ({})", operand(*offset)?, operand(*size)?)
        }
        Terminator::Stop => ".stop".to_string(),
        Terminator::Invalid => ".invalid".to_string(),
        Terminator::TailCall { function, args } => {
            let args = mir_call_args(module, func, *function, args)?;
            format!(".tailCall {} [{}]", function.index(), args.join(", "))
        }
        Terminator::Return { values } => {
            let values = values.iter().map(|&v| operand(v)).collect::<Result<Vec<_>, String>>()?;
            format!(".leave [{}]", values.join(", "))
        }
        other => return Err(format!("terminator `{}` has no Lean semantics", other.mnemonic())),
    })
}

// ---------------------------------------------------------------------------------------------
// EVM IR.

/// The Lean `EVM.Op`s of EVM IR's stack instructions, by mnemonic.
const STACK_OPS: &[&str] = &[
    "add",
    "sub",
    "mul",
    "div",
    "sdiv",
    "mod",
    "smod",
    "addmod",
    "mulmod",
    "exp",
    "signextend",
    "and",
    "or",
    "xor",
    "not",
    "byte",
    "shl",
    "shr",
    "sar",
    "lt",
    "gt",
    "slt",
    "sgt",
    "eq",
    "iszero",
    "calldatasize",
    "returndatasize",
];

/// The Lean memory operations by mnemonic.
const MEMORY_OPS: &[(&str, &str)] =
    &[("mload", "load"), ("mstore", "store"), ("mstore8", "storeByte"), ("msize", "size")];

/// The Lean `Effect`s of EVM IR's environment and storage instructions, with their arity.
const EVM_EFFECTS: &[(&str, usize)] = &[
    ("sload", 1),
    ("sstore", 2),
    ("tload", 1),
    ("tstore", 2),
    ("calldataload", 1),
    ("callvalue", 0),
    ("caller", 0),
    ("address", 0),
    ("keccak256", 2),
    ("calldatacopy", 3),
    ("mcopy", 3),
    ("returndatacopy", 3),
    ("gas", 0),
    ("codecopy", 3),
    ("create", 3),
    ("create2", 4),
];

/// The Lean halts by mnemonic.
const HALTS: &[(&str, &str)] =
    &[("stop", "stop"), ("return", "ret"), ("revert", "revert"), ("invalid", "invalid")];

/// The Lean term of an EVM IR program.
fn evm_program(program: &ir::Module) -> Result<String, String> {
    let mut blocks = Vec::new();
    for (id, block) in program.blocks.iter_enumerated() {
        let at = |message: String| format!("bb{} (block {}): {message}", block.label, id.index());
        let mut instrs = Vec::new();
        let mut iter = block.instructions.iter().peekable();
        while let Some(inst) = iter.next() {
            if let Some(value) = &inst.value {
                if let Some(immutable) = inst.immutable_push() {
                    let bytes = inst.immutable_type_size().map_or(0, |size| size.bytes());
                    if !(1..=32).contains(&bytes) {
                        return Err(at(format!(
                            "an immutable placeholder of {bytes} bytes has no Lean semantics"
                        )));
                    }
                    // pushN placeholder: the N bytes the deployment code writes there
                    instrs.push(format!(
                        ".effect (.immutable {} {bytes} .word) 0",
                        immutable.index()
                    ));
                    continue;
                }
                match value {
                    PushValue::Immediate(value) => {
                        instrs.push(format!(".op (.stack (.push {}))", word(value)));
                    }
                    PushValue::Block(target) => {
                        let jump = iter
                            .peek()
                            .filter(|next| next.value.is_none() && next.as_stack_op().is_none())
                            .and_then(|next| op::mnemonic(next.opcode))
                            .filter(|name| matches!(*name, "jump" | "jumpi"));
                        match jump {
                            Some(name) => {
                                iter.next();
                                // push bbN; jump(i)
                                instrs.push(format!(".{name} {}", target.index()));
                            }
                            // push bbN
                            None => instrs.push(format!(".pushLabel {}", target.index())),
                        }
                    }
                    // push the offset of program data, which assembly fixes
                    PushValue::Data(data) => {
                        instrs.push(format!(
                            ".op (.stack (.push {}))",
                            data_offset(&program.data, *data)
                        ));
                    }
                    // push a size derived from program data's length, which assembly fixes
                    PushValue::DataSize(size) => {
                        instrs.push(format!(
                            ".op (.stack (.push {}))",
                            data_size(&program.data, *size)
                        ));
                    }
                    other => return Err(at(format!("push of {other:?} has no Lean semantics"))),
                }
                continue;
            }
            if let Some(stack_op) = inst.as_stack_op() {
                instrs.push(match stack_op {
                    StackOp::Dup(n @ 1..=16) => format!(".op (.stack (.dup {}))", n - 1),
                    StackOp::Swap(n @ 1..=16) => format!(".op (.stack (.swap {}))", n - 1),
                    StackOp::Pop => ".op (.stack .pop)".to_string(),
                    other => return Err(at(format!("`{other:?}` has no Lean semantics"))),
                });
                continue;
            }
            let name = op::mnemonic(inst.opcode)
                .ok_or_else(|| at(format!("raw opcode 0x{:02x}", inst.opcode)))?;
            if STACK_OPS.contains(&name) {
                instrs.push(format!(".op (.stack .{name})"));
            } else if let Some((_, lean)) = MEMORY_OPS.iter().find(|op| op.0 == name) {
                instrs.push(format!(".op .{lean}"));
            } else if let Some((_, arity)) = EVM_EFFECTS.iter().find(|effect| effect.0 == name) {
                instrs.push(format!(".effect .{name} {arity}"));
            } else if let Some((_, arity, _)) = ENV_READS.iter().find(|read| read.0 == name) {
                instrs.push(format!(".effect (.env \"{name}\") {arity}"));
            } else if let Some((_, arity)) = CALLS.iter().find(|call| call.0 == name) {
                instrs.push(format!(".effect (.call .{name}) {arity}"));
            } else if let Some(topics) = log_topics(name) {
                instrs.push(format!(".effect (.log {topics}) {}", topics + 2));
            } else if name == "jump" {
                instrs.push(".jumpDyn".to_string());
            } else if name == "jumpi" {
                // jumpi with its label already on the stack: to the top, on the word below it
                instrs.push(".jumpiDyn".to_string());
            } else if let Some((_, lean)) = HALTS.iter().find(|halt| halt.0 == name) {
                instrs.push(format!(".{lean}"));
            } else {
                return Err(at(format!("instruction `{name}` has no Lean semantics")));
            }
        }
        if let Some(terminator) = &block.terminator {
            match &terminator.kind {
                TerminatorKind::Jump(target) => instrs.push(format!(".jump {}", target.index())),
                TerminatorKind::JumpI { then_block, else_block } => {
                    instrs.push(format!(".jumpi {}", then_block.index()));
                    instrs.push(format!(".jump {}", else_block.index()));
                }
                TerminatorKind::IndexedJump(targets) => {
                    let targets = targets.iter().map(|t| t.index().to_string()).collect::<Vec<_>>();
                    // indexed_jump [bbA, bbB, ..]: to the target the index on top numbers
                    instrs.push(format!(".jumpTable [{}]", targets.join(", ")));
                }
                TerminatorKind::Op(opcode) if op::mnemonic(*opcode) == Some("jump") => {
                    instrs.push(".jumpDyn".to_string());
                }
                TerminatorKind::Op(opcode) => {
                    let name = op::mnemonic(*opcode).unwrap_or("raw");
                    let Some((_, lean)) = HALTS.iter().find(|halt| halt.0 == name) else {
                        return Err(at(format!("terminator `{name}` has no Lean semantics")));
                    };
                    instrs.push(format!(".{lean}"));
                }
            }
        }
        blocks.push(format!("    -- bb{}\n    [{}]", block.label, instrs.join(",\n     ")));
    }
    Ok(format!("  {{ blocks := [\n{}] }}", blocks.join(",\n")))
}

// ---------------------------------------------------------------------------------------------
// Bytecode and manifest.

/// What binds a contract's stages to the compilation that produced them.
pub struct ManifestInfo<'a> {
    /// The compiler's version, with its commit.
    pub compiler: &'a str,
    /// The command line, after the program name.
    pub arguments: &'a [String],
    /// Every source of the compilation: its path and contents.
    pub sources: Vec<(String, &'a [u8])>,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn lean_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
        }
    }
    out.push('"');
    out
}

/// Writes the offsets and sizes of program data that assembly fixed into the stages, replacing
/// their placeholders, and returns where the code ends: the program data follows it to the end of
/// the runtime code. `data` is where final assembly put each entry it was given, by
/// [`data_key`], with its length; entries may overlap, as assembly shares equal bytes. Data that
/// does not follow the code, a placeholder of data the runtime code lacks, or of data placed at
/// two offsets, is an error.
fn program_data(
    gcx: Gcx<'_>,
    dir: &Path,
    contract: &str,
    len: usize,
    data: &[(String, usize, usize)],
) -> Result<usize> {
    let mut placed = data.to_vec();
    placed.sort_unstable_by_key(|&(_, offset, size)| (offset, size));
    let code_end = placed.first().map_or(len, |&(_, offset, _)| offset);
    let end = placed.iter().try_fold(code_end, |end, &(_, offset, size)| {
        (offset <= end).then_some(end.max(offset + size))
    });
    if end != Some(len) {
        return Err(error(
            gcx,
            format_args!("the program data of `{contract}` does not follow its code"),
        ));
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| error(gcx, format_args!("cannot read `{}`: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let Ok(file) = entry.file_name().into_string() else { continue };
        if !file.ends_with(".lean") || !is_stage_file(&file) {
            continue;
        }
        // The stage files are this exporter's output, not sources the source map loads.
        #[allow(clippy::disallowed_methods)]
        let text = fs::read_to_string(entry.path()).map_err(|e| {
            error(gcx, format_args!("cannot read `{}`: {e}", entry.path().display()))
        })?;
        if !text.contains("⟪data") {
            continue;
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find("⟪data") {
            out.push_str(&rest[..start]);
            let Some(len) = rest[start..].find('⟫') else { break };
            let marker = &rest[start + '⟪'.len_utf8()..start + len];
            rest = &rest[start + len + '⟫'.len_utf8()..];
            let mut words = marker.split(' ');
            let (kind, key) = (words.next(), words.next().unwrap_or_default());
            let fields = words.map(str::parse::<u64>).collect::<Vec<_>>();
            let mut places = placed.iter().filter(|(data, ..)| data == key);
            let found = places.next().filter(|first| places.all(|other| other == *first));
            if found.is_none() && placed.iter().any(|(data, ..)| data == key) {
                return Err(error(
                    gcx,
                    format_args!(
                        "in `{file}` of `{contract}`: the runtime code has program data {key} \
                         at several offsets"
                    ),
                ));
            }
            let value = match (kind, fields.as_slice()) {
                // the data's offset in the runtime code, plus the offset into it
                (Some("dataoffset"), [Ok(offset)]) => {
                    found.map(|&(_, at, _)| U256::from(at) + U256::from(*offset))
                }
                // the data's length plus the addend, rounded down to a word when aligned
                (Some("datasize"), [Ok(addend), Ok(aligned @ (0 | 1))]) => {
                    found.map(|&(_, _, size)| {
                        let size = U256::from(size) + U256::from(*addend);
                        if *aligned == 1 { size & !U256::from(31) } else { size }
                    })
                }
                _ => return Err(error(gcx, format_args!("`{file}` has a malformed placeholder"))),
            };
            let Some(value) = value else {
                return Err(error(
                    gcx,
                    format_args!(
                        "in `{file}` of `{contract}`: the runtime code has no program data {key}"
                    ),
                ));
            };
            out.push_str(&word(&value));
        }
        out.push_str(rest);
        write(gcx, &entry.path(), out.as_bytes())?;
    }
    Ok(code_end)
}

/// One stage as its file's first line names it.
struct Listed {
    file: String,
    kind: String,
    index: usize,
    pass: String,
    sha256: String,
}

impl Listed {
    /// The name of the stage's definition: its file's, lowercased first.
    fn def(&self) -> String {
        let mut def = self.file.clone();
        def[..1].make_ascii_lowercase();
        def
    }
}

/// Writes `Bytecode.lean`, `Manifest.lean` and `manifest.json` for a contract whose stages this
/// compilation wrote, once its runtime code is assembled. Does nothing for a contract without
/// stages, such as one without code. `libraries` says that the runtime code has library
/// placeholders, and `immutables` lists its immutable placeholders: the immutable, the offset of
/// the placeholder's bytes and their count.
pub fn write_manifest(
    gcx: Gcx<'_>,
    contract: &str,
    runtime: &[u8],
    libraries: bool,
    immutables: &[(usize, usize, usize)],
    data: &[(String, usize, usize)],
    info: &ManifestInfo<'_>,
) -> Result {
    let Some(dir) = contract_dir(gcx, contract) else { return Ok(()) };
    if !dir.join("Stage0.lean").exists() {
        return Ok(());
    }
    if libraries {
        return Err(error(
            gcx,
            format_args!(
                "the runtime code of `{contract}` has library placeholders, which the Lean \
                 bytecode cannot represent"
            ),
        ));
    }
    // NOTE: the deployment code writes each immutable over its placeholders, the zero bytes of
    // a `PUSH<N>`, which the stages read as the low `N` bytes of the immutable's word; the
    // manifest lists where, and the runtime code a deployment runs is this one linked there.
    let mut immutables = immutables.to_vec();
    immutables.sort_unstable_by_key(|&(id, start, bytes)| (start, id, bytes));
    immutables.dedup();
    for &(_, start, bytes) in &immutables {
        let window = start.checked_sub(1).and_then(|push| runtime.get(push..start + bytes));
        let push = 0x5f + bytes;
        if !(1..=32).contains(&bytes)
            || !window.is_some_and(|w| usize::from(w[0]) == push && w[1..].iter().all(|&b| b == 0))
        {
            return Err(error(
                gcx,
                format_args!(
                    "the immutable placeholder at byte {start} of `{contract}` is no \
                     `PUSH{bytes}` of zeros"
                ),
            ));
        }
    }
    if let Some(pair) = immutables.windows(2).find(|pair| pair[0].1 + pair[0].2 > pair[1].1) {
        return Err(error(
            gcx,
            format_args!(
                "the immutable placeholders at bytes {} and {} of `{contract}` overlap",
                pair[0].1, pair[1].1
            ),
        ));
    }
    let code_end = program_data(gcx, &dir, contract, runtime.len(), data)?;
    let ns = namespace(contract);
    let hex: String = runtime.iter().map(|b| format!("{b:02x}")).collect();
    let bytes = runtime[..code_end].iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>();
    let mut bytecode = format!(
        "{HEADER} kind=bytecode index=0 pass=assemble\n\
         /-! Generated by `solar -Zemit-lean-stages` for `{contract}`; do not edit. -/\n\
         \n\
         namespace {ns}\n\
         \n\
         /-- The runtime bytecode, byte by byte. -/\n\
         def bytecode : List Nat :=\n  [{}]\n\
         \n\
         /-- The runtime bytecode in hex, as `--emit=bin-runtime` prints it. -/\n\
         def bytecodeHex : String :=\n  \"{hex}\"\n\
         \n",
        bytes.chunks(16).map(|chunk| chunk.join(", ")).collect::<Vec<_>>().join(",\n   "),
    );
    if code_end < runtime.len() {
        let data = runtime[code_end..].iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>();
        let _ = write!(
            bytecode,
            "/-- The program data after the code, such as the creation code of a contract it \
             creates,\n\
             which \u{60}bytecode\u{60} leaves out: its offset in the runtime code and its bytes. -/\n\
             def dataStart : Nat := {code_end}\n\
             \n\
             def data : List Nat :=\n  [{}]\n\
             \n",
            data.chunks(16).map(|chunk| chunk.join(", ")).collect::<Vec<_>>().join(",\n   ")
        );
    }
    let _ = writeln!(bytecode, "end {ns}");
    write(gcx, &dir.join("Bytecode.lean"), bytecode.as_bytes())?;

    let mut stages = Vec::new();
    let entries = fs::read_dir(&dir)
        .map_err(|e| error(gcx, format_args!("cannot read `{}`: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let Ok(file) = entry.file_name().into_string() else { continue };
        if !file.ends_with(".lean") || file == "Manifest.lean" {
            continue;
        }
        // The stage files are this exporter's output, not sources the source map loads.
        #[allow(clippy::disallowed_methods)]
        let bytes = fs::read(entry.path()).map_err(|e| {
            error(gcx, format_args!("cannot read `{}`: {e}", entry.path().display()))
        })?;
        let text = String::from_utf8_lossy(&bytes);
        if text.contains('⟪') {
            return Err(error(
                gcx,
                format_args!("`{file}` still has a placeholder for a word codegen places"),
            ));
        }
        let Some(header) = text.lines().next().and_then(|line| line.strip_prefix(HEADER)) else {
            continue;
        };
        let field = |key: &str| {
            header.split_whitespace().find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
        };
        let (Some(kind), Some(index), Some(pass)) =
            (field("kind"), field("index").and_then(|i| i.parse().ok()), field("pass"))
        else {
            return Err(error(gcx, format_args!("`{file}` has a malformed stage header")));
        };
        stages.push(Listed {
            file: file.trim_end_matches(".lean").to_string(),
            kind: kind.to_string(),
            index,
            pass: pass.to_string(),
            sha256: sha256(&bytes),
        });
    }
    let rank = |kind: &str| ["mir", "sched", "evm", "bytecode"].iter().position(|k| *k == kind);
    if let Some(stage) = stages.iter().find(|stage| rank(&stage.kind).is_none()) {
        return Err(error(gcx, format_args!("`{}` has an unknown kind", stage.file)));
    }
    stages.sort_by_key(|stage| (rank(&stage.kind), stage.index));
    if !stages.iter().any(|stage| stage.kind == "sched") {
        return Err(error(gcx, format_args!("no scheduled runtime EVM IR for `{contract}`")));
    }

    let opts = &gcx.sess.opts;
    let evm_version = opts.evm_version.to_string();
    let optimization = format!("{:?}", opts.optimization);
    let list = |items: Vec<String>| format!("[{}]", items.join(",\n   "));
    let mut lean = format!(
        "{HEADER} kind=manifest index=0 pass=none\n\
         /-! Generated by `solar -Zemit-lean-stages` for `{contract}`; do not edit.\n\
         \n\
         What the stages of this directory were compiled from and by. `stages` lists each\n\
         stage's file, its definition, the pass whose output it is, and the SHA-256 of the file.\n\
         -/\n\
         \n\
         namespace {ns}.Manifest\n\
         \n"
    );
    let sources = info
        .sources
        .iter()
        .map(|(path, bytes)| format!("({}, {})", lean_string(path), lean_string(&sha256(bytes))))
        .collect();
    let _ = writeln!(lean, "def sources : List (String × String) :=\n  {}\n", list(sources));
    let _ = writeln!(lean, "def compiler : String := {}\n", lean_string(info.compiler));
    let arguments = info.arguments.iter().map(|a| lean_string(a)).collect();
    let _ = writeln!(lean, "def arguments : List String :=\n  {}\n", list(arguments));
    let _ = writeln!(lean, "def evmVersion : String := {}\n", lean_string(&evm_version));
    let _ = writeln!(lean, "def optimization : String := {}\n", lean_string(&optimization));
    let listed = stages
        .iter()
        .map(|s| {
            format!(
                "({}, {}, {}, {})",
                lean_string(&s.file),
                lean_string(&s.def()),
                lean_string(&s.pass),
                lean_string(&s.sha256)
            )
        })
        .collect();
    let _ = writeln!(
        lean,
        "def stages : List (String × String × String × String) :=\n  {}\n",
        list(listed)
    );
    let _ = writeln!(lean, "def bytecodeHex : String :=\n  {}\n", lean_string(&hex));
    let _ = writeln!(lean, "def bytecodeSha256 : String := {}\n", lean_string(&sha256(runtime)));
    if !immutables.is_empty() {
        let listed = immutables
            .iter()
            .map(|(id, start, bytes)| format!("({id}, {start}, {bytes})"))
            .collect();
        let _ = writeln!(
            lean,
            "/-- The immutable placeholders, which a deployment links the bytecode at: each one's\n\
             immutable, the offset of its bytes and their count. -/\n\
             def immutables : List (Nat × Nat × Nat) :=\n  {}\n",
            list(listed)
        );
    }
    if !data.is_empty() {
        let mut placed = data.to_vec();
        placed.sort_unstable_by_key(|&(_, offset, _)| offset);
        let listed = placed
            .iter()
            .map(|(key, offset, size)| format!("(\"{key}\", {offset}, {size})"))
            .collect();
        let _ = writeln!(
            lean,
            "/-- The program data after the code, as assembly placed it: each entry's key, \
             its offset in the\n\
             runtime code and its size, in the order of their offsets. -/\n\
             def data : List (String × Nat × Nat) :=\n  {}\n",
            list(listed)
        );
    }
    let _ = writeln!(lean, "end {ns}.Manifest");
    write(gcx, &dir.join("Manifest.lean"), lean.as_bytes())?;

    let mut json = serde_json::json!({
        "contract": contract,
        "sources": info.sources.iter().map(|(path, bytes)| serde_json::json!({
            "path": path,
            "sha256": sha256(bytes),
        })).collect::<Vec<_>>(),
        "compiler": info.compiler,
        "arguments": info.arguments,
        "evm_version": evm_version,
        "optimization": optimization,
        "options": format!("{opts:?}"),
        "stages": stages.iter().map(|s| serde_json::json!({
            "file": format!("{}.lean", s.file),
            "definition": s.def(),
            "kind": s.kind,
            "index": s.index,
            "pass": s.pass,
            "sha256": s.sha256,
        })).collect::<Vec<_>>(),
        "bytecode": hex,
        "bytecode_sha256": sha256(runtime),
    });
    if !data.is_empty() {
        json["data"] = data
            .iter()
            .map(|(key, offset, size)| serde_json::json!({ "key": key, "offset": offset, "size": size }))
            .collect();
    }
    if !immutables.is_empty() {
        json["immutables"] = immutables
            .iter()
            .map(|&(id, start, bytes)| serde_json::json!({ "id": id, "start": start, "bytes": bytes }))
            .collect();
    }
    let mut text = serde_json::to_string_pretty(&json).expect("the manifest serializes");
    text.push('\n');
    write(gcx, &dir.join("manifest.json"), text.as_bytes())
}
