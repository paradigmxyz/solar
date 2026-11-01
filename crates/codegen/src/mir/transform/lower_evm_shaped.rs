//! Finish representation conversion and prepare control flow for the EVM backend.
//!
//! After ABI and dispatch lowering, some internal calls still target bodies that
//! terminate execution instead of returning. This pass drops the continuation of
//! every call to a bodied callee proven unable to return, including calls with
//! results, as `noreturn-calls` does early in the semantic pipeline. It replaces
//! eligible calls with tail calls; other calls remain ordinary instructions followed
//! by an unreachable `invalid` terminator.
//!
//! The pass then erases pointer types: every pointer becomes the `i256` word it holds and the
//! casts between the two disappear, so the backend sees one value for each address. It verifies
//! all lowered-representation requirements and advances the module from semantic to lowered MIR. A
//! failed check leaves its phase semantic and prevents the backend from consuming it.
//!
//! Arguments ride along on the stack: the backend jumps with them in place and
//! pushes no return address. Only bodied, selectorless, non-recursive callees
//! qualify.
//! Returnability follows explicit returns and tail-call chains conservatively;
//! unreachable returns can prevent a proof until CFG cleanup removes them. A
//! caller worklist propagates newly proven nonreturning bodies after cleanup;
//! it also follows tail-call wrappers without rescanning unrelated functions.
//!
//! The backend also eliminates phis by copying each incoming value at the end of its predecessor.
//! When a phi's previous value remains live on a sibling edge, that copy must run after the branch
//! selects the phi successor. This pass isolates only those copies in a single-successor block.

use crate::mir::{
    Function, Immediate, InstId, InstKind, MirPhase, MirType, Module, Value, ValueId,
    analysis::Liveness, pass::MirPass, transform::cfg_simplify::end_nonreturning_calls,
    utils::split_edge,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::cell::LazyCell;

/// Shapes call edges and checks the final MIR phase transition.
pub(crate) struct LowerEvmShaped;

impl MirPass for LowerEvmShaped {
    fn name(&self) -> &'static str {
        "lower-evm-shaped"
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase() == MirPhase::Semantic
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        end_nonreturning_calls(module, true);
        let mut replacements = FxHashMap::default();
        let mut casts = DenseBitSet::new_empty(0);
        let mut instructions = Vec::new();
        for func in &mut module.functions {
            erase_pointer_types(func, &mut replacements, &mut casts, &mut instructions);
            split_clobbering_phi_edges(func);
        }
        if let Err(error) = module.advance_phase(gcx.sess, MirPhase::Lowered) {
            analyses.fail(error);
        }
        true
    }
}

/// Retypes every pointer as the word it holds and drops the casts between the two, so the backend
/// sees one value for each address.
fn erase_pointer_types(
    func: &mut Function,
    replacements: &mut FxHashMap<ValueId, ValueId>,
    casts: &mut DenseBitSet<InstId>,
    instructions: &mut Vec<InstId>,
) {
    let word = |ty: MirType| if ty.is_pointer() { MirType::I256 } else { ty };
    for index in func.arg_indices() {
        let ty = func.arg_ty(index);
        func.set_arg_ty(index, word(ty));
    }
    let returns = func.return_abi().map(|abi| abi.iter().copied().map(word).collect::<Vec<_>>());
    func.set_return_type(word(func.return_type()));
    if let Some(returns) = returns {
        func.set_return_abi(returns);
    }
    for index in 0..func.num_values() {
        // ptr<location> literal -> i256 literal
        match func.value_mut(ValueId::new(index)) {
            Value::Immediate(immediate @ Immediate::Pointer(..)) => {
                let word = immediate.as_u256().expect("pointer literals hold a word");
                *immediate = Immediate::I256(word);
            }
            Value::Undef(ty) => *ty = word(*ty),
            _ => {}
        }
    }
    replacements.clear();
    casts.clear_to(func.num_insts());
    instructions.clear();
    instructions.extend(func.instructions());
    for &inst in &*instructions {
        let narrow_operand = match func.inst(inst).kind {
            InstKind::IntToPtr(operand) => func.value_ty(operand) != Some(MirType::I256),
            _ => false,
        };
        let instruction = func.inst_mut(inst);
        instruction.result_ty = instruction.result_ty.map(word);
        match instruction.kind {
            // pointer = inttoptr narrow to ptr<location> -> zext narrow to i256
            InstKind::IntToPtr(operand) if narrow_operand => {
                instruction.kind = InstKind::Zext(operand);
            }
            // word = ptrtoint pointer to i256 -> pointer
            // pointer = inttoptr word to ptr<location> -> word
            InstKind::PtrToInt(operand, 256) | InstKind::IntToPtr(operand) => {
                casts.insert(inst);
                if let Some(result) = func.inst_result_value(inst) {
                    replacements.insert(result, operand);
                }
            }
            // narrow = ptrtoint pointer to iN -> trunc pointer to iN
            InstKind::PtrToInt(operand, bits) => instruction.kind = InstKind::Trunc(operand, bits),
            _ => {}
        }
    }
    if casts.is_empty() {
        return;
    }
    func.replace_uses_canonicalized(replacements);
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !casts.contains(*inst));
    }
}

fn split_clobbering_phi_edges(func: &mut Function) {
    let phi_successors =
        func.blocks.indices().filter(|&block| func.block_has_phi(block)).collect::<Vec<_>>();
    if phi_successors.is_empty() {
        return;
    }

    // Only a phi on an edge whose source has another successor needs liveness.
    let liveness = LazyCell::new(|| Liveness::compute_live_sets(func));
    let mut edges = Vec::new();

    for successor in phi_successors {
        let block = &func.blocks[successor];
        for &predecessor in &block.predecessors {
            let Some(terminator) = &func.blocks[predecessor].terminator else { continue };
            let successors = terminator.successors();
            if !successors.iter().any(|&sibling| sibling != successor) {
                continue;
            }

            let terminator_operands = terminator.operands();
            let copy_clobbers_live_value = block
                .instructions
                .iter()
                .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                .filter(|&&inst| {
                    let InstKind::Phi(incoming) = &func.inst(inst).kind else { unreachable!() };
                    incoming.iter().any(|&(block, _)| block == predecessor)
                })
                .filter_map(|&inst| func.inst_result_value(inst))
                .any(|destination| {
                    terminator_operands.contains(&destination)
                        || successors.iter().any(|&sibling| {
                            sibling != successor && liveness.live_in(sibling).contains(destination)
                        })
                });
            if copy_clobbers_live_value {
                edges.push((predecessor, successor));
            }
        }
    }

    edges.sort_unstable_by_key(|(predecessor, successor)| (predecessor.index(), successor.index()));
    edges.dedup();
    for (predecessor, successor) in edges {
        split_edge(func, predecessor, successor);
    }
}
