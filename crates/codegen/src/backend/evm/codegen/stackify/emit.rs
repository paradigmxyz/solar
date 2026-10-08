//! Emission of planned stack-resident functions into EVM IR.

use super::{BlockPlan, Edge, Exit, FunctionPlan, ModuleInfo, Step};
use crate::{
    backend::{
        assembler::Label,
        evm::{
            DebugFunctionExit,
            codegen::{EvmCodegen, StackModel, StackOp, stack::rematerializable_nullary_value},
            op::{self, WORD_BYTES},
        },
    },
    mir::{
        BlockId, Function, FunctionId, InstId, InstKind, Module, Terminator, Value, ValueId,
        analysis::{CallGraphInfo, CfgInfo},
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};

impl<'gcx> EvmCodegen<'gcx> {
    pub(super) fn emit_runtime_stackified(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        info: &ModuleInfo,
        mut plans: IndexVec<FunctionId, Option<FunctionPlan>>,
    ) {
        let entry = info.entry;
        // The switch planner prices tail calls to empty bodies as shared terminals.
        self.collect_empty_stop_functions(module);
        // Spill words and frame objects of non-recursive internal functions live at fixed
        // addresses; recursive functions own no frame.
        for func_id in info.internal.iter() {
            if !call_graph.is_recursive(func_id) {
                self.static_frame_functions.insert(func_id);
            }
        }
        self.runtime_stack_args = true;
        for (func_id, plan) in plans.iter_enumerated() {
            if plan.is_some() && func_id != entry {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }
        }

        self.record_runtime_entry_reachability(call_graph, entry);
        self.in_internal_function = false;
        self.emitting_entry = info.emitting_entry;
        let plan = plans[entry].take().expect("entry plan");
        self.emit_stackified_body(entry, &module.functions[entry], plan);
        self.emitting_entry = false;
        self.runtime_entry_funcs.push(entry);

        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry || info.internal.contains(func_id) {
                continue;
            }
            let Some(plan) = plans[func_id].take() else { continue };
            self.asm.define_label(self.function_labels[&func_id]);
            self.mark_debug_function_invoke(func);
            self.in_internal_function = false;
            self.emit_entry_free_memory_start(module, call_graph, func_id);
            self.emit_stackified_body(func_id, func, plan);
            self.runtime_entry_funcs.push(func_id);
        }

        for func_id in info.internal.iter() {
            let Some(plan) = plans[func_id].take() else { continue };
            let func = &module.functions[func_id];
            self.asm.define_label(self.function_labels[&func_id]);
            self.mark_debug_function_invoke(func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.emit_stackified_body(func_id, func, plan);
            self.in_internal_function = false;
            self.current_internal_function = None;
        }

        self.pack_scalar_static_frames(module);
        self.resolve_static_frames(module);
    }

    fn emit_stackified_body(&mut self, func_id: FunctionId, func: &Function, plan: FunctionPlan) {
        self.scheduler.reset();
        self.spill_addr_consts.clear();
        self.spill_stores.clear();
        self.spill_loads.clear();
        for value in plan.spilled.iter() {
            self.scheduler.spills.reserve(value);
        }
        self.cold_blocks = self.collect_cold_blocks(func);
        self.block_labels.clear();
        let cfg = CfgInfo::new(func);
        let loop_blocks = cfg.cyclic_blocks();
        for block in func.blocks.indices() {
            let label = self.asm.new_label();
            if self.block_is_cold(block) {
                self.asm.mark_label_cold(label);
            }
            if loop_blocks.contains(block) {
                self.asm.mark_label_loop(label);
            }
            self.block_labels.insert(block, label);
        }

        // A block that only jumps on is never emitted; edges into it go to its final target.
        let mut forward = FxHashMap::default();
        for (block, block_plan) in plan.blocks.iter_enumerated() {
            if block != BlockId::ENTRY
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
        // Blocks in an empty cycle are emitted as they are.
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
        let resolve = |block: BlockId| forward.get(&block).copied().unwrap_or(block);

        let mut order = self.block_layout_order(func, &cfg);
        order.retain(|block| !forward.contains_key(block));
        let mut trampolines: Vec<(Label, Vec<Step>, BlockId)> = Vec::new();
        for (pos, &block) in order.iter().enumerate() {
            let Some(block_plan) = plan.blocks[block].as_ref() else { continue };
            let next = order.get(pos + 1).copied();
            if !func.blocks[block].predecessors.is_empty() {
                self.asm.define_label(self.block_labels[&block]);
            }
            if self.capture_debug_info {
                let depth = func.blocks[block]
                    .instructions
                    .first()
                    .map(|&inst| func.inst(inst).metadata.modifier_depth())
                    .unwrap_or(0);
                self.asm.set_modifier_depth(depth);
            }
            self.emit_stackified_block(
                func_id,
                func,
                block,
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
        for (label, steps, target) in trampolines {
            self.asm.define_label(label);
            for step in &steps {
                self.emit_stackified_step(func_id, func, step);
            }
            self.emit_stackified_jump(target, None);
        }

        self.function_stack_peaks.insert(func_id, plan.peak);
        self.record_function_spill_size(func_id);
        self.assign_ranked_spill_addrs(func_id);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_stackified_block(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        block: BlockId,
        plan: &BlockPlan,
        next: Option<BlockId>,
        resolve: &dyn Fn(BlockId) -> BlockId,
        trampolines: &mut Vec<(Label, Vec<Step>, BlockId)>,
    ) {
        for step in &plan.steps {
            self.emit_stackified_step(func_id, func, step);
        }
        let mut edge_label = |codegen: &mut Self, edge: &Edge| match &edge.trampoline {
            None => codegen.block_labels[&edge.target],
            Some(steps) => {
                let label = codegen.asm.new_label();
                trampolines.push((label, steps.clone(), edge.target));
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
                                || self.block_is_cold(then_edge.target)
                                    && !self.block_is_cold(else_edge.target)));
                let (taken, fall) =
                    if invert { (else_edge, then_edge) } else { (then_edge, else_edge) };
                // [iszero]; jumpi taken; <fall trampoline>; jump fall
                if invert {
                    self.asm.emit_op(op::ISZERO);
                }
                let label = edge_label(self, taken);
                self.asm.emit_push_label(label);
                self.asm.emit_op(op::JUMPI);
                for step in fall.trampoline.iter().flatten() {
                    self.emit_stackified_step(func_id, func, step);
                }
                self.emit_stackified_jump(resolve(fall.target), next);
            }
            Exit::Switch { value, default, cases, trampolines: switch_trampolines } => {
                // The switch emitter reads the physical stack model: the scrutinee on top of
                // words it must preserve for every target. Targets with trampolines jump to
                // them instead of the block.
                let mut restore = Vec::new();
                for (target, steps) in switch_trampolines {
                    let label = self.asm.new_label();
                    trampolines.push((label, steps.clone(), *target));
                    restore.push((*target, self.block_labels.insert(*target, label).unwrap()));
                }
                let next = next.filter(|next| !restore.iter().any(|(target, _)| target == next));
                let mut stack = StackModel::new();
                stack.push(*value);
                self.scheduler.stack = stack;
                self.emit_switch_terminator(func, *value, *default, cases, next, true);
                self.scheduler.clear_stack();
                for (target, label) in restore {
                    self.block_labels.insert(target, label);
                }
            }
            Exit::Return => {
                self.asm.emit_op(op::JUMP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }
            Exit::TailCall(callee) => {
                self.asm.emit_push_label(self.function_labels[callee]);
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
                Terminator::RevertReturndata => {
                    self.emit_revert_returndata();
                    self.scheduler.clear_stack();
                }
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

    fn emit_stackified_step(&mut self, func_id: FunctionId, func: &Function, step: &Step) {
        match *step {
            Step::Stack(op) => self.asm.emit_stack_op(op),
            Step::Materialize(value) => self.emit_materialized(func, value),
            Step::Reload(value) => {
                let slot = self.scheduler.spills.get(value).expect("reserved spill slot");
                self.emit_spill_load(func, slot);
            }
            Step::Spill(value) => {
                // push slot; mstore
                let slot = self.scheduler.spills.get(value).expect("reserved spill slot");
                self.emit_spill_slot_addr(func, slot);
                self.asm.emit_op(op::MSTORE);
            }
            Step::Filler => self.asm.emit_push(U256::ZERO),
            Step::Begin(inst) => {
                if self.capture_debug_info {
                    let metadata = &func.inst(inst).metadata;
                    self.asm.set_source_spans(metadata.source_spans());
                    self.asm.set_modifier_depth(metadata.modifier_depth());
                }
            }
            Step::Op(opcode) => self.asm.emit_op(opcode),
            Step::Inst(inst) => self.emit_stackified_inst(func_id, func, inst),
            Step::Call(callee) => {
                // push return; push callee; jump; return:
                let return_label = self.asm.new_label();
                self.asm.emit_push_label(return_label);
                self.asm.emit_push_label(self.function_labels[&callee]);
                self.asm.emit_op(op::JUMP);
                self.asm.define_continuation_label(return_label);
            }
            Step::Publish { callee, arity, params } => {
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
                self.asm.emit_push_deferred(addr);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MSTORE);
            }
        }
    }

    /// Jumps to `target` unless it is the next block in the layout.
    fn emit_stackified_jump(&mut self, target: BlockId, next: Option<BlockId>) {
        if next != Some(target) {
            self.asm.emit_push_label(self.block_labels[&target]);
            self.asm.emit_op(op::JUMP);
        }
    }

    fn emit_materialized(&mut self, func: &Function, value: ValueId) {
        match func.value(value) {
            Value::Immediate(imm) => self.asm.emit_push(imm.as_u256().unwrap_or_default()),
            Value::Undef(_) => self.asm.emit_push(U256::ZERO),
            Value::Inst(inst) => match &func.inst(*inst).kind {
                InstKind::DataSize(size) => self.asm.emit_push_data_size(*size),
                InstKind::Gas => self.asm.emit_op(op::GAS),
                InstKind::Sub(_, reserve) => {
                    self.emit_gas_minus(func.value_u256(*reserve).expect("constant gas reserve"));
                }
                _ => {
                    let opcode = rematerializable_nullary_value(func, value)
                        .expect("materialized instruction is a stable read");
                    self.asm.emit_op(opcode);
                }
            },
            Value::Arg(index) => {
                // External arguments: push 4 + 32 * index; calldataload
                let offset = 4 + (index.index() as u64) * WORD_BYTES as u64;
                self.asm.emit_push(U256::from(offset));
                self.asm.emit_op(op::CALLDATALOAD);
            }
            Value::Error(_) => unreachable!("value {value:?} is not materialized"),
        }
    }

    fn emit_stackified_inst(&mut self, func_id: FunctionId, func: &Function, inst: InstId) {
        match &func.inst(inst).kind {
            InstKind::Alloc { size, .. } => {
                let size =
                    func.value_u64(*size).expect("deferred allocation must have a constant size");
                let alloc = self.asm.emit_deferred_alloc();
                self.pending_static_allocs.entry(func_id).or_default().push((alloc, size));
            }
            InstKind::LibraryAddress(library) => self.asm.emit_push_library(*library),
            InstKind::LoadImmutable(id) => self.emit_load_immutable(*id),
            InstKind::InternalFrameAddr(offset) => self.emit_own_frame_addr(*offset),
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
