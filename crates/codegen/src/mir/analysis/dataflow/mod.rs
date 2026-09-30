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
//! - [`value`] plugs single-value abstract domains into the engine: [`interval`], [`rounding`], and
//!   [`units`] are the built-in ones.
//!
//! Clients build on these layers. The analyses never change MIR: they are queried by lints
//! and by `-Zdataflow`, which prints per-program-point facts for FileCheck-based tests.
//! See `docs/DATAFLOW.md` for the design, the sources it draws on, and its limitations.

pub(crate) mod engine;
pub(crate) mod interproc;
pub(crate) mod interval;
pub(crate) mod lattice;
pub(crate) mod liveness;
pub(crate) mod reentrancy;
pub(crate) mod rounding;
pub(crate) mod slot_state;
pub(crate) mod storage;
pub(crate) mod storage_path;
pub(crate) mod taint;
pub(crate) mod units;
pub(crate) mod value;

use crate::mir::{ArgIdx, Module};
use interproc::ContextPolicy;
use solar_data_structures::map::FxHashMap;
use solar_sema::{Gcx, hir};
use std::fmt::Write as _;

/// Analyses selectable with `-Zdataflow`.
const ANALYSES: &[&str] =
    &["storage", "taint", "reentrancy", "liveness", "intervals", "rounding", "units"];

/// Runs the analyses requested with `-Zdataflow` on `module` and returns their fact dump.
///
/// Unknown analysis names emit an error and return `None`.
pub(crate) fn run_requested(gcx: Gcx<'_>, module: &Module, name: &str) -> Option<String> {
    let spec = gcx.sess.opts.unstable.dataflow.as_deref()?;
    let policy = ContextPolicy::call_strings(gcx.sess.opts.unstable.dataflow_k);
    let seeds = natspec_seeds(gcx, module);
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
            "intervals" => {
                run_value::<interval::Interval>(gcx, module, policy, seeds.clone(), &mut out);
            }
            "rounding" => {
                run_value::<rounding::Rounding>(gcx, module, policy, seeds.clone(), &mut out);
            }
            "units" => run_value::<units::Units>(gcx, module, policy, seeds.clone(), &mut out),
            _ => unreachable!("validated above"),
        }
    }
    Some(out)
}

/// Runs one value domain and emits its findings as warnings.
fn run_value<D: value::Seeded>(
    gcx: Gcx<'_>,
    module: &Module,
    policy: ContextPolicy,
    seeds: value::Seeds,
    out: &mut String,
) {
    let engine = value::analyze_module::<D>(module, policy, seeds);
    value::dump(&engine, out);
    for (func, context, _) in engine.final_summaries() {
        if !context.call_string.is_empty() {
            continue;
        }
        let Some(results) = engine.analysis.functions.get(&(func, context.clone())) else {
            continue;
        };
        let function = module.function(func);
        for (inst, finding) in &results.findings {
            let mut diag = gcx.dcx().warn(format!("{}: {finding}", D::NAME));
            if let Some(span) = function.inst(*inst).metadata.source_span() {
                diag = diag.span(span);
            }
            diag.emit();
        }
    }
}

/// Collects NatSpec annotations of parameters and results, prefixed by the parameter's name.
///
/// MIR functions are matched to their HIR declarations by span, so functions synthesized
/// during lowering receive no seeds.
fn natspec_seeds(gcx: Gcx<'_>, module: &Module) -> value::Seeds {
    let mut by_span = FxHashMap::default();
    for id in gcx.hir.function_ids() {
        by_span.insert(gcx.hir.function(id).span, id);
    }
    let mut seeds = value::Seeds::default();
    for (func, function) in module.iter_functions() {
        let Some(&id) = by_span.get(&function.declaration_span) else { continue };
        let hir_function = gcx.hir.function(id);
        if hir_function.parameters.len() != function.params.len() {
            continue;
        }
        let view = gcx.natspec_view(hir::ItemId::Function(id));
        for (i, &param) in hir_function.parameters.iter().enumerate() {
            let name = gcx.hir.variable(param).name.map(|name| name.name.to_string());
            let docs = view.parameter(i).iter().map(|item| item.content()).collect::<Vec<_>>();
            if name.is_some() || !docs.is_empty() {
                let text = format!("{} {}", name.unwrap_or_default(), docs.join(" "));
                seeds.params.insert((func, ArgIdx::from_usize(i)), text.trim().to_owned());
            }
        }
        for i in 0..hir_function.returns.len() {
            let docs = view.return_(i).iter().map(|item| item.content()).collect::<Vec<_>>();
            if !docs.is_empty() {
                seeds.returns.insert((func, i), docs.join(" "));
            }
        }
    }
    seeds
}
