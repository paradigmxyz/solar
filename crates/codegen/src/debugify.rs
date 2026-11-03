//! Audits existing source origins around each MIR and EVM IR pass, like LLVM's original-debug-info
//! checks. Inventories contain only origins on live IR and proven label checkpoints; debugify
//! never inserts executable operations or influences a pass. Reports identify the first pass to
//! lose an origin. Strict checking is useful for isolated preservation regressions, not arbitrary
//! optimizing pipelines: deleting dead code can legitimately delete its last source location.
//!
//! NOTE: An origin's presence does not prove correct attribution or control-flow placement. Runtime
//! source-map tests must also check that untaken paths do not acquire hits. This audits source
//! spans, not variable locations or inline scopes, and does not synthesize LLVM-style line numbers.

use solar_config::Debugify;
use solar_data_structures::map::FxHashSet;
use solar_interface::{Result, Span};
use solar_sema::Gcx;

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
        eprintln!(
            "debugify: {ir} {module} after {pass}: {retained}/{} origins retained, {} lost",
            self.0.len(),
            lost.len()
        );
        for span in &lost {
            eprintln!("debugify: lost {}..{}", span.lo().0, span.hi().0);
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
