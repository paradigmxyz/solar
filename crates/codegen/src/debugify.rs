//! Audits existing source origins around each MIR and EVM IR pass, like LLVM's original-debug-info
//! checks. Inventories contain only origins on live IR and proven label checkpoints; debugify
//! never inserts executable operations or influences a pass. Reports identify the first pass to
//! lose an origin. Strict checking is useful for isolated preservation regressions, not arbitrary
//! optimizing pipelines: deleting dead code can legitimately delete its last source location.
//!
//! NOTE: An origin's presence does not prove correct attribution or control-flow placement. Runtime
//! source-map tests must also check that untaken paths do not acquire hits. This audits source
//! spans, not variable locations or inline scopes, and does not synthesize LLVM-style line numbers.

use crate::backend::evm::CodegenState;
use solar_config::Debugify;
use solar_data_structures::map::FxHashSet;
use solar_interface::{Result, Span};
use solar_sema::Gcx;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) struct Origins(FxHashSet<Span>);

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
        Self(spans)
    }

    pub(crate) fn evm(module: &solar_evmir::ir::Module) -> Self {
        let mut spans = FxHashSet::default();
        let valid = module.valid_entry_checkpoints();
        for (id, block) in module.blocks.iter_enumerated() {
            if valid.contains(id)
                && let Some(checkpoint) = block.metadata.entry_checkpoint
            {
                spans.insert(checkpoint.source);
            }
            for inst in &block.instructions {
                spans.extend(inst.metadata.source_spans());
            }
            if let Some(term) = &block.terminator {
                spans.extend(term.metadata.source_spans());
            }
        }
        Self(spans)
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    pub(crate) fn check(
        self,
        gcx: Gcx<'_>,
        ir: &str,
        module: impl std::fmt::Display,
        pass: &str,
        after: Self,
    ) -> Result {
        let mut lost = self.0.difference(&after.0).copied().collect::<Vec<_>>();
        lost.sort_unstable_by_key(|span| (span.lo(), span.hi()));
        let retained = self.0.len() - lost.len();
        if gcx.sess.opts.unstable.debugify.is_some() {
            eprintln!(
                "debugify: {ir} {module} after {pass}: {retained}/{} origins retained, {} lost",
                self.0.len(),
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
                "before": self.0.len(),
                "retained": retained,
                "lost": lost.iter().map(|span| [span.lo().0, span.hi().0]).collect::<Vec<_>>(),
            });
            write_record(gcx, directory, &record).map_err(|error| {
                gcx.dcx().err(format!("could not write debugify audit: {error}")).emit()
            })?;
        }
        if !lost.is_empty() && gcx.sess.opts.unstable.debugify == Some(Debugify::Check) {
            return Err(gcx
                .dcx()
                .err(format!("debugify: {ir} pass `{pass}` lost {} source origins", lost.len()))
                .emit());
        }
        Ok(())
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
