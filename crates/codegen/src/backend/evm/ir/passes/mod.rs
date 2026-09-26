//! EVM IR optimization and layout passes.
//!
//! This module owns the pass list and canonical backend pipeline. Individual
//! transforms live in their own modules so their implementation and invariants
//! remain local, matching the organization of the MIR transforms.
//! Within a pipeline run, a pass that reported no change need not repeat until
//! another pass changes the module. The cache uses the pass type, name, and an
//! explicit configuration key so differently configured adapters stay distinct.
//! A changing pass clears the module cache. Independent block passes also retain exact
//! unchanged inputs, including metadata, keyed by pass configuration and stable block label.
//! They reuse those inputs across edits to other blocks and run large batches within the
//! contract graph's shared thread budget. No pass is assumed to reach a fixed point.

mod block_cse;
mod block_layout;
mod cfg_simplify;
mod coalesce_copies;
pub(crate) mod compact_pushes;
mod constant_data;
pub(super) mod data;
mod dce;
mod inline_returns;
mod late_structural;
mod legalize_shifts;
mod loop_layout;
mod outline;
mod peephole;
mod reorder_pushes;
mod share_reverts;
mod stack_normalize;
mod tail_merge;
mod terminal_dedup;
mod terminal_layout;
pub(super) mod utils;

pub(in crate::backend) use legalize_shifts::legalize_shifts;

use super::{Block, Module};
use crate::{
    mir::pass_manager::{
        parse_pass_pipeline, pipeline_output_name, print_pass_diff, should_validate_ir,
    },
    timing::PassTimer,
};
use solar_config::OptimizationMode;
use solar_data_structures::{map::FxHashMap, sync};
use solar_interface::diagnostics::DiagCtxt;
use solar_sema::Gcx;
use std::any::{Any, TypeId};

pub use crate::mir::pass_manager::pipeline_label;

