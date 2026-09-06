//! Expand semantic conditional checks before revert outlining and arithmetic expansion.
//!
//! Each check splits its block at the original execution point and branches to a typed panic or
//! revert payload. Checks in one function share payload blocks, merging their debug origins.
//! Surviving instructions and the original terminator move to the continuation; successor phi
//! predecessors update locally. The session's revert-string mode selects debug payloads without
//! changing when argument evaluation or the check occurs.

use crate::{
    mir::{FunctionBuilder, InstKind, Module},
    pass::{MirPass, run_function_pass},
    transform::utils::redirect_successor_predecessors,
};

pub(crate) struct LowerChecks;

impl MirPass for LowerChecks {
    fn name(&self) -> &'static str {
        "lower-checks"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            if !func.instructions().any(|id| matches!(func.inst(id).kind, InstKind::Check { .. })) {
                return false;
            }
            let blocks = func.blocks.indices();
            let mut builder =
                FunctionBuilder::new(func).with_revert_strings(gcx.sess.opts.revert_strings);
            for block in blocks {
                if !builder.func().blocks[block]
                    .instructions
                    .iter()
                    .any(|&id| matches!(builder.func().inst(id).kind, InstKind::Check { .. }))
                {
                    continue;
                }
                let instructions =
                    std::mem::take(&mut builder.func_mut().blocks[block].instructions);
                let (terminator, metadata) = builder.func_mut().blocks[block].take_terminator();
                builder.switch_to_block(block);
                for id in instructions {
                    let InstKind::Check { condition, is_zero, failure } =
                        builder.func().inst(id).kind
                    else {
                        // continuation: original instruction
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(id);
                        continue;
                    };
                    let context = builder.func().inst(id).metadata.debug_context();
                    builder.set_debug_context(&context);
                    // branch condition, failure, continuation
                    // failure: revert payload
                    builder.branch_to_revert(condition, is_zero, failure);
                }
                // continuation: original terminator
                let end = builder.current_block();
                if let Some(terminator) = terminator {
                    builder.func_mut().blocks[end].set_terminator(terminator, metadata);
                }
                if end != block {
                    redirect_successor_predecessors(builder.func_mut(), block, end);
                }
            }
            true
        })
    }
}
