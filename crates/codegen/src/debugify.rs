//! Audits source debug information around each MIR and EVM IR pass and across stack scheduling.
//! Debugify never inserts executable operations and never changes a pass's decisions.
//!
//! Two measures, both copied from LLVM (`llvm/lib/Transforms/Utils/Debugify.cpp`):
//!
//! - **Gaps** are LLVM's original-mode `not-generate` check (`checkInstructionCoverage`): after
//!   each pass, the live instructions and terminators (phis excluded) that have no origin and no
//!   [`DebugLocKind`] explaining why. Each gap is reported once, in the record of the pass that
//!   left it, and then set to [`DebugLocKind::Unknown`], as LLVM's caller does, so that later
//!   passes are not blamed for it. Gaps present when a pipeline run receives a module, as lowering
//!   made them, get an `input` record. `-Zdebugify=check` rejects every gap, like LLVM's check,
//!   which has no exemptions; use it on a focused `-Zmir-pipeline` or `-Zevm-ir-pipeline`.
//! - **Lost origins** are the analogue of LLVM's synthetic-mode `Missing line` warning, which LLVM
//!   prints and does not count as a failure: the origins on live IR before a pass that no longer
//!   appear after it. Deleting dead code or merging two locations legitimately loses an origin, so
//!   this is information, never a check. Reports identify the first pass to lose one.
//!
//! Marking a gap `Unknown` changes only the location kind, which code generation never reads and
//! text output does not print for `Unknown`: the debug-output tests compare bytecode, source maps
//! and ETHDebug with the audit on and off.
//!
//! NOTE: An origin's presence does not prove correct attribution or control-flow placement. Runtime
//! source-map tests must also check that untaken paths do not acquire hits. This audits source
//! spans, not variable locations or inline scopes, and does not synthesize LLVM-style line numbers.

use crate::backend::evm::CodegenState;
use solar_config::Debugify;
use solar_data_structures::map::FxHashSet;
use solar_evmir::debug::DebugLocKind;
use solar_interface::{Result, Span, Symbol};
use solar_sema::Gcx;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub(crate) struct Origins {
    spans: FxHashSet<Span>,
    /// Gaps found while collecting, which are reported with the pass that created them.
    gaps: Gaps,
}

/// Instructions and terminators with neither a source origin nor a reason for lacking one: LLVM's
/// `not-generate` finding (`Debugify.cpp` `checkInstructionCoverage`). Unlike a lost origin, a gap
/// never depends on what other code survives.
///
/// Only the count and a histogram of opcodes are exact; the first [`SITE_LIMIT`] sites are kept to
/// locate them, so that memory does not grow with the number of gaps.
#[derive(Default)]
pub(crate) struct Gaps {
    count: usize,
    opcodes: BTreeMap<String, usize>,
    sites: Vec<Gap>,
}

struct Gap {
    /// The MIR function, or the EVM IR program, which is a contract's runtime or deployment code.
    scope: Option<String>,
    block: usize,
    /// The index in the block, or `None` for its terminator.
    instruction: Option<usize>,
    opcode: String,
}

impl Gaps {
    /// Finds the gaps in `module`, then marks each [`DebugLocKind::Unknown`] as LLVM's caller
    /// does, so that later passes are not blamed for it. Phis have no location of their own, and
    /// LLVM skips them too.
    fn mir(module: &mut solar_mir::Module) -> Self {
        let mut gaps = Self::default();
        if !module.debug_info_is_tracked() {
            return gaps;
        }
        for func in module.functions.iter_mut() {
            let function = func.debug_identifier.unwrap_or(func.name.symbol);
            for block in 0..func.blocks.len() {
                let block_id = solar_mir::BlockId::from_usize(block);
                for index in 0..func.blocks[block_id].instructions.len() {
                    let inst = func.blocks[block_id].instructions[index];
                    let instruction = func.inst_mut(inst);
                    if matches!(instruction.kind, solar_mir::InstKind::Phi(_))
                        || !instruction.metadata.lacks_debug_location()
                    {
                        continue;
                    }
                    instruction.metadata.set_debug_loc_kind(DebugLocKind::Unknown);
                    gaps.push(Some(function), block, Some(index), instruction.kind.mnemonic());
                }
                let body = &mut func.blocks[block_id];
                if let Some(terminator) = &body.terminator
                    && body.terminator_metadata.lacks_debug_location()
                {
                    gaps.push(Some(function), block, None, terminator.mnemonic());
                    body.terminator_metadata.set_debug_loc_kind(DebugLocKind::Unknown);
                }
            }
        }
        gaps
    }

