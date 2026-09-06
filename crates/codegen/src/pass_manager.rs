//! MIR pass execution, following rustc's MIR pass manager.

use crate::{
    mir::{MirPhase, Module, validate},
    pass::ModuleAnalyses,
    timing::PassTimer,
};
use solar_config::OptimizationMode;
use solar_data_structures::fmt::line_diff;
use solar_interface::{Result, diagnostics::DiagCtxt};
use solar_sema::Gcx;
use std::fmt::Display;

// `foo::Bar<'a>` becomes `Bar`, matching rustc's default MIR pass naming.
const fn simplify_pass_type_name(name: &'static str) -> &'static str {
    let bytes = name.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1] != b':' {
        i -= 1;
    }
    let (_, bytes) = bytes.split_at(i);

    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'<' {
        i += 1;
    }
    let (bytes, _) = bytes.split_at(i);

    match std::str::from_utf8(bytes) {
        Ok(name) => name,
        Err(_) => panic!(),
    }
}

pub(crate) fn parse_pass_pipeline<P: Copy>(
    gcx: Gcx<'_>,
    value: &str,
    ir: &str,
    lookup: impl Fn(&str) -> Option<P>,
) -> Result<Option<Vec<Option<P>>>> {
    if value == "default" {
        return Ok(None);
    }
    let passes = value
        .split(',')
        .map(|name| match name {
            "none" => Ok(None),
            _ => lookup(name)
                .map(Some)
                .ok_or_else(|| gcx.dcx().err(format!("unknown {ir} pass: {name}")).emit()),
        })
        .collect::<Result<_>>()?;
    Ok(Some(passes))
}

/// Returns whether pass managers should validate IR after each pass.
pub(crate) fn should_validate_ir(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.validate_ir.unwrap_or(cfg!(debug_assertions))
}

/// Returns the display label for a configured IR pipeline.
pub fn pipeline_label(value: &str) -> &str {
    if value == "default" { "pipeline-default" } else { value }
}

pub(crate) fn pipeline_output_name(gcx: Gcx<'_>, fallback: impl Display) -> String {
    if !gcx.sess.opts.language.is_source()
        && let Some(source) = gcx.sources.first()
    {
        return source.file.name.display().to_string();
    }
    fallback.to_string()
}

pub(crate) fn mir_output_name(gcx: Gcx<'_>, module: &Module) -> String {
    if gcx.sess.opts.language.is_source()
        && let Some(contract_id) = gcx
            .hir
            .contract_ids()
            .find(|&contract_id| gcx.hir.contract(contract_id).name == module.name)
    {
        return gcx.contract_fully_qualified_name(contract_id).to_string();
    }
    pipeline_output_name(gcx, module.name)
}

#[derive(Clone, Copy)]
struct PassOutput<'a> {
    name: Option<&'a str>,
}

/// A streamlined trait for a MIR transformation pass.
pub trait MirPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str {
        simplify_pass_type_name(std::any::type_name::<Self>())
    }

    /// Returns whether this pass is enabled with the current compiler flags and MIR phase.
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        self.is_required() || !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Runs the pass and returns whether it changed MIR.
    #[must_use]
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool;
}

/// Runs a sequence of MIR passes without validating after each pass.
#[must_use]
pub fn run_passes_no_validate(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
) -> bool {
    let output = PassOutput { name: None };
    run_passes_inner(gcx, module, passes, phase_change, false, output)
}

/// Runs a sequence of MIR passes, then applies `phase_change` when present.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    name: Option<&str>,
) -> bool {
    let output = PassOutput { name };
    run_passes_inner(gcx, module, passes, phase_change, true, output)
}

#[must_use]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    validate_each: bool,
    output: PassOutput<'_>,
) -> bool {
    let output_name =
        output.name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module));
    let explicit = output.name.is_some();
    let mut changed = false;
    let mut analyses = ModuleAnalyses::default();
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
            analyses.begin_pass();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = pass.run_pass(gcx, module, &mut analyses);
            timer.finish("MIR", module.name, pass_name, pass_changed);
            analyses.finish_pass(pass_changed);
            changed |= pass_changed;
            assert_debug_info_handled(module, pass_name, "after");

            if pass_changed && validate_each && should_validate_ir(gcx) {
                validate_module_after_pass(module, pass_name);
            }
        }

        if let Some(before) = before {
            print_pass_diff(&output_name, pass_name, before, module.to_text());
        } else if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {output_name} (after {pass_name}) ===");
            print!("{}", module.to_text());
        }
    }

    if let Some(new_phase) = phase_change {
        assert!(
            module.phase <= new_phase,
            "invalid MIR phase transition from {} to {}",
            module.phase.name(),
            new_phase.name()
        );
        let phase_changed = module.phase != new_phase;
        module.advance_phase(new_phase);
        changed |= phase_changed;
        if phase_changed && validate_each && should_validate_ir(gcx) {
            validate_module_after_pass(module, new_phase.name());
        }
    }

    changed
}

fn assert_debug_info_handled(module: &Module, pass_name: &str, when: &str) {
    if !module.debug_info_is_tracked() {
        return;
    }
    for (function, func) in module.iter_functions() {
        for (block, body) in func.blocks.iter_enumerated() {
            debug_assert!(
                body.terminator_metadata.debug_info_is_handled(),
                "MIR terminator debug information is unclassified {when} `{pass_name}` at fn{}, bb{}",
                function.index(),
                block.index(),
            );
            for &inst in &body.instructions {
                debug_assert!(
                    func.inst(inst).metadata.debug_info_is_handled(),
                    "MIR debug information is unclassified {when} `{pass_name}` at fn{}, bb{}, inst{}",
                    function.index(),
                    block.index(),
                    inst.index(),
                );
            }
        }
    }
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    validate(&dcx, module);
    if dcx.has_errors().is_err() {
        panic!("MIR validation failed after `{pass_name}`");
    }
}

pub(crate) fn print_pass_diff(
    name: impl Display,
    pass: impl Display,
    before: impl Display,
    after: impl Display,
) {
    let before = format!("// === {name} (before {pass}) ===\n{before}");
    let after = format!("// === {name} (after {pass}) ===\n{after}");
    print!("{}", line_diff(&before, &after));
}
