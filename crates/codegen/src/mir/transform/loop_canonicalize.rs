//! Loop canonicalization for MIR.
//!
//! Gives every natural loop a dedicated preheader, LLVM's LoopSimplify
//! building block that LICM, induction-variable simplification, storage
//! promotion, and scalar evolution rely on. A loop already has one when a
//! single predecessor outside the loop jumps to its header; otherwise the pass
//! inserts a block that jumps to the header and redirects every outside
//! predecessor's edges to it. Each header phi then takes one input from the
//! preheader: the outside value when every outside predecessor passes the same
//! one, or a new phi in the preheader that merges them.
//!
//! The pass reanalyzes the loops after each insertion and stops when no loop
//! lacks a preheader. It skips a loop when some header phi has no input for an
//! outside predecessor. It runs before the semantic loop passes.

use crate::mir::{
    BlockId, Function, InstId, InstKind, Instruction, Module, Terminator, ValueId,
    analysis::{Loop, LoopAnalyzer},
    pass::{MirPass, run_function_pass},
    utils::replace_terminator,
};

/// Function pass for loop canonicalization.
pub(crate) struct LoopCanonicalize;

impl MirPass for LoopCanonicalize {
    fn name(&self) -> &'static str {
        "loop-canonicalize"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| canonicalize(func))
    }
}

/// Inserts preheaders until every loop that can take one has one.
fn canonicalize(func: &mut Function) -> bool {
    let mut changed = false;
    loop {
        let loop_info = LoopAnalyzer::new().analyze_structure(func);
        let Some((header, outside_preds)) = loop_info
            .loops
            .values()
            .filter(|loop_data| loop_data.preheader.is_none())
            .find_map(|loop_data| {
                let outside_preds = outside_predecessors(func, loop_data);
                (!outside_preds.is_empty() && phis_cover(func, loop_data.header, &outside_preds))
                    .then_some((loop_data.header, outside_preds))
            })
        else {
            return changed;
        };
        insert_preheader(func, header, &outside_preds);
        changed = true;
    }
}

fn outside_predecessors(func: &Function, loop_data: &Loop) -> Vec<BlockId> {
    func.blocks[loop_data.header]
        .predecessors
        .iter()
        .copied()
        .filter(|&pred| !loop_data.blocks.contains(pred))
        .collect()
}

/// Whether every header phi has an input for each outside predecessor.
fn phis_cover(func: &Function, header: BlockId, outside_preds: &[BlockId]) -> bool {
    func.blocks[header].instructions.iter().all(|&inst_id| {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { return true };
        outside_preds.iter().all(|pred| incoming.iter().any(|(from, _)| from == pred))
    })
}

fn insert_preheader(func: &mut Function, header: BlockId, outside_preds: &[BlockId]) {
    let preheader = func.alloc_block();
    // preheader: phi [outside_pred: value], ...; jump header
    // header: phi [..., preheader: merged_value]
    let phis: Vec<_> = func.blocks[header]
        .instructions
        .iter()
        .copied()
        .take_while(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        .collect();
    for inst_id in phis {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { unreachable!() };
        let outside: Vec<_> = outside_preds
            .iter()
            .filter_map(|pred| incoming.iter().find(|(from, _)| from == pred).copied())
            .collect();
        let value = preheader_value(func, inst_id, outside, preheader);
        let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind else { unreachable!() };
        incoming.retain(|(pred, _)| !outside_preds.contains(pred));
        incoming.push((preheader, value));
    }
    replace_terminator(func, preheader, Terminator::Jump(header));
    for &pred in outside_preds {
        // pred -> header => pred -> preheader
        let mut term = func.blocks[pred].terminator.clone().expect("predecessors are terminated");
        super::loop_split::retarget(
            &mut term,
            |target| {
                if target == header { preheader } else { target }
            },
        );
        replace_terminator(func, pred, term);
    }
}

/// The value a header phi takes from the preheader: the outside value when every outside
/// predecessor passes the same one, or a new preheader phi merging them.
fn preheader_value(
    func: &mut Function,
    header_phi: InstId,
    incoming: Vec<(BlockId, ValueId)>,
    preheader: BlockId,
) -> ValueId {
    let first = incoming[0].1;
    if incoming.iter().all(|&(_, value)| value == first) {
        return first;
    }
    let result_ty = func.inst(header_phi).result_ty;
    let (phi, result) = func.alloc_value_inst(
        Instruction::new(InstKind::Phi(incoming), result_ty).with_debug_info_dropped(),
    );
    func.blocks[preheader].instructions.push(phi);
    result
}