/// A streamlined trait for an EVM IR transformation pass.
pub trait EvmPass: Any + Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str;

    /// Returns whether this pass is enabled with the current compiler flags.
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        self.is_required() || !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Stable discriminator for configured instances of the same pass type and name.
    fn cache_config(&self) -> u64 {
        0
    }

    /// Runs the pass and returns whether it changed EVM IR.
    #[must_use]
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;

    /// Runs with pipeline-local caches for explicitly independent block transforms.
    fn run_pass_with_cache(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _cache: &mut PassCache,
    ) -> bool {
        self.run_pass(gcx, module)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PassCacheKey {
    type_id: TypeId,
    name: &'static str,
    config: u64,
}

impl PassCacheKey {
    fn new(pass: &dyn EvmPass) -> Self {
        Self { type_id: pass.type_id(), name: pass.name(), config: pass.cache_config() }
    }
}

/// Retains unchanged block inputs for local passes within one pipeline run.
#[doc(hidden)]
#[derive(Default)]
pub struct PassCache {
    blocks: FxHashMap<PassCacheKey, FxHashMap<u32, Block>>,
    threads: usize,
    scheduling: crate::scheduling::Scheduling,
}

impl PassCache {
    fn new(gcx: Gcx<'_>, scheduling: &crate::scheduling::Scheduling) -> Self {
        let opts = &gcx.sess.opts.unstable;
        let threads = if opts.time_passes || opts.print_after_each || opts.pass_diff {
            1
        } else {
            gcx.sess.threads()
        };
        Self { threads, scheduling: scheduling.clone(), ..Self::default() }
    }

    fn run_blocks<R: FnMut(&mut Block) -> bool>(
        &mut self,
        pass: &dyn EvmPass,
        module: &mut Module,
        make_run: impl Fn() -> R + Sync,
    ) -> bool {
        let blocks = self.blocks.entry(PassCacheKey::new(pass)).or_default();
        let threads = self.scheduling.threads(self.threads);
        if threads > 1
            && module.blocks.iter().map(|block| block.instructions.len()).sum::<usize>() >= 8192
        {
            let mut tasks = module
                .blocks
                .iter_mut()
                .filter(|block| blocks.get(&block.label).is_none_or(|cached| cached != *block))
                .map(|block| (block, false))
                .collect::<Vec<_>>();
            let work = tasks.iter().map(|(block, _)| block.instructions.len()).sum::<usize>();
            if tasks.len() > 1 && work >= 8192 {
                let chunk_size = tasks.len().div_ceil(threads * 4);
                sync::scope(true, |scope| {
                    for chunk in tasks.chunks_mut(chunk_size) {
                        let make_run = &make_run;
                        scope.spawn(move |_| {
                            let mut run = make_run();
                            for (block, changed) in chunk {
                                *changed = run(block);
                            }
                        });
                    }
                });
            } else {
                let mut run = make_run();
                for (block, changed) in &mut tasks {
                    *changed = run(block);
                }
            }
            let mut changed = false;
            for (block, block_changed) in tasks {
                if block_changed {
                    blocks.remove(&block.label);
                } else if block.instructions.len() >= 8 {
                    blocks.insert(block.label, block.clone());
                }
                changed |= block_changed;
            }
            return changed;
        }
        let mut run = make_run();
        let mut changed = false;
        for block in &mut module.blocks {
            if blocks.get(&block.label).is_some_and(|cached| cached == block) {
                continue;
            }
            if run(block) {
                blocks.remove(&block.label);
                changed = true;
            } else if block.instructions.len() >= 8 {
                blocks.insert(block.label, block.clone());
            }
        }
        changed
    }
}

/// All EVM IR passes exposed by `-Zevm-ir-pipeline`.
pub static ALL_PASSES: &[&dyn EvmPass] = &[
    &block_cse::BlockCse,
    &peephole::Peephole::FINAL,
    &peephole::LateWord,
    &dce::Dce,
    &inline_returns::InlineReturns,
    &reorder_pushes::REORDER_PUSHES,
    &reorder_pushes::REORDER_EXPRESSIONS,
    &share_reverts::ShareReverts,
    &stack_normalize::StackDedup,
    &stack_normalize::StackNormalize,
    &compact_pushes::CompactPushes,
    &constant_data::ConstantData,
    &coalesce_copies::CoalesceCopies,
    &data::PackData,
    &legalize_shifts::LegalizeShifts,
    &cfg_simplify::CfgSimplify::FINAL,
    &outline::Outline,
    &terminal_dedup::TerminalDedup,
    &tail_merge::TailMerge,
    &block_layout::BlockLayout,
    &loop_layout::LoopLayout,
    &terminal_layout::TerminalLayout,
];

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
static DEFAULT_PIPELINE: &[&dyn EvmPass] = &[
    // Normalize and establish the first physical layout.
    &peephole::Peephole::EARLY,
    &coalesce_copies::CoalesceCopies,
    &cfg_simplify::CfgSimplify::EARLY,
    &data::PackExistingData,
    &peephole::Cleanup(compact_pushes::CompactPushes),
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    // Simplify and merge the explicit control-flow graph.
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify::EARLY,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify::EARLY,
    &tail_merge::TailMerge,
    // Outline only after straight-line paths and terminal tails are canonical.
    &outline::Outline,
    &cfg_simplify::CfgSimplify::EARLY,
    // Stack allocation can leave `producer; push; swap1` when the producer was emitted first.
    // Reorder it only after structural sharing is fixed so local stack cleanup cannot perturb
    // outlining choices.
    &reorder_pushes::REORDER_PUSHES,
    &peephole::Peephole::EARLY,
    // Regenerate only after structural sharing is fixed. Doing this before
    // tail merging can make otherwise-identical blocks context-dependent and
    // lose more shared bytes than the local CSE removes.
    &peephole::Cleanup(block_cse::BlockCse),
    &peephole::Cleanup(dce::Dce),
    // Stack normalization exposes local rewrites.
    &peephole::Cleanup(stack_normalize::StackNormalize),
    // Pack address-sensitive terminal blocks, then clean up any adjacent
    // revert branch that remains profitable in the final layout.
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &cfg_simplify::CfgSimplify::EARLY,
    &block_layout::BlockLayout,
    // Block CSE and final placement can expose new equal tails whose addresses or predecessors
    // differed during the first structural sweep. Run a bounded second structural sweep;
    // each pass remains internally profitability-gated.
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify::EARLY,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify::EARLY,
    &tail_merge::TailMerge,
    &outline::Outline,
    &cfg_simplify::CfgSimplify::EARLY,
    &reorder_pushes::FINAL_REORDER_PUSHES,
    &peephole::Peephole::EARLY,
    &peephole::Cleanup(block_cse::BlockCse),
    &peephole::Cleanup(dce::Dce),
    &peephole::Cleanup(stack_normalize::StackNormalize),
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &cfg_simplify::CfgSimplify::FINAL,
    &block_layout::BlockLayout,
    // Materialize constants and pack the referenced data pool before final sharing and cleanup.
    &constant_data::ConstantData,
    &data::PackData,
    // Data packing can add compactable immediates and local stack shuffles.
    &compact_pushes::CompactPushes,
    &peephole::Peephole::FINAL,
    &stack_normalize::StackDedup,
    &peephole::Cleanup(dce::Dce),
    &late_structural::LateStructural,
    &peephole::Peephole::FINAL,
    &inline_returns::InlineReturns,
    &cfg_simplify::CfgSimplify::FINAL,
    &loop_layout::LoopLayout,
    &terminal_layout::TerminalLayout,
    &reorder_pushes::REORDER_EXPRESSIONS,
    &peephole::LateWord,
];

/// Finds an EVM IR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

/// Runs an EVM IR pass pipeline.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
) -> bool {
    run_passes_inner(gcx, module, passes, true, name, &crate::scheduling::Scheduling::default())
}

