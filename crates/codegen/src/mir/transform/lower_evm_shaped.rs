//! Finish representation conversion and prepare control flow for the EVM backend.
//!
//! After ABI and dispatch lowering, some internal calls still target bodies that
//! terminate execution instead of returning. This pass drops the continuation of
//! every call to a bodied callee proven unable to return, including calls with
//! results. It replaces eligible calls with [`Terminator::TailCall`]; other calls
//! remain ordinary instructions followed by an unreachable `invalid` terminator.
//!
//! The pass then verifies all lowered-representation requirements and advances the
//! module from semantic to lowered MIR. A failed check leaves its phase semantic
//! and prevents the backend from consuming it.
//!
//! Arguments ride along: the backend stores them at the callee's compile-time
//! frame addresses and jumps, pushing no return address. That addressing only
//! exists for callees the backend gives a static frame (bodied, selectorless,
//! non-recursive), so calls to any other callee are left as ordinary calls.
//! Returnability follows explicit returns and tail-call chains conservatively;
//! unreachable returns can prevent a proof until CFG cleanup removes them. A
//! caller worklist propagates newly proven nonreturning bodies after cleanup;
//! it also follows tail-call wrappers without rescanning unrelated functions.
//!
//! The backend also eliminates phis by copying each incoming value at the end of its predecessor.
//! When a phi's previous value remains live on a sibling edge, that copy must run after the branch
//! selects the phi successor. This pass isolates only those copies in a single-successor block.

use crate::mir::{
    Callee, Function, InstKind, MirPhase, Module, Terminator,
    analysis::{CallGraphInfo, Liveness},
    pass::MirPass,
    transform::cfg_simplify::remove_unreachable_blocks,
    utils::{replace_terminator, split_edge},
};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};

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
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let changed = lower_evm_shaped(module);
        let _ = module.advance_phase(gcx.dcx(), MirPhase::Lowered);
        changed
    }
}

fn lower_evm_shaped(module: &mut Module) -> bool {
    if module.phase() != MirPhase::Semantic {
        return false;
    }

    // Skip call-graph analysis when no internal calls remain.
    let has_candidate = module.functions.iter().any(|func| {
        func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::ICall { function: Callee::Function(_), .. })
        })
    });
    if has_candidate {
        let call_graph = CallGraphInfo::new(module);
        let returning = module.returning_functions();
        let mut nonreturning = DenseBitSet::new_empty(module.functions.len());
        let mut tail_callable = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if func.blocks.is_empty() {
                continue;
            }
            if !returning.contains(func_id) {
                nonreturning.insert(func_id);
            }
            if func.selector.is_none()
                && !func.attributes.is_receive
                && !func.attributes.is_fallback
                && !call_graph.is_recursive(func_id)
            {
                tail_callable.insert(func_id);
            }
        }

        // The deployment path emits constructor-reachable bodies without static
        // frames, so an argument-carrying tail call has no compile-time
        // argument addresses there. Keep those calls ordinary; argument-less
        // rewrites need no frame addressing and stay valid on both paths.
        let mut constructor_reachable = call_graph.reachable_callees_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id)),
        );
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.is_constructor {
                constructor_reachable.insert(id);
            }
        }

        // Keep reverse edges even when cleanup removes a call: stale edges only
        // cause a redundant visit, and function IDs stay stable through block cleanup.
        let mut callers = index_vec![Vec::new(); module.functions.len()];
        for (caller, func) in module.iter_functions() {
            for inst in func.instructions() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                {
                    callers[callee].push(caller);
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                    callers[function].push(caller);
                }
            }
        }
        let mut worklist = Vec::new();
        let mut queued = DenseBitSet::new_empty(module.functions.len());
        for callee in &nonreturning {
            for &caller in &callers[callee] {
                if queued.insert(caller) {
                    worklist.push(caller);
                }
            }
        }
        while let Some(func_id) = worklist.pop() {
            queued.remove(func_id);
            let func = &mut module.functions[func_id];
            let mut function_changed = false;
            for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                let insts = &func.blocks[block_id].instructions;
                let Some((position, function)) =
                    insts.iter().enumerate().find_map(|(position, &inst_id)| {
                        let inst = func.inst(inst_id);
                        if let InstKind::ICall { function: Callee::Function(function), .. } =
                            &inst.kind
                            && nonreturning.contains(*function)
                        {
                            Some((position, *function))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };

                let inst = func.inst(insts[position]);
                let metadata = inst.metadata.debug_context();
                let InstKind::ICall { args, .. } = &inst.kind else { unreachable!() };
                if tail_callable.contains(function)
                    && (args.is_empty() || !constructor_reachable.contains(func_id))
                {
                    // result = icall callee, args -> tail_call callee, args
                    let terminator =
                        Terminator::TailCall { function, args: args.iter().copied().collect() };
                    func.blocks[block_id].instructions.truncate(position);
                    replace_terminator(func, block_id, terminator);
                    func.blocks[block_id].terminator_metadata = metadata;
                } else {
                    // result = icall callee, args
                    // invalid
                    func.blocks[block_id].instructions.truncate(position + 1);
                    replace_terminator(func, block_id, Terminator::Invalid);
                    // NOTE: The unreachable terminator has no source checkpoint.
                    func.blocks[block_id].terminator_metadata.mark_debug_info_dropped();
                }
                function_changed = true;
            }
            if function_changed {
                let _ = remove_unreachable_blocks(func);
            }
            if !nonreturning.contains(func_id)
                && !func.blocks.is_empty()
                && !func.blocks.iter().any(|block| match &block.terminator {
                    Some(Terminator::Return { .. }) => true,
                    Some(Terminator::TailCall { function, .. }) => {
                        !nonreturning.contains(*function)
                    }
                    _ => false,
                })
            {
                nonreturning.insert(func_id);
                for &caller in &callers[func_id] {
                    if queued.insert(caller) {
                        worklist.push(caller);
                    }
                }
            }
        }
    }
    for func in &mut module.functions {
        split_clobbering_phi_edges(func);
    }

    true
}

fn split_clobbering_phi_edges(func: &mut Function) {
    let phi_successors =
        func.blocks.indices().filter(|&block| func.block_has_phi(block)).collect::<Vec<_>>();
    if phi_successors.is_empty() {
        return;
    }

    let liveness = Liveness::compute(func);
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
