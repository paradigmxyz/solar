//! EVM pass registration and execution contracts.
//!
//! The registry preserves command-line names while adapters remain beside their
//! transforms. Execution coordinates optimization gating, validation, timings and
//! textual before/after captures. Pipeline booleans report IR changes; diagnostics
//! carry failures independently. Explicit `none` retains pass-output conventions.

use super::Module;
use crate::{
    mir::pass_manager::{
        parse_pass_pipeline, pipeline_output_name, print_pass_diff, should_validate_ir,
    },
    timing::PassTimer,
};
use solar_config::OptimizationMode;
use solar_sema::Gcx;

/// A transformation of a physical EVM program.
pub trait EvmPass: Sync {
    /// Returns the command-line pass name.
    fn name(&self) -> &'static str;

    /// Returns whether this pass is enabled at the current optimization level.
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        self.is_required() || !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    /// Returns whether target correctness requires this pass.
    fn is_required(&self) -> bool {
        false
    }

    /// Runs the pass and reports whether its input changed.
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;
}

/// All public EVM pass names, in command-line registry order.
pub static ALL_PASSES: &[&dyn EvmPass] = &[
    &super::local::LocalPass("block-cse"),
    &super::local::LocalPass("peephole"),
    &super::local::LocalPass("dce"),
    &super::local::TerminalPrefixes,
    &super::local::LocalPass("reorder-pushes"),
    &super::cfg::ShareReverts,
    &super::local::LocalPass("stack-dedup"),
    &super::local::LocalPass("stack-normalize"),
    &super::local::LocalPass("compact-pushes"),
    &super::data::ConstantData,
    &super::data::CoalesceCopies,
    &super::data::PackData,
    &super::legalize::LegalizeShifts,
    &super::cfg::CfgSimplify,
    &super::outline::Outline,
    &super::blocks::BlockDedup,
    &super::cfg::TerminalDedup,
    &super::cfg::RedirectTerminals,
    &super::cfg::TailMerge,
    &super::cold::ColdBlocks,
    &super::cfg::BlockLayout,
    &super::local::LiteralOrientation,
    &super::local::EnvironmentCopies,
];

/// Looks up a public pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

/// Executes a pass sequence, validating each result.
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
) -> bool {
    execute(gcx, module, passes, name, true)
}

/// Executes a pass sequence without intermediate validation.
pub fn run_passes_no_validate(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass]) -> bool {
    execute(gcx, module, passes, None, false)
}

fn execute(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
    validate: bool,
) -> bool {
    if validate {
        super::validate(gcx, module);
        if gcx.dcx().has_errors().is_err() {
            return false;
        }
    }
    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name));
    let mut changed = false;
    for pass in passes {
        let before = (name.is_some() && gcx.sess.opts.unstable.pass_diff)
            .then(|| module.to_text().to_string());
        if pass.is_enabled(gcx, module) {
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = pass.run_pass(gcx, module);
            timer.finish("EVM IR", module.name, pass.name(), pass_changed);
            changed |= pass_changed;
            if validate && should_validate_ir(gcx) {
                super::validate(gcx, module);
            }
            if gcx.dcx().has_errors().is_err() {
                return changed;
            }
        } else if name.is_none() {
            continue;
        }
        if let Some(before) = before {
            print_pass_diff(&output_name, pass.name(), before, module.to_text());
        } else if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {output_name} (after {}) ===", pass.name());
            print!("{}", module.to_text());
        }
    }
    changed
}

/// Executes the configured target pipeline.
pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    if let Some(value) = gcx.sess.opts.unstable.evm_ir_pipeline.as_deref() {
        let Ok(pipeline) = parse_pass_pipeline(gcx, value, "EVM IR", lookup_pass) else {
            return false;
        };
        if let Some(passes) = pipeline {
            let output_name = name
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| pipeline_output_name(gcx, module.name));
            let mut changed = false;
            for pass in passes {
                if let Some(pass) = pass {
                    changed |= run_passes(gcx, module, &[pass], Some(&output_name));
                } else if gcx.sess.opts.unstable.pass_diff {
                    let text = module.to_text();
                    print_pass_diff(&output_name, "none", &text, &text);
                } else if gcx.sess.opts.unstable.print_after_each {
                    println!("// === {output_name} (after none) ===");
                    print!("{}", module.to_text());
                }
                if gcx.dcx().has_errors().is_err() {
                    return changed;
                }
            }
            return changed;
        }
    }
    let mut passes: Vec<&dyn EvmPass> = vec![
        &super::cfg::CfgSimplify,
        &super::local::LocalPass("peephole"),
        &super::local::LocalPass("block-cse"),
        &super::local::LocalPass("dce"),
        &super::local::LocalPass("stack-normalize"),
        &super::local::LocalPass("reorder-pushes"),
        &super::local::LocalPass("compact-pushes"),
    ];
    if gcx.sess.opts.optimization.is_gas() {
        passes.push(&super::local::LocalPass("block-cse"));
    }
    passes.extend([
        &super::cfg::CfgSimplify as &dyn EvmPass,
        &super::data::PackData,
        &super::data::CoalesceCopies,
    ]);
    if gcx.sess.opts.optimization.is_size() {
        passes.extend([
            &super::cfg::TerminalDedup as &dyn EvmPass,
            &super::cfg::ShareReverts,
            &super::cfg::TailMerge,
            &super::outline::Outline,
            &super::cfg::CfgSimplify,
        ]);
    }
    passes.extend([
        &super::local::LocalPass("dce") as &dyn EvmPass,
        &super::local::TerminalPrefixes,
        &super::cfg::CfgSimplify,
        &super::cfg::BlockLayout,
        &super::blocks::BlockDedup,
        &super::cfg::RedirectTerminals,
        &super::legalize::LegalizeShifts,
        &super::local::LiteralOrientation,
        &super::local::EnvironmentCopies,
        &super::cold::ColdBlocks,
    ]);
    run_passes(gcx, module, &passes, None)
}