/// Runs EVM IR passes without validating after each pass.
#[must_use]
pub fn run_passes_no_validate(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass]) -> bool {
    run_passes_inner(gcx, module, passes, false, None, &crate::scheduling::Scheduling::default())
}

#[must_use]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    validate_each: bool,
    name: Option<&str>,
    scheduling: &crate::scheduling::Scheduling,
) -> bool {
    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let explicit = name.is_some();
    let mut changed = false;
    let mut unchanged = Vec::<PassCacheKey>::new();
    let mut cache = PassCache::new(gcx, scheduling);
    for pass in passes {
        let pass_name = pass.name();
        let before =
            (explicit && gcx.sess.opts.unstable.pass_diff).then(|| module.to_text().to_string());
        let enabled = pass.is_enabled(gcx, module);
        if !enabled && !explicit {
            continue;
        }

        if enabled {
            assert_debug_info_handled(module, pass_name, "before");
            let errors_before = gcx.dcx().err_count();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let cache_key = PassCacheKey::new(*pass);
            let cached = unchanged.contains(&cache_key);
            debug_assert!(unchanged.iter().filter(|entry| **entry == cache_key).count() <= 1);
            let target_support_before = (!cached && validate_each && should_validate_ir(gcx))
                .then(|| super::verify::Verifier::new(gcx).target_support_snapshot(module));
            let pass_changed = !cached && pass.run_pass_with_cache(gcx, module, &mut cache);
            if pass_changed {
                unchanged.clear();
            } else if !cached {
                unchanged.push(cache_key);
            }
            timer.finish("EVM IR", module.name(), pass_name, pass_changed);
            changed |= pass_changed;
            if gcx.dcx().err_count() != errors_before {
                return changed;
            }
            if pass_changed && let Some(target_support_before) = target_support_before {
                validate_module_after_pass(gcx, module, pass_name, &target_support_before);
            }
            assert_debug_info_handled(module, pass_name, "after");
        }

        if let Some(before) = before {
            print_pass_diff(&output_name, pass_name, before, module.to_text());
        } else if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {output_name} (after {pass_name}) ===");
            print!("{}", module.to_text());
        }
    }
    changed
}

fn validate_module_after_pass(
    gcx: Gcx<'_>,
    module: &Module,
    pass_name: &str,
    target_support_before: &super::verify::TargetSupportSnapshot,
) {
    let dcx = DiagCtxt::new_early();
    super::verify::Verifier::for_evm_version(&dcx, gcx.sess.opts.evm_version)
        .verify_between_passes(module, target_support_before);
    if dcx.has_errors().is_err() {
        panic!("EVM IR validation failed after `{pass_name}`");
    }
}

fn assert_debug_info_handled(module: &Module, pass_name: &str, when: &str) {
    // NOTE: These development-only checks catch missing metadata policy in new
    // rewrites. Release builds must not abort just because debug output was
    // requested; unclassified locations remain unknown.
    if !module.debug_info_is_tracked() {
        return;
    }
    for (block_id, block) in module.blocks.iter_enumerated() {
        for (index, inst) in block.instructions.iter().enumerate() {
            debug_assert!(
                inst.metadata.debug_info_is_handled(),
                "EVM IR debug information is unclassified {when} `{pass_name}` at bb{}, instruction {index}",
                block_id.index(),
            );
        }
        if let Some(term) = &block.terminator {
            debug_assert!(
                term.metadata.debug_info_is_handled(),
                "EVM IR terminator debug information is unclassified {when} `{pass_name}` at bb{}",
                block_id.index(),
            );
        }
    }
}

