//! A generic, compositional dataflow framework over MIR.
//!
//! The framework separates four concerns so that analyses compose instead of each pass
//! re-implementing a fixpoint:
//!
//! - [`lattice`] defines join-semilattices with optional widening and reusable combinators (flat
//!   constants, may/must sets, pointwise maps, products).
//! - [`engine`] solves one function's dataflow equations with a reverse-postorder worklist.
//!   Transfer functions see instructions, terminators, and CFG edges; edge transfer is where
//!   path-sensitive analyses assume branch conditions, and phis are bound per edge.
//! - [`interproc`] computes function summaries on demand, bottom-up through the call graph, with
//!   fixpoint iteration for recursion and configurable call-string sensitivity (`k = 0` gives pure,
//!   parametric summaries).
//! - [`storage_path`] abstracts storage locations as symbolic paths with field, index and
//!   mapping-key sensitivity; [`storage`] maps SSA values to those paths and summarizes each
//!   function's storage footprint relative to its storage-pointer parameters.
//!
//! Clients build on these layers. The analyses never change MIR: they are queried by lints
//! and by `-Zdataflow`, which prints per-program-point facts for FileCheck-based tests.
//! See `docs/DATAFLOW.md` for the design, the sources it draws on, and its limitations.

pub(crate) mod engine;
pub(crate) mod interproc;
pub(crate) mod lattice;
pub(crate) mod liveness;
pub(crate) mod reentrancy;
pub(crate) mod slot_state;
pub(crate) mod storage;
pub(crate) mod storage_path;
pub(crate) mod taint;

use crate::mir::Module;
use interproc::ContextPolicy;
use solar_sema::Gcx;
use std::fmt::Write as _;

/// Analyses selectable with `-Zdataflow`.
const ANALYSES: &[&str] = &["storage", "taint", "reentrancy", "liveness"];

/// Runs the analyses requested with `-Zdataflow` on `module` and returns their fact dump.
///
/// Unknown analysis names emit an error and return `None`.
pub(crate) fn run_requested(gcx: Gcx<'_>, module: &Module, name: &str) -> Option<String> {
    let spec = gcx.sess.opts.unstable.dataflow.as_deref()?;
    let policy = ContextPolicy::call_strings(gcx.sess.opts.unstable.dataflow_k);
    let mut out = String::new();
    for analysis in spec.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        if !ANALYSES.contains(&analysis) {
            gcx.dcx()
                .err(format!("unknown dataflow analysis: `{analysis}`"))
                .note(format!("valid analyses are {}", ANALYSES.join(", ")))
                .emit();
            return None;
        }
        let _ = writeln!(out, "// === dataflow {analysis} (k={}): {name} ===", policy.k);
        match analysis {
            "storage" => {
                let engine = storage::analyze_module(module, policy);
                storage::dump(&engine, &mut out);
            }
            "taint" => {
                let engine = taint::analyze_module(module, policy);
                taint::dump(&engine, &mut out);
            }
            "reentrancy" => {
                let (engine, findings) =
                    reentrancy::analyze_module(module, policy, gcx.sess.opts.evm_version);
                reentrancy::dump(&engine, &findings, &mut out);
                for finding in findings {
                    let mut diag = gcx.dcx().warn(finding.message).span(finding.span);
                    for (span, note) in finding.notes {
                        diag = diag.span_note(span, note);
                    }
                    diag.emit();
                }
            }
            "liveness" => liveness::dump(module, &mut out),
            _ => unreachable!("validated above"),
        }
    }
    Some(out)
}
