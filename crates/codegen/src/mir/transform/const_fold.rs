//! Constant-only cleanup after representation lowering.
//!
//! Reuse the e-graph's evaluator and identity rules, accepting only immediate
//! results. Rewrite uses after each sweep and repeat while instructions disappear,
//! so constants propagate through backedges without changing instruction choices
//! or extending nonconstant live ranges before stack scheduling. Passing checks
//! and zero-length copies also disappear; failing checks and trapping arithmetic
//! remain explicit. CFG cleanup runs separately after this pass.

use super::egraph;
use crate::mir::{
    Module,
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

pub(crate) struct ConstFold;

impl MirPass for ConstFold {
    fn name(&self) -> &'static str {
        "const-fold"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let changed = run_function_pass(module, analyses, |func, _| {
            let mut changed = false;
            let mut replacements = FxHashMap::default();
            let mut dead = DenseBitSet::new_empty(func.num_insts());
            loop {
                replacements.clear();
                dead.clear();
                for block in func.blocks.indices() {
                    for index in 0..func.blocks[block].instructions.len() {
                        let id = func.blocks[block].instructions[index];
                        let mut kind = func.inst(id).kind.clone();
                        kind.visit_operands_mut(|value| {
                            *value = replacements.get(value).copied().unwrap_or(*value);
                        });
                        if egraph::is_dead_noop(func, &kind, |value| value) {
                            // <passing check or zero-length copy> => nothing
                            dead.insert(id);
                        } else if let Some(result) = func.inst_result_value(id)
                            && let Some(value) = egraph::fold_constant(
                                func,
                                &kind,
                                gcx.sess.opts.evm_version,
                                func.value_ty(result),
                            )
                            && func.value_ty(result) == func.value_ty(value)
                        {
                            // %result = <constant expression> => constant
                            replacements.insert(result, value);
                            dead.insert(id);
                        }
                    }
                }
                if dead.is_empty() {
                    return changed;
                }
                // uses(%result) => constant; remove the folded definitions
                func.replace_uses_canonicalized(&replacements);
                for block in func.blocks.iter_mut() {
                    block.instructions.retain(|&id| !dead.contains(id));
                }
                changed = true;
            }
        });
        analyses.preserve_call_summaries();
        changed
    }
}