    /// Like [`Self::mir`], for EVM IR.
    fn evm(module: &mut solar_evmir::ir::Module) -> Self {
        let mut gaps = Self::default();
        if !module.debug_info_is_tracked() {
            return gaps;
        }
        let program = module.name();
        for (id, block) in module.blocks.iter_mut().enumerate() {
            for (index, instruction) in block.instructions.iter_mut().enumerate() {
                if instruction.metadata.lacks_debug_location() {
                    instruction.metadata.set_debug_loc_kind(DebugLocKind::Unknown);
                    gaps.push(Some(program), id, Some(index), &instruction.mnemonic().to_string());
                }
            }
            if let Some(terminator) = &mut block.terminator
                && terminator.metadata.lacks_debug_location()
            {
                terminator.metadata.set_debug_loc_kind(DebugLocKind::Unknown);
                let opcode = match &terminator.kind {
                    solar_evmir::ir::TerminatorKind::Jump(_) => "jump",
                    solar_evmir::ir::TerminatorKind::JumpI { .. } => "jumpi",
                    solar_evmir::ir::TerminatorKind::IndexedJump(_) => "indexed_jump",
                    solar_evmir::ir::TerminatorKind::Op(opcode) => {
                        solar_evmir::op::mnemonic(*opcode).unwrap_or("raw")
                    }
                };
                gaps.push(Some(program), id, None, opcode);
            }
        }
        gaps
    }

    fn push(
        &mut self,
        scope: Option<Symbol>,
        block: usize,
        instruction: Option<usize>,
        opcode: &str,
    ) {
        self.count += 1;
        match self.opcodes.get_mut(opcode) {
            Some(count) => *count += 1,
            None => {
                self.opcodes.insert(opcode.to_owned(), 1);
            }
        }
        if self.sites.len() < SITE_LIMIT {
            self.sites.push(Gap {
                scope: scope.map(|scope| scope.to_string()),
                block,
                instruction,
                opcode: opcode.to_owned(),
            });
        }
    }

    fn extend(&mut self, other: Self) {
        self.count += other.count;
        for (opcode, count) in other.opcodes {
            *self.opcodes.entry(opcode).or_default() += count;
        }
        let room = SITE_LIMIT.saturating_sub(self.sites.len());
        self.sites.extend(other.sites.into_iter().take(room));
    }

    /// Reports the gaps found at `pass`, which is `input` for what a module arrived with.
    ///
    /// Like LLVM's check, `-Zdebugify=check` rejects every gap, whichever pass left it.
    fn report(&self, gcx: Gcx<'_>, ir: &str, module: impl std::fmt::Display, pass: &str) -> Result {
        if self.count == 0 {
            return Ok(());
        }
        if gcx.sess.opts.unstable.debugify.is_some() {
            let when = if pass == INPUT { "at input".to_owned() } else { format!("after {pass}") };
            eprintln!("debugify: {ir} {module} {when}: {} without a location", self.count);
            for gap in &self.sites {
                let scope =
                    gap.scope.as_ref().map_or_else(String::new, |scope| format!("{scope} "));
                let index =
                    gap.instruction.map_or_else(|| "terminator".into(), |i| format!("inst{i}"));
                eprintln!("debugify: gap {scope}bb{} {index} `{}`", gap.block, gap.opcode);
            }
            if self.count > self.sites.len() {
                eprintln!("debugify: and {} more", self.count - self.sites.len());
            }
        }
        if let Some(directory) = &gcx.sess.opts.unstable.debugify_output {
            // The count and the opcode histogram stay exact; only the sites are capped.
            let record = serde_json::json!({
                "kind": "gaps",
                "ir": ir,
                "module": module.to_string(),
                "pass": pass,
                "count": self.count,
                "opcodes": self.opcodes,
                "sites": self.sites.iter().map(|gap| serde_json::json!({
                    "scope": gap.scope,
                    "block": gap.block,
                    "instruction": gap.instruction,
                    "opcode": gap.opcode,
                })).collect::<Vec<_>>(),
                "omitted": self.count - self.sites.len(),
            });
            write_record(gcx, directory, &record).map_err(|error| {
                gcx.dcx().err(format!("could not write debugify audit: {error}")).emit()
            })?;
        }
        if gcx.sess.opts.unstable.debugify == Some(Debugify::Check) {
            let what = if self.count == 1 { "instruction" } else { "instructions" };
            let message = if pass == INPUT {
                format!(
                    "debugify: {ir} module arrived with {} {what} without a location",
                    self.count
                )
            } else {
                format!(
                    "debugify: {ir} pass `{pass}` left {} {what} without a location",
                    self.count
                )
            };
            return Err(gcx.dcx().err(message).emit());
        }
        Ok(())
    }
}

/// The record that reports the gaps a module arrives with.
const INPUT: &str = "input";

/// Sites listed per record in the JSON audit, which keeps the raw artifact small.
const SITE_LIMIT: usize = 100;

