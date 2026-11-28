//! Emission of planned stack-resident functions into EVM IR.

use super::{BlockPlan, Edge, Exit, FunctionPlan, ModuleInfo, Reader, Step, plan::call_projection};
use crate::{
    backend::{
        assembler::Label,
        evm::{
            DebugFunctionExit,
            codegen::{
                Body, EvmCodegen, StackOp,
                select::{InstKindOpcode, rematerializable_nullary_value},
                values::gas_minus,
            },
            ir::Metadata,
            op,
        },
    },
    mir::{
        BlockId, Callee, EdgeAssignment, Function, FunctionId, InstId, InstKind,
        InstructionMetadata, Module, PtrLocation, StackReturn, Terminator, Value, ValueId,
        analysis::{CallGraphInfo, CfgInfo},
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_evmir::debug::DebugLocKind;
use solar_interface::Span;

impl<'gcx> EvmCodegen<'gcx> {
    pub(super) fn emit_runtime_stackified(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        info: &ModuleInfo,
        mut plans: IndexVec<FunctionId, Option<FunctionPlan>>,
    ) {
        let entry = info.entry;
        self.register_stackified_functions(module, call_graph, info, &plans);

        self.record_runtime_entry_reachability(call_graph, entry);
        self.emitting_entry = info.emitting_entry;
        let plan = plans[entry].take().expect("entry plan");
        self.enter_function(module, &module.functions[entry]);
        self.comment_function_entry(&module.functions[entry], true, false);
        self.emit_stackified_body(entry, module, plan);
        self.leave_function(module);
        self.emitting_entry = false;
        self.runtime_entry_funcs.push(entry);

        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry || info.internal.contains(func_id) {
                continue;
            }
            let Some(plan) = plans[func_id].take() else { continue };
            self.enter_function(module, func);
            let label = self.function_labels[&func_id];
            self.asm.define_external_entry_label(label);
            self.mark_debug_external_entry_label(label, func);
            self.mark_debug_function_invoke(func, true);
            self.comment_function_entry(func, false, false);
            self.emit_entry_free_memory_start(module, call_graph, func_id);
            self.emit_stackified_body(func_id, module, plan);
            self.leave_function(module);
            self.runtime_entry_funcs.push(func_id);
        }

        self.emit_internal_bodies(module, info, &mut plans);

        self.pack_scalar_static_frames(module);
        self.resolve_static_frames(module);
    }

    /// Emits the constructor's helpers, then the constructor, which the deployment prefix
    /// enters by jumping over the helpers. Every helper gets a fixed frame; constructor code
    /// runs once, so the frames lie one after another above the constructor's spill area.
    pub(super) fn emit_constructor_stackified(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        info: &ModuleInfo,
        mut plans: IndexVec<FunctionId, Option<FunctionPlan>>,
    ) {
        let constructor = info.entry;
        self.static_frame_functions.insert(constructor);
        self.register_stackified_functions(module, call_graph, info, &plans);

        // push constructor; jump; <helpers>; constructor:
        let func = &module.functions[constructor];
        let constructor_label = self.asm.new_label();
        if !info.internal.is_empty() {
            // NOTE: This jump is part of entering the constructor: it runs once, after the argument
            // copy, which already maps to the constructor's declaration, and before the body, and
            // it only reaches the body. It therefore maps to the same declaration, which is the
            // contract's for the synthesized constructor. solc has no such jump, but it emits the
            // code that enters the constructor under the same location, in `appendConstructor`
            // (libsolidity/codegen/ContractCompiler.cpp L286-L288 at v0.8.38).
            self.emit_with_origin(Some(func.declaration_span), |codegen| {
                codegen.asm.emit_push_label(constructor_label);
                codegen.asm.emit_op(op::JUMP);
            });
        }
        self.emit_internal_bodies(module, info, &mut plans);
        self.enter_function(module, func);
        if !info.internal.is_empty() {
            self.asm.define_label(constructor_label);
            self.mark_block_label_origin(
                constructor_label,
                &func.blocks[BlockId::ENTRY].label_metadata,
            );
        }
        self.mark_debug_function_invoke(func, true);
        self.comment_function_entry(func, false, true);
        self.body = Body::Constructor(constructor);
        let plan = plans[constructor].take().expect("constructor plan");
        self.emit_stackified_body(constructor, module, plan);
        self.leave_function(module);
        self.body = Body::External;

        self.pack_scalar_static_frames(module);
        // Allocations stay dynamic: the constructor's heap starts above its argument copy.
        for (_, allocations) in self.pending_static_allocs.drain() {
            for (alloc, size) in allocations {
                self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
            }
        }
    }

    /// Gives every internal function a fixed frame, and every planned function but the entry a
    /// label. A recursive function's frame lies apart from the others: its callers keep the
    /// words they still need on the stack across calls that may reenter it.
    fn register_stackified_functions(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        info: &ModuleInfo,
        plans: &IndexVec<FunctionId, Option<FunctionPlan>>,
    ) {
        for func_id in info.internal.iter() {
            self.static_frame_functions.insert(func_id);
            if call_graph.is_recursive(func_id) {
                self.recursive_frame_functions.insert(func_id);
            }
        }
        for (func_id, plan) in plans.iter_enumerated() {
            if plan.is_some() && func_id != info.entry {
                let label = self.new_function_label(module, func_id);
                self.function_labels.insert(func_id, label);
            }
        }
    }

    fn emit_internal_bodies(
        &mut self,
        module: &Module,
        info: &ModuleInfo,
        plans: &mut IndexVec<FunctionId, Option<FunctionPlan>>,
    ) {
        for func_id in info.internal.iter() {
            let Some(plan) = plans[func_id].take() else { continue };
            let func = &module.functions[func_id];
            self.enter_function(module, func);
            self.asm.repeating_code = info.repeated.contains(func_id);
            let label = self.function_labels[&func_id];
            match info.call_effect(func_id) {
                Some(effect) => self.asm.define_function_label(label, effect),
                None => self.asm.define_label(label),
            }
            self.asm.mark_function_entry(label);
            self.mark_block_label_origin(label, &func.blocks[BlockId::ENTRY].label_metadata);
            self.mark_debug_function_invoke(func, false);
            self.comment_function_entry(func, false, false);
            self.body = Body::Internal(func_id);
            self.emit_stackified_body(func_id, module, plan);
            self.leave_function(module);
        }
        self.asm.repeating_code = false;
        self.body = Body::External;
    }

    /// Starts emitting `func`, whose own objective prices its code and marks its blocks, its
    /// first label's included, when its tag selects one.
    fn enter_function(&mut self, module: &Module, func: &Function) {
        self.optimization = module.objective_of(func);
        self.asm.block_optimization = func.attributes.optimization;
    }

    fn leave_function(&mut self, module: &Module) {
        self.optimization = module.optimization;
        self.asm.block_optimization = None;
    }

    fn emit_stackified_body(
        &mut self,
        func_id: FunctionId,
        module: &Module,
        mut plan: FunctionPlan,
    ) {
        let func = &module.functions[func_id];
        // NOTE: The records name instructions of the function the dump prints, so they come from
        // `func` even when the plan is for a rebuilt copy. Rebuilding adds and removes only pure
        // instructions, so the buffer loads, and the values bound to the extra results, are the
        // same in both.
        if self.capture_mir {
            let published = plan
                .blocks
                .iter()
                .flatten()
                .flat_map(|block| &block.steps)
                .filter_map(|step| match step {
                    &Step::Publish { inst, .. } => Some(inst),
                    _ => None,
                })
                .collect::<FxHashSet<_>>();
            for (block_id, block) in func.blocks.iter_enumerated() {
                for (index, &inst) in block.instructions.iter().enumerate() {
                    let InstKind::ICall { function: Callee::Function(callee), .. } =
                        func.inst(inst).kind
                    else {
                        continue;
                    };
                    let arity = module.functions[callee].return_components().len();
                    if arity <= 1 {
                        continue;
                    }
                    let Some(protocol) = call_projection(func, block_id, index, arity) else {
                        continue;
                    };
                    if protocol.extras.iter().any(Option::is_some) || !published.contains(&inst) {
                        self.stack_return_sites.insert(
                            (func_id, inst),
                            StackReturn {
                                elided: protocol.elided,
                                extras: protocol.extras.into_vec(),
                            },
                        );
                    }
                }
            }
        }
        // The plan may be for a copy that rebuilds values at their uses.
        let rebuilt = plan.rebuilt.take();
        let original_values = func.num_values();
        let func = rebuilt.as_deref().unwrap_or(func);
        self.use_counts.clear();
        if self.capture_debug_info {
            self.use_counts.resize(func.num_values(), 0);
            let mut definitions = FxHashMap::<ValueId, BlockId>::default();
            for (block, data) in func.blocks.iter_enumerated() {
                for &inst in &data.instructions {
                    if let Some(result) = func.inst_result_value(inst) {
                        definitions.insert(result, block);
                    }
                }
            }
            // A use in another block than the definition counts twice: the read moves into that
            // block, where LLVM drops the location of the moved code (`dropLocation`), so the
            // read takes the origin of its use like a copy. So does a phi input, which an edge
            // copies when the edge is taken and not where the phi sits.
            let mut count = |operand: ValueId, block: Option<BlockId>| {
                let local = block
                    .is_some_and(|block| definitions.get(&operand).is_none_or(|&def| def == block));
                self.use_counts[operand] += if local { 1 } else { 2 };
            };
            for (block, data) in func.blocks.iter_enumerated() {
                for &inst in &data.instructions {
                    match &func.inst(inst).kind {
                        InstKind::Phi(incoming) => {
                            for &(_, operand) in incoming {
                                count(operand, None);
                            }
                        }
                        kind => kind.visit_operands(|operand| count(operand, Some(block))),
                    }
                }
                if let Some(term) = &data.terminator {
                    term.visit_operands(|operand| count(operand, Some(block)));
                }
            }
            // A copy that rebuilding made stands for one of several uses of its original, so it
            // has no count.
            self.use_counts.truncate(original_values);
        }
        // Spill slots: the spilled values in value order, then the return address.
        self.spill_slots.clear();
        self.spill_slots.extend(plan.spilled.iter().zip(0..));
        self.ret_spill_slot = plan.ret_spilled.then_some(self.spill_slots.len() as u32);
        self.spill_addr_consts.clear();
        self.block_labels.clear();
        let cfg = CfgInfo::new(func);
        let loop_blocks = cfg.cyclic_blocks();
        for block in func.blocks.indices() {
            let label = self.asm.new_label();
            if func.blocks[block].hotness.is_cold() {
                self.asm.mark_label_cold(label);
            }
            if loop_blocks.contains(block) {
                self.asm.mark_label_loop(label);
            }
            self.block_labels.insert(block, label);
        }

        // A block that only jumps on is never emitted; edges into it go to its final target.
        // `-O none` emits every block.
        let mut forward = FxHashMap::default();
        for (block, block_plan) in plan.blocks.iter_enumerated() {
            if !self.optimization.is_none()
                && block != BlockId::ENTRY
                && let Some(block_plan) = block_plan
                && block_plan.steps.is_empty()
                && let Exit::Jump(mut target) = block_plan.exit
            {
                for _ in 0..plan.blocks.len() {
                    match plan.blocks[target].as_ref().map(|plan| (&plan.steps, &plan.exit)) {
                        Some((steps, Exit::Jump(next)))
                            if steps.is_empty() && target != BlockId::ENTRY && target != block =>
                        {
                            target = *next;
                        }
                        _ => break,
                    }
                }
                if target != block {
                    forward.insert(block, target);
                }
            }
        }
        // Emit blocks in an empty cycle as they are.
        let cyclic: Vec<BlockId> = forward
            .iter()
            .filter(|(_, target)| forward.contains_key(*target))
            .map(|(&block, _)| block)
            .collect();
        for block in cyclic {
            forward.remove(&block);
        }
        for (&block, &target) in &forward {
            let label = self.block_labels[&target];
            self.block_labels.insert(block, label);
        }
        // NOTE: A bypassed block's label marked where control entered; its predecessors now enter
        // the target, so the target's label takes that origin, or the union.
        let mut bypassed = FxHashMap::<BlockId, Vec<BlockId>>::default();
        for block in func.blocks.indices() {
            if self.capture_debug_info
                && let Some(&target) = forward.get(&block)
                && !func.blocks[block].predecessors.is_empty()
            {
                bypassed.entry(target).or_default().push(block);
            }
        }
        let bypassed_origins = bypassed
            .iter()
            .filter_map(|(&target, blocks)| {
                let label = |block: BlockId| &func.blocks[block].label_metadata;
                // No bypassed block names a construct, so the target keeps its own label.
                if blocks.iter().all(|&block| label(block).source_spans().next().is_none()) {
                    return None;
                }
                // The blocks of a chain that one bypassed block heads are not entries themselves.
                let heads = blocks
                    .iter()
                    .copied()
                    .filter(|&block| {
                        let predecessors = &func.blocks[block].predecessors;
                        predecessors.iter().any(|pred| !blocks.contains(pred))
                    })
                    .collect::<Vec<_>>();
                let only_bypassed_entries = target != BlockId::ENTRY
                    && func.blocks[target].predecessors.iter().all(|pred| blocks.contains(pred));
                let origin = match heads.split_first() {
                    // The target now stands for the bypassed entries alone.
                    Some((&first, rest)) if only_bypassed_entries => {
                        let mut origin = label(first).clone();
                        origin.merge_debug_locs(rest.iter().map(|&block| label(block)));
                        origin
                    }
                    _ => {
                        let mut origin = label(target).clone();
                        origin.merge_debug_locs(blocks.iter().map(|&block| label(block)));
                        origin
                    }
                };
                Some((target, origin))
            })
            .collect::<FxHashMap<_, _>>();
        let resolve = |block: BlockId| forward.get(&block).copied().unwrap_or(block);

        let mut order = self.block_layout_order(func, &cfg);
        order.retain(|block| !forward.contains_key(block));
        let mut trampolines = Vec::new();
        // The continuations and trampolines of a block in a loop of creation code may run as often
        // as the block, but only the labels of MIR blocks are marked as loop labels.
        let repeating = self.asm.repeating_code;
        let in_loop = |codegen: &Self, block| codegen.in_constructor && loop_blocks.contains(block);
        for (pos, &block) in order.iter().enumerate() {
            let Some(block_plan) = plan.blocks[block].as_ref() else { continue };
            let next = order.get(pos + 1).copied();
            self.asm.repeating_code = repeating || in_loop(self, block);
            let label_origin =
                bypassed_origins.get(&block).unwrap_or(&func.blocks[block].label_metadata);
            if !func.blocks[block].predecessors.is_empty() {
                let label = self.block_labels[&block];
                self.asm.define_label(label);
                self.mark_block_label_origin(label, label_origin);
            }
            self.emit_stackified_block(
                func_id,
                func,
                block,
                label_origin,
                block_plan,
                next,
                &resolve,
                &mut trampolines,
            );
            if self.capture_debug_info {
                self.asm.set_source_span(None);
                self.asm.set_modifier_depth(0);
            }
        }
        for (label, steps, target, source) in trampolines {
            self.asm.repeating_code = repeating || in_loop(self, source);
            self.asm.define_label(label);
            let terminator_origin = Self::terminator_origin(func, source, None);
            self.set_terminator_origin(terminator_origin);
            self.asm.set_label_origin_from_current(label);
            let cold = self.block_code_is_cold(func, target);
            let assignments = self.edge_assignments(func, source, target);
            self.emit_edge_steps(func_id, func, terminator_origin, &steps, assignments, cold);
            self.drop_bypassed_exit_origin(func, source, target);
            self.emit_stackified_jump(target, None);
        }
        self.asm.repeating_code = repeating;
        if self.capture_debug_info {
            self.asm.set_source_span(None);
            self.asm.set_modifier_depth(0);
        }

        let spill_words = self.spill_slots.len() + usize::from(plan.ret_spilled);
        self.function_spill_sizes.insert(func_id, spill_words as u64 * EvmMemoryLayout::WORD_SIZE);
        self.assign_ranked_spill_addrs(func_id);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_stackified_block(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        block: BlockId,
        label_origin: &InstructionMetadata,
        plan: &BlockPlan,
        next: Option<BlockId>,
        resolve: &dyn Fn(BlockId) -> BlockId,
        trampolines: &mut Vec<(Label, Vec<Step>, BlockId, BlockId)>,
    ) {
        let (body, terminator) = plan.steps.split_at(plan.terminator_start);
        let cold = self.block_code_is_cold(func, block);
        let terminator_origin = Self::terminator_origin(func, block, Some(&plan.exit));
        self.set_insertion_origin(func, block, body, &plan.exit);
        let leading = body.iter().take_while(|step| !matches!(step, Step::Begin(_))).count();
        let mut label_spans = label_origin.source_spans();
        let label_span = label_spans.next().filter(|_| label_spans.next().is_none());
        for (index, step) in body.iter().enumerate() {
            // NOTE: rustc gives inserted code the source info of the statement it precedes
            // (`MirPatch`), but a `pop` or return-slot save at a block start ends a value of the
            // construct the block's label closes, as solc pops scope variables under the
            // statement it leaves, so it takes the label origin; the shuffles that prepare the
            // next instruction take that instruction's.
            if self.capture_debug_info
                && index < leading
                && matches!(step, Step::Stack(StackOp::Pop) | Step::SpillRet)
                && let Some(span) = label_span
            {
                let insertion = self.asm.debug_origin();
                self.asm.set_source_span(Some(span));
                self.asm.set_modifier_depth(label_origin.modifier_depth());
                self.emit_stackified_step(func_id, func, step, cold);
                self.asm.set_debug_origin(insertion);
            } else {
                self.emit_block_step(func_id, func, terminator_origin, step, cold);
            }
        }
        self.set_terminator_origin(terminator_origin);
        let assignments = match plan.exit {
            Exit::Jump(target) => self.edge_assignments(func, block, target),
            _ => Vec::new(),
        };
        self.emit_edge_steps(func_id, func, terminator_origin, terminator, assignments, cold);
        let mut edge_label = |codegen: &mut Self, edge: &Edge| match &edge.trampoline {
            None => codegen.block_labels[&edge.target],
            Some(steps) => {
                let label = codegen.new_trampoline_label(func, edge.target);
                trampolines.push((label, steps.clone(), edge.target, block));
                label
            }
        };
        match &plan.exit {
            Exit::Jump(target) => self.emit_stackified_jump(resolve(*target), next),
            Exit::Branch { then_edge, else_edge } => {
                let falls =
                    |edge: &Edge| edge.trampoline.is_none() && next == Some(resolve(edge.target));
                // Fall through to the next block, else into a trampoline, else keep a cold
                // successor out of line; the other successor is the jump target.
                let invert = !falls(else_edge)
                    && (falls(then_edge)
                        || else_edge.trampoline.is_none()
                            && (then_edge.trampoline.is_some()
                                || func.blocks[then_edge.target].hotness.is_cold()
                                    && !func.blocks[else_edge.target].hotness.is_cold()));
                let (taken, fall) =
                    if invert { (else_edge, then_edge) } else { (then_edge, else_edge) };
                // [iszero]; jumpi taken; <fall trampoline>; jump fall
                let label = edge_label(self, taken);
                if invert {
                    // NOTE: With the plan's inversion, this `ISZERO` computes the innermost peeled
                    // `eq`, so it keeps that location, as LLVM keeps the location of a replacement
                    // for one instruction. Without the plan's inversion no instruction stands in
                    // for the `eq`, so its origin stays dropped.
                    if self.capture_debug_info
                        && let Some(zero_test) = plan.zero_test
                    {
                        let terminator_origin = self.asm.debug_origin();
                        self.set_origins(&func.inst(zero_test).metadata);
                        self.asm.emit_op(op::ISZERO);
                        self.asm.set_debug_origin(terminator_origin);
                    } else {
                        self.asm.emit_op(op::ISZERO);
                    }
                }
                self.emit_conditional_jump(label, false);
                let assignments = self.edge_assignments(func, block, fall.target);
                let steps = fall.trampoline.as_deref().unwrap_or_default();
                self.emit_edge_steps(func_id, func, terminator_origin, steps, assignments, cold);
                self.drop_bypassed_exit_origin(func, block, fall.target);
                self.emit_stackified_jump(resolve(fall.target), next);
            }
            Exit::Switch { default, cases, trampolines: switch_trampolines } => {
                // The scrutinee is on top of the words every target expects below it. Targets
                // with trampolines jump to them instead of the block.
                let mut restore = Vec::new();
                for (target, steps) in switch_trampolines {
                    let label = self.new_trampoline_label(func, *target);
                    trampolines.push((label, steps.clone(), *target, block));
                    restore.push((*target, self.block_labels.insert(*target, label).unwrap()));
                }
                let next = next.filter(|next| !restore.iter().any(|(target, _)| target == next));
                self.emit_switch_terminator(func, *default, cases, next);
                for (target, label) in restore {
                    self.block_labels.insert(target, label);
                }
            }
            Exit::Return => {
                // [results, return] -> jump
                self.asm.emit_op(op::JUMP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }
            Exit::TailCall { callee, .. } => {
                // push callee; jump
                self.asm.emit_push_label(self.function_labels[callee]);
                // A `return` or `leave` that follows the call as its own statement owns the jump
                // that leaves this function, as the jump of a plain `return` is its own. The push
                // and the jump are one terminator in EVM IR, so the push takes its origin too.
                if Self::tail_call_follows_statement(func, block, &plan.exit) {
                    self.set_terminator_origin(&func.blocks[block].terminator_metadata);
                }
                self.asm.emit_op(op::JUMP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }
            Exit::Terminal => match func.blocks[block].terminator.as_ref().expect("terminator") {
                Terminator::Revert { .. } => {
                    self.asm.emit_op(op::REVERT);
                    self.mark_debug_function_exit(func, DebugFunctionExit::Revert);
                }
                Terminator::ReturnData { .. } => {
                    self.asm.emit_op(op::RETURN);
                    self.mark_debug_function_exit(func, DebugFunctionExit::Return);
                }
                Terminator::SelfDestruct { .. } => {
                    self.asm.emit_op(op::SELFDESTRUCT);
                    self.mark_debug_function_exit(func, DebugFunctionExit::Return);
                }
                Terminator::Stop => {
                    self.asm.emit_op(op::STOP);
                    self.mark_debug_function_exit(func, DebugFunctionExit::Return);
                }
                Terminator::Invalid => self.asm.emit_op(op::INVALID),
                Terminator::RevertReturndata => self.emit_revert_returndata(),
                Terminator::Return { .. } => self.emit_external_return(func),
                Terminator::Jump(_)
                | Terminator::Branch { .. }
                | Terminator::Switch { .. }
                | Terminator::TailCall { .. } => {
                    unreachable!("terminal exit for a control transfer")
                }
            },
        }
    }

    /// Emits one step of a block's own code, as opposed to a trampoline's, where a
    /// [`Step::BeginTerminator`] switches to `terminator`.
    fn emit_block_step(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        terminator: &InstructionMetadata,
        step: &Step,
        cold: bool,
    ) {
        match step {
            Step::BeginTerminator => self.set_terminator_origin(terminator),
            step => self.emit_stackified_step(func_id, func, step, cold),
        }
    }

    fn emit_stackified_step(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        step: &Step,
        cold: bool,
    ) {
        match step {
            &Step::Stack(op) => self.asm.emit_stack_op(op),
            &Step::Dup(n, _) => self.asm.emit_stack_op(StackOp::Dup(n)),
            &Step::Materialize(value) => self.emit_materialized(func, value),
            &Step::Reload(value) => self.emit_spill_load(func, self.spill_slots[&value]),
            &Step::Spill(value) => {
                // push slot; mstore
                self.emit_spill_slot_addr(func, self.spill_slots[&value]);
                self.asm.emit_op(op::MSTORE);
            }
            Step::SpillRet => {
                // push slot; mstore
                self.emit_spill_slot_addr(func, self.ret_spill_slot.expect("return slot"));
                self.asm.emit_op(op::MSTORE);
            }
            Step::ReloadRet => {
                self.emit_spill_load(func, self.ret_spill_slot.expect("return slot"))
            }
            // push 0
            Step::Filler => self.asm.emit_push(U256::ZERO),
            &Step::Begin(inst) => {
                if self.capture_debug_info {
                    let metadata = &func.inst(inst).metadata;
                    self.set_origins(metadata);
                }
            }
            Step::BeginTerminator => {
                unreachable!("only a block's own code restarts its terminator")
            }
            Step::Shared { value, origins } => {
                if self.capture_debug_info {
                    let current = self.asm.debug_origin();
                    self.set_shared_origins(func, origins);
                    self.emit_materialized(func, *value);
                    self.asm.set_debug_origin(current);
                } else {
                    self.emit_materialized(func, *value);
                }
            }
            &Step::Op(opcode) => self.asm.emit_op(opcode),
            &Step::Inst(inst) => self.emit_stackified_inst(func_id, func, inst),
            &Step::Call { callee, inst } => {
                // push return; push callee; jump; return:
                let return_label = self.asm.new_label();
                if cold {
                    self.asm.mark_label_cold(return_label);
                }
                self.asm.emit_push_label(return_label);
                self.asm.emit_push_label(self.function_labels[&callee]);
                self.asm.emit_op(op::JUMP);
                self.asm.define_continuation_label(return_label);
                // NOTE: The code after the call of a wrapper to its extracted body is the epilogue
                // of the wrapper, which maps to the closing brace, as clang places the epilogue
                // (`CodeGenFunction::FinishFunction(EndLoc)`), so the return tag does too and the
                // header is not a stop of the return, and neither are the scheduler steps that
                // follow the tag before the next instruction begins. The call itself and the other
                // calls of the wrapper, such as the argument decoders, keep the declaration.
                if self.capture_debug_info && func.inst(inst).metadata.returns_to_exit() {
                    self.asm.set_label_origins(return_label, [func.exit_span], 0);
                    self.asm.set_source_spans([func.exit_span]);
                }
            }
            &Step::Publish { callee, arity, params, .. } => {
                // [result(m-1), ..., result1, result0]
                // swap1; push frame[return + 32 * k]; mstore   (k = 1..m)
                // push frame[return]; push 0x20; mstore
                let base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + params as u64 * EvmMemoryLayout::WORD_SIZE;
                for index in 1..arity {
                    self.asm.emit_stack_op(StackOp::Swap(1));
                    let addr = self.static_frame_addr(
                        callee,
                        base + index as u64 * EvmMemoryLayout::WORD_SIZE,
                    );
                    self.asm.emit_push_deferred(addr);
                    self.asm.emit_op(op::MSTORE);
                }
                let addr = self.static_frame_addr(callee, base);
                if self.capture_mir {
                    self.return_buffer_sites.insert(callee, addr);
                }
                self.asm.emit_push_deferred(addr);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MSTORE);
            }
        }
    }

    /// Attributes the code that follows to the origins of `metadata`.
    fn set_origins(&mut self, metadata: &InstructionMetadata) {
        self.asm.set_source_spans(metadata.source_spans());
        self.asm.set_all_source_spans(metadata.all_source_spans());
        self.asm.set_debug_loc_kind(metadata.debug_loc_kind());
        self.asm.set_modifier_depth(metadata.modifier_depth());
    }

    /// Attributes the code that precedes the first instruction that `steps` begin to that
    /// instruction, else to the call that a tail-call `exit` folds in, else to the block's
    /// terminator.
    ///
    /// Scheduler code, such as the pops that clean the stack at the start of a block, belongs to no
    /// instruction of its own. It takes the location of the instruction it precedes, as rustc's
    /// `MirPatch::source_info_for_index` gives inserted code the source info of the statement it
    /// goes before, and LLVM puts a spill or copy at `findDebugLoc` of its insertion point.
    ///
    /// NOTE: With no instruction before a tail call, the insertion point is the tail call, which
    /// LLVM lowers to one `TCRETURN` that keeps the call's `DebugLoc` (`X86ISelLowering.cpp`
    /// `LowerCall`), so the code before it takes the call's origin.
    fn set_insertion_origin(
        &mut self,
        func: &Function,
        block: BlockId,
        steps: &[Step],
        exit: &Exit,
    ) {
        if !self.capture_debug_info {
            return;
        }
        let next = steps.iter().find_map(|step| match step {
            Step::Begin(inst) => Some(&func.inst(*inst).metadata),
            _ => None,
        });
        let metadata = next.unwrap_or(match *exit {
            Exit::TailCall { origin: Some(call), .. } => &func.inst(call).metadata,
            _ => &func.blocks[block].terminator_metadata,
        });
        self.set_origins(metadata);
    }

    /// Returns the origin of the terminator of `block`, or of the call that a tail call `exit`
    /// makes.
    ///
    /// NOTE: LLVM folds a call and the return after it into one tail-call instruction that keeps
    /// the call's `DebugLoc` (`X86ISelLowering.cpp` `LowerCall` builds `X86ISD::TC_RETURN` with the
    /// call's location), and the copies that set up a call's arguments carry the call's location,
    /// so the arguments of a tail call map to the call, not to the `return`. The jump keeps the
    /// call too, unless a statement of its own follows the call, see
    /// [`Self::tail_call_follows_statement`]. The scheduler steps that precede the block's first
    /// instruction take the call's origin too, see [`Self::set_insertion_origin`].
    fn terminator_origin<'f>(
        func: &'f Function,
        block: BlockId,
        exit: Option<&Exit>,
    ) -> &'f InstructionMetadata {
        match exit {
            Some(&Exit::TailCall { origin: Some(call), .. }) => &func.inst(call).metadata,
            _ => &func.blocks[block].terminator_metadata,
        }
    }

    /// Attributes the code that follows to `origin`, as [`Self::terminator_origin`] returns it.
    fn set_terminator_origin(&mut self, origin: &InstructionMetadata) {
        if self.capture_debug_info {
            self.set_origins(origin);
        }
    }

    /// Returns whether `exit` is a tail call whose `return` or `leave` is a statement that does
    /// not enclose the call, as in `f(a); return;`, unlike `return f(a);` or the implicit return
    /// that ends a function, which maps to the closing brace and so is no statement either.
    ///
    /// NOTE: Such a statement would otherwise map to no code, since the tail call takes the call's
    /// location for everything but the jump, and a tail call that layout falls through into its
    /// callee has no jump either; the statement then stays without an entry, as a statement that
    /// emits no code does.
    fn tail_call_follows_statement(func: &Function, block: BlockId, exit: &Exit) -> bool {
        let &Exit::TailCall { origin: Some(call), .. } = exit else { return false };
        let ret = &func.blocks[block].terminator_metadata;
        let call = &func.inst(call).metadata;
        ret.source_spans().next().is_some()
            && !ret.source_spans().all(|ret| is_closing_brace(func, ret))
            && !call.source_spans().all(|call| ret.source_spans().any(|ret| ret.contains(call)))
    }

    /// Emits the code of an edge, giving each push of the value that an assignment of
    /// `assignments` stored the source context of that assignment.
    fn emit_edge_steps(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        terminator: &InstructionMetadata,
        steps: &[Step],
        mut assignments: Vec<(ValueId, &InstructionMetadata)>,
        cold: bool,
    ) {
        for step in steps {
            let origin = match *step {
                Step::Materialize(value) => assignments
                    .iter()
                    .position(|&(stored, _)| stored == value)
                    .map(|index| assignments.remove(index).1),
                // A copy of a value that several assignments stored serves one of them, and no
                // record says which.
                Step::Dup(_, Some(value)) | Step::Reload(value)
                    if assignments.iter().filter(|&&(stored, _)| stored == value).count() == 1 =>
                {
                    assignments
                        .iter()
                        .position(|&(stored, _)| stored == value)
                        .map(|index| assignments.remove(index).1)
                }
                _ => None,
            };
            if let Some(origin) = origin {
                let terminator_origin = self.asm.debug_origin();
                self.set_origins(origin);
                self.emit_stackified_step(func_id, func, step, cold);
                self.asm.set_debug_origin(terminator_origin);
            } else {
                self.emit_block_step(func_id, func, terminator, step, cold);
            }
        }
    }

    /// Returns the records on the terminator of `block` that are exact for its edge to `target`.
    fn edge_records<'f>(
        &self,
        func: &'f Function,
        block: BlockId,
        target: BlockId,
    ) -> Vec<&'f EdgeAssignment> {
        if !self.capture_debug_info {
            return Vec::new();
        }
        let block = &func.blocks[block];
        let jump = matches!(block.terminator, Some(Terminator::Jump(_)));
        let single_edge = match &block.terminator {
            Some(Terminator::Jump(next)) => *next == target,
            Some(Terminator::Branch { then_block, else_block, .. }) => {
                (*then_block == target) != (*else_block == target)
            }
            _ => false,
        };
        if !single_edge {
            return Vec::new();
        }
        (block.terminator_metadata.edge_assignments().iter())
            .filter(
                |assignment| {
                    if jump { assignment.edge.is_none() } else { assignment.edge == Some(target) }
                },
            )
            .collect()
    }

    /// Drops the origin of the jump that ends the edge from `block` to `target` when the edge
    /// replaces the jump of a bypassed arm that has no location.
    ///
    /// NOTE: An empty arm that `cfg-simplify` bypassed leaves its assignments on the edge. The jump
    /// replaces the arm's jump, so it keeps that jump's location, as LLVM's Preserve rule gives a
    /// replacement the location of what it replaces. The jump out of an `if` arm has none, and the
    /// branch's origin would show the condition's line again after the arm ran.
    fn drop_bypassed_exit_origin(&mut self, func: &Function, block: BlockId, target: BlockId) {
        let records = self.edge_records(func, block, target);
        if !records.is_empty() && records.iter().all(|record| record.dropped_exit == Some(true)) {
            self.asm.set_source_span(None);
            self.asm.set_debug_loc_kind(DebugLocKind::Dropped);
        }
    }

    /// Returns the assignments, as the value each stored and its source context, whose pushes and
    /// copies the code of the edge from `block` to `target` may carry.
    ///
    /// NOTE: The code of an edge runs exactly when control takes the edge. An assignment that ran
    /// in `block` runs once per run of `block`, so it matches an edge out of a jump: the push or
    /// `DUP` of its value runs exactly when the assignment does and may carry its span, as a copy
    /// carries the location of the instruction it replaces. An edge out of a branch keeps the
    /// origin of its terminator unless the assignment is exact for that edge, as one that a
    /// bypassed arm left on it is ([`InstructionMetadata::edge_assignments`]), and the branch has
    /// no other edge to `target`. A push is matched by its value. A `DUP` or a reload, which
    /// [`Step::Dup`] and [`Step::Reload`] name by the value they copy, is matched only when one
    /// assignment stored the value, since the planner does not record which phi a copy serves. A
    /// word that the edge moves with a `SWAP` or a `POP`, as in `a = b; b = c`, has no copy to
    /// map.
    fn edge_assignments<'f>(
        &self,
        func: &'f Function,
        block: BlockId,
        target: BlockId,
    ) -> Vec<(ValueId, &'f InstructionMetadata)> {
        (self.edge_records(func, block, target).into_iter())
            .map(|assignment| (assignment.value, &assignment.origin))
            .collect()
    }

    /// Attributes the push of a retained constant to `origins`, the combined origin of its readers
    /// or else the origin of the code it precedes.
    ///
    /// NOTE: The push runs once and serves each reader, so it is shared code: the location of one
    /// reader would be arbitrary, and the combined location of all of them is correct only
    /// when every reader runs whenever the push does, and then they all hold for it. A push hoisted
    /// out of a branch arm or a loop body runs on paths where a reader never does, and a reader's
    /// span on it would let coverage count a skipped statement as executed. It then takes the
    /// origin of the code it precedes, as scheduler-placed code does in `set_insertion_origin`;
    /// that code runs exactly when the push does. LLVM's `Instruction::updateLocationAfterHoist`
    /// drops the location of a hoisted instruction for the same reason, and `ConstantHoisting`
    /// merges the locations of a constant's users.
    fn set_shared_origins(&mut self, func: &Function, origins: &[Reader]) {
        let metadata = |reader: &Reader| match *reader {
            Reader::Inst(inst) => &func.inst(inst).metadata,
            Reader::Terminator(block) => &func.blocks[block].terminator_metadata,
        };
        let (first, rest) = origins.split_first().expect("a retained push has an origin");
        let mut combined = metadata(first).clone();
        combined.combine_debug_locs(rest.iter().map(metadata));
        self.set_origins(&combined);
    }

    /// Jumps to `target` unless it is the next block in the layout.
    fn emit_stackified_jump(&mut self, target: BlockId, next: Option<BlockId>) {
        if next != Some(target) {
            // push target; jump
            self.asm.emit_push_label(self.block_labels[&target]);
            self.asm.emit_op(op::JUMP);
        } else if self.capture_debug_info {
            self.asm.mark_fallthrough();
        }
    }

    /// Recomputes `value` at its use. A read without operands that has several uses takes the
    /// origins of the use, and any other value keeps the origins of the instruction that defines
    /// it.
    fn emit_materialized(&mut self, func: &Function, value: ValueId) {
        self.emit_materialized_in(func, value, true);
    }

    /// Recomputes `value`, an operand of a value that is emitted once when `within_single_use`.
    fn emit_materialized_in(&mut self, func: &Function, value: ValueId, within_single_use: bool) {
        let single_use = within_single_use && self.use_counts.get(value) == Some(&1);
        if self.capture_debug_info
            && let Value::Inst(inst) = func.value(value)
            && let metadata = &func.inst(*inst).metadata
            && metadata.source_spans().next().is_some()
            && let consumer = self.asm.debug_origin()
            && self.keeps_definition_origins(func, value, metadata, &consumer, single_use)
        {
            // A recomputed value is the same computation at every use, so it keeps the definition's
            // origins, as LLVM's `TargetInstrInfo::reMaterialize` clones the original with its
            // `DebugLoc`.
            self.asm.set_source_spans(metadata.source_spans());
            self.asm.set_all_source_spans(metadata.all_source_spans());
            self.asm.set_modifier_depth(metadata.modifier_depth());
            self.emit_recomputed(func, value, single_use);
            self.asm.set_debug_origin(consumer);
        } else {
            self.emit_recomputed(func, value, single_use);
        }
    }

    /// Returns whether `value`, recomputed under the origins `consumer`, keeps the origins of its
    /// definition. `single_use` tells that the value is emitted once, because it and every value
    /// that recomputes it have a single use.
    ///
    /// A read without operands, such as a calldata parameter or `msg.sender`, with several uses is
    /// re-executed at each use like an immediate, so it takes the user's origins, as LLVM gives a
    /// per-use materialized constant the user's location and a reload none of its own. The
    /// definition's origins would send the debugger back to the parameter list or to an earlier
    /// statement at every use. A read with a single use in its own block is not such a copy: it
    /// runs once and only moves within its block, and LLVM keeps the `DebugLoc` of an instruction
    /// that moves within its block (`HowToUpdateDebugInfo`, "Preserve"). Its use can be code the
    /// optimizer moved into the statement, such as the body of an inlined callee that reads the
    /// argument, and the statement of the read would be left with no instruction. A use in another
    /// block, such as the body of a loop, counts as a second use: LLVM drops the location of code
    /// that moves to another block. A definition inside the use is the
    /// more precise origin of this very read, so it stays. An unoptimized build also keeps a
    /// definition that is its own item, such as the initializer of a local, because the read may
    /// be the only instruction coverage can anchor that item to. A parameter read, whose definition
    /// is the whole function, takes the use's origins whatever its uses, or the function header
    /// would be the stop of every use, including a single use in code inlined into the function.
    ///
    /// The use's origins apply because the read is re-executed as part of evaluating the use. An
    /// implicit function exit, which maps to the closing brace of the body, evaluates no
    /// expression of its own: it only returns the named return variables. A read recomputed into
    /// its return encoding carries the value of the statement that read it, and LLVM's
    /// `TargetInstrInfo::reMaterialize` clone keeps the definition's location there. So at that
    /// exit every definition keeps its origins, or the statement of the read could be left with no
    /// instruction.
    fn keeps_definition_origins(
        &self,
        func: &Function,
        value: ValueId,
        definition: &InstructionMetadata,
        consumer: &Metadata,
        single_use: bool,
    ) -> bool {
        let definition_in_use = || {
            definition
                .source_spans()
                .all(|def| consumer.source_spans().any(|user| user.contains(def)))
        };
        let use_in_definition = || {
            consumer
                .source_spans()
                .all(|user| definition.source_spans().any(|def| def.contains(user)))
        };
        let at_implicit_exit =
            || consumer.only_source_span().is_some_and(|user| is_closing_brace(func, user));
        // A parameter read maps to the definition of its function. The dispatcher's entry holds the
        // routes it inlined and has no span to tell their parameters by, so any read of it passes
        // for one.
        let is_parameter_read = || {
            func.declaration_span.is_dummy()
                || definition.source_span() == Some(func.declaration_span)
        };
        !is_operandless_read(func, value)
            || !consumer.has_source_spans()
            || definition_in_use()
            || at_implicit_exit()
            || (self.optimization.is_none() || single_use && !is_parameter_read())
                && !use_in_definition()
    }

    /// Emits the opcodes that compute `value` under the current origins.
    fn emit_recomputed(&mut self, func: &Function, value: ValueId, single_use: bool) {
        match func.value(value) {
            Value::Immediate(imm) => self.asm.emit_push(imm.as_u256().unwrap_or_default()),
            Value::Undef(_) => self.asm.emit_push(U256::ZERO),
            Value::Inst(inst) => match &func.inst(*inst).kind {
                InstKind::DataSize(size) => self.asm.emit_push_data_size(*size),
                InstKind::Gas => self.asm.emit_op(op::GAS),
                InstKind::Sub(..) if let Some((_, reserve)) = gas_minus(func, value) => {
                    self.emit_gas_minus(reserve);
                }
                InstKind::Zext(operand)
                | InstKind::PtrToInt(operand, _)
                | InstKind::IntToPtr(operand) => {
                    self.emit_materialized_in(func, *operand, single_use)
                }
                InstKind::Load(offset, PtrLocation::Calldata) => {
                    // <offset>; calldataload
                    self.emit_materialized_in(func, *offset, single_use);
                    self.asm.emit_op(op::CALLDATALOAD);
                }
                kind => {
                    if let Some(opcode) = rematerializable_nullary_value(func, value) {
                        self.asm.emit_op(opcode);
                        return;
                    }
                    // A recomputed value: its operands in push order, then its opcode.
                    let opcode = kind.evm_opcode().expect("recomputed value has an opcode");
                    for &operand in kind.operands().iter().rev() {
                        self.emit_materialized_in(func, operand, single_use);
                    }
                    self.asm.emit_op(opcode);
                }
            },
            Value::Arg(_) | Value::Error(_) => unreachable!("value {value:?} is not materialized"),
        }
    }

    fn emit_stackified_inst(&mut self, func_id: FunctionId, func: &Function, inst: InstId) {
        match &func.inst(inst).kind {
            InstKind::Alloc { size, .. } => {
                let size =
                    func.value_u64(*size).expect("deferred allocation must have a constant size");
                let alloc = self.asm.emit_deferred_alloc();
                if self.capture_mir {
                    self.allocation_sites.insert(alloc, (func_id, inst));
                }
                self.pending_static_allocs.entry(func_id).or_default().push((alloc, size));
            }
            InstKind::LibraryAddress(library) => self.asm.emit_push_library(*library),
            InstKind::LoadImmutable(id) => self.emit_load_immutable(*id),
            InstKind::ConstructorArgsBase => self.emit_constructor_args_base(),
            InstKind::ConstructorArgsEnd => self.emit_constructor_args_end(),
            InstKind::InternalFrameAddr(offset) => self.emit_own_frame_addr(*offset),
            InstKind::HeapFloor => self.emit_heap_floor(func_id),
            InstKind::Coverage(id) => {
                // push id; coverage
                self.asm.emit_push(U256::from(*id));
                self.asm.keep_last_with_next();
                self.asm.emit_op(op::COVERAGE);
            }
            InstKind::DataCopy(data, ..) => {
                // [size, dest] -> push_data data; swap1; codecopy
                self.asm.emit_push_data(*data);
                self.asm.emit_stack_op(StackOp::Swap(1));
                self.asm.emit_op(op::CODECOPY);
            }
            InstKind::Select(..) => {
                // [f, c, t] -> [f, c, f, t] -> [f, c, t-f] -> [f, c*(t-f)] -> [f + c*(t-f)]
                self.asm.emit_stack_op(StackOp::Dup(3));
                self.asm.emit_stack_op(StackOp::Swap(1));
                self.asm.emit_op(op::SUB);
                self.asm.emit_op(op::MUL);
                self.asm.emit_op(op::ADD);
            }
            kind => unreachable!("instruction `{}` has no custom stack lowering", kind.mnemonic()),
        }
    }
}

/// Returns whether `span` is the closing brace of `func`, where an implicit exit maps.
fn is_closing_brace(func: &Function, span: Span) -> bool {
    span.hi() == func.declaration_span.hi() && span.lo() + 1 == span.hi()
}

/// Returns whether `value` is a read that takes no operands, possibly cast: a nullary environment
/// read or a calldata load at a constant offset.
fn is_operandless_read(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst) = *func.value(value) else { return false };
    match &func.inst(inst).kind {
        InstKind::Zext(operand) | InstKind::PtrToInt(operand, _) | InstKind::IntToPtr(operand) => {
            is_operandless_read(func, *operand)
        }
        InstKind::Load(offset, PtrLocation::Calldata) => {
            matches!(func.value(*offset), Value::Immediate(_))
        }
        _ => rematerializable_nullary_value(func, value).is_some(),
    }
}
