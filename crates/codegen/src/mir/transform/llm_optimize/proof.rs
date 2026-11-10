//! Conservative equivalence checking through trusted MIR normalization.
//!
//! Sampling finds counterexamples but cannot justify installing an arbitrary body. Normalize
//! both bodies with the same existing, semantics-preserving passes and require identical MIR.
//! Equality is sufficient, not necessary: unproved loop closed forms and other transformations
//! are rejected even if every generated test agrees. No external proof claims are trusted.
//!
//! Run on private modules, with fresh analyses for each pass and without candidate metadata or
//! raw-memory assumptions. Four bounded rounds need not reach a fixed point: equal resulting
//! bodies already establish equivalence in their common callee and module environment.

use super::super::{cfg_simplify::CfgSimplify, cse::Cse, dce::Dce, egraph::Egraph, sccp::Sccp};
use crate::mir::{
    Function, FunctionId, Module,
    pass::{MirPass, ModuleAnalyses},
};
use solar_sema::Gcx;

pub(super) fn check(
    gcx: Gcx<'_>,
    module: &Module,
    id: FunctionId,
    candidate: &Function,
) -> Result<(), &'static str> {
    let normalized = |body: &Function| -> Result<String, &'static str> {
        let mut copy = module.clone();
        *copy.function_mut(id) = body.clone();
        copy.set_unsafe_assembly(true);
        super::drop_metadata(copy.function_mut(id));
        for _ in 0..4 {
            let mut changed = false;
            for pass in [&Sccp as &dyn MirPass, &Egraph, &Cse::All, &CfgSimplify, &Dce] {
                changed |= pass.run_pass(gcx, &mut copy, &mut ModuleAnalyses::default());
            }
            if !changed {
                break;
            }
        }
        // Parsing renumbers live SSA values and blocks; arena IDs are not semantic identities.
        let text = copy.candidate_text(copy.function(id)).to_string();
        let body = super::parse_candidate(gcx, &copy, &text)
            .map_err(|_| "cannot canonicalize the normalized body")?;
        Ok(copy.candidate_text(&body).to_string())
    };
    if normalized(module.function(id))? == normalized(candidate)? {
        Ok(())
    } else {
        Err("equivalence is unproved: trusted normalization does not produce the same body")
    }
}