/// Reports the gaps a MIR pipeline run receives in its module.
pub(crate) fn report_mir_input(gcx: Gcx<'_>, module: &mut solar_mir::Module) -> Result {
    Gaps::mir(module).report(gcx, "MIR", module.name, INPUT)
}

impl Origins {
    pub(crate) fn mir(module: &solar_mir::Module) -> Self {
        let mut spans = FxHashSet::default();
        for (_, func) in module.iter_functions() {
            for block in &func.blocks {
                spans.extend(block.terminator_metadata.source_spans());
                for &inst in &block.instructions {
                    spans.extend(func.inst(inst).metadata.source_spans());
                }
            }
        }
        Self { spans, gaps: Gaps::default() }
    }

    pub(crate) fn evm(module: &solar_evmir::ir::Module) -> Self {
        let mut spans = FxHashSet::default();
        for block in &module.blocks {
            for inst in &block.instructions {
                spans.extend(inst.metadata.source_spans());
            }
            if let Some(term) = &block.terminator {
                spans.extend(term.metadata.source_spans());
            }
        }
        Self { spans, gaps: Gaps::default() }
    }

    /// The origins of `module` after a pass, together with the gaps it left. See [`Gaps::mir`].
    pub(crate) fn mir_after(module: &mut solar_mir::Module) -> Self {
        let mut origins = Self::mir(module);
        origins.gaps = Gaps::mir(module);
        origins
    }

    /// The origins of `module` after a pass, together with the gaps it left. See [`Gaps::evm`].
    pub(crate) fn evm_after(module: &mut solar_evmir::ir::Module) -> Self {
        let mut origins = Self::evm(module);
        origins.gaps = Gaps::evm(module);
        origins
    }

    pub(crate) fn check(
        self,
        gcx: Gcx<'_>,
        ir: &str,
        module: impl std::fmt::Display,
        pass: &str,
        after: Self,
    ) -> Result {
        let mut lost = self.spans.difference(&after.spans).copied().collect::<Vec<_>>();
        lost.sort_unstable_by_key(|span| (span.lo(), span.hi()));
        let retained = self.spans.len() - lost.len();
        if gcx.sess.opts.unstable.debugify.is_some() {
            eprintln!(
                "debugify: {ir} {module} after {pass}: {retained}/{} origins retained, {} lost",
                self.spans.len(),
                lost.len()
            );
            for span in &lost {
                eprintln!("debugify: lost {}..{}", span.lo().0, span.hi().0);
            }
        }
        if let Some(directory) = &gcx.sess.opts.unstable.debugify_output {
            let record = serde_json::json!({
                "kind": "pass",
                "ir": ir,
                "module": module.to_string(),
                "pass": pass,
                "before": self.spans.len(),
                "retained": retained,
                "lost": lost.iter().map(|span| [span.lo().0, span.hi().0]).collect::<Vec<_>>(),
            });
            write_record(gcx, directory, &record).map_err(|error| {
                gcx.dcx().err(format!("could not write debugify audit: {error}")).emit()
            })?;
        }
        after.gaps.report(gcx, ir, &module, pass)
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.spans.extend(other.spans);
        self.gaps.extend(other.gaps);
    }
}

pub(crate) fn enabled(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.debugify.is_some() || gcx.sess.opts.unstable.debugify_output.is_some()
}

/// One file per compilation, with serialized writes from parallel codegen workers.
/// Write complete records immediately: the command-line driver can exit without dropping state.
fn write_record(gcx: Gcx<'_>, directory: &Path, record: &serde_json::Value) -> io::Result<()> {
    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    let mut output = gcx.codegen_state::<CodegenState>().debugify_output.0.lock().unwrap();
    if output.is_none() {
        fs::create_dir_all(directory)?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        // Concurrent compilations may observe the same clock tick.
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("{}-{stamp}-{sequence}.jsonl", std::process::id()));
        let mut file = File::options().write(true).create_new(true).open(path)?;
        let header = serde_json::json!({
            "kind": "invocation",
            "version": 1,
            "arguments": std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "sources": gcx.sess.source_map().files().iter().map(|file| serde_json::json!({
                "name": file.name.display().to_string(),
                "start": file.start_pos.0,
                "end": file.end_position().0,
            })).collect::<Vec<_>>(),
        });
        write_json_line(&mut file, &header)?;
        *output = Some(file);
    }
    write_json_line(output.as_mut().expect("audit file initialized"), record)
}

fn write_json_line(file: &mut File, record: &serde_json::Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(record)?;
    bytes.push(b'\n');
    file.write_all(&bytes)
}

/// The audit stream belongs to the compilation, not its process or output directory.
#[derive(Default)]
pub(crate) struct Output(Mutex<Option<File>>);