/// Runs the configured EVM IR pipeline, or the canonical pipeline when none was provided.
///
/// `name` overrides the module name in pass output.
#[must_use]
pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    run_pipeline_with_scheduling(gcx, module, name, &crate::scheduling::Scheduling::default())
}

pub(crate) fn run_pipeline_with_scheduling(
    gcx: Gcx<'_>,
    module: &mut Module,
    name: Option<&str>,
    scheduling: &crate::scheduling::Scheduling,
) -> bool {
    super::verify::Verifier::new(gcx).verify_before_pipeline(module);
    if gcx.dcx().has_errors().is_err() {
        return false;
    }

    let Some(value) = gcx.sess.opts.unstable.evm_ir_pipeline.as_deref() else {
        return run_passes_inner(gcx, module, DEFAULT_PIPELINE, true, None, scheduling);
    };
    let pipeline = match parse_pass_pipeline(gcx, value, "EVM IR", lookup_pass) {
        Ok(pipeline) => pipeline,
        Err(_) => return false,
    };
    let Some(passes) = pipeline else {
        return run_passes_inner(gcx, module, DEFAULT_PIPELINE, true, None, scheduling);
    };

    let name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let mut changed = false;
    for pass in passes {
        if let Some(pass) = pass {
            changed |= run_passes_inner(gcx, module, &[pass], true, Some(&name), scheduling);
        } else if gcx.sess.opts.unstable.pass_diff {
            let text = module.to_text();
            print_pass_diff(&name, "none", &text, &text);
        } else if gcx.sess.opts.unstable.print_after_each {
            println!("// === {name} (after none) ===");
            print!("{}", module.to_text());
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::{ir::Instruction, op};
    use solar_interface::sym;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn pass_cache_keys_include_configuration() {
        let ordinary = PassCacheKey::new(&reorder_pushes::REORDER_PUSHES);
        let final_pushes = PassCacheKey::new(&reorder_pushes::FINAL_REORDER_PUSHES);
        let expressions = PassCacheKey::new(&reorder_pushes::REORDER_EXPRESSIONS);

        assert_ne!(ordinary, final_pushes);
        assert_ne!(final_pushes, expressions);
        assert_ne!(ordinary, expressions);
        assert_eq!(ordinary, PassCacheKey::new(&reorder_pushes::REORDER_PUSHES));

        let dce_with_cleanup = peephole::Cleanup(dce::Dce);
        assert_ne!(PassCacheKey::new(&dce::Dce), PassCacheKey::new(&dce_with_cleanup));
    }

    #[test]
    fn block_cache_tracks_inputs_and_configuration() {
        let mut module = Module::new(sym::runtime);
        let mut block = Block::new(0);
        block.instructions = vec![Instruction::opcode(op::PC); 8];
        module.add_block(block);
        let mut cache = PassCache::default();
        let calls = AtomicUsize::new(0);
        let unchanged = || {
            |_: &mut Block| {
                calls.fetch_add(1, Ordering::Relaxed);
                false
            }
        };
        assert!(!cache.run_blocks(&peephole::Peephole::EARLY, &mut module, unchanged));
        assert!(!cache.run_blocks(&peephole::Peephole::EARLY, &mut module, unchanged));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        module.blocks[super::super::BlockId::from_usize(0)].instructions[0] =
            Instruction::opcode(op::MSIZE);
        assert!(!cache.run_blocks(&peephole::Peephole::EARLY, &mut module, unchanged));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(!cache.run_blocks(&peephole::Peephole::FINAL, &mut module, unchanged));
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert!(cache.run_blocks(&peephole::LateWord, &mut module, || |block| {
            block.instructions[0] = Instruction::opcode(op::PC);
            true
        }));
        assert!(!cache.run_blocks(&peephole::LateWord, &mut module, unchanged));
        assert_eq!(calls.load(Ordering::Relaxed), 4);
    }
}
