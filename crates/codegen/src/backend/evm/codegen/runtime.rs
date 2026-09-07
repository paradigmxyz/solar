//! Runtime emission, retry policies, and whole-program stack limits.

use super::{
    ArtifactKind, BlockId, CallGraphInfo, DenseBitSet, EvmCodegen, FunctionId, GeneratedCode,
    IndexVec, MAX_STACK_DEPTH, MirPhase, Module, OptimizationMode, Terminator, index_vec,
    run_pipeline,
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Runs the canonical MIR optimization pipeline on the module.
    pub(super) fn run_optimization_passes(&mut self, module: &mut Module) {
        let _changed = run_pipeline(self.gcx, module, None);
    }

    /// Generates runtime bytecode for a module.
    pub(super) fn generate_runtime_code(
        &mut self,
        module: &crate::mir::LoweredModule<'_>,
        call_graph: &CallGraphInfo,
    ) -> GeneratedCode {
        assert_eq!(
            module.phase(),
            MirPhase::Lowered,
            "EVM codegen requires MIR in the final phase"
        );
        let runtime_code_size_limit = self.gcx.sess.opts.evm_version.runtime_code_size_limit();
        let may_need_code_size_rescue = self.gcx.sess.opts.optimization.is_gas();
        let mut code_size_rescue = false;
        let mut gas_first_result = None;
        loop {
            let mut preserve_caller_stack =
                !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None);
            let mut runtime_stack_args = true;
            let mut stack_returns_enabled = true;
            self.disabled_stack_only_functions.clear_to(module.functions.len());
            loop {
                let disabled_stack_only_functions = self.disabled_stack_only_functions.count();
                self.reset_runtime_codegen(module);
                self.preserve_caller_stack = preserve_caller_stack;
                self.runtime_stack_args = runtime_stack_args;
                self.stack_returns_enabled = stack_returns_enabled;

                if !module.functions.is_empty() {
                    self.emit_runtime(module, call_graph);
                }

                if self.disabled_stack_only_functions.count() > disabled_stack_only_functions {
                    continue;
                }
                let stack_fits = self.caller_stack_prefixes_fit(module, MAX_STACK_DEPTH);
                if !stack_fits && !self.icall_stack_edges.is_empty() {
                    if preserve_caller_stack {
                        preserve_caller_stack = false;
                        continue;
                    }
                    if runtime_stack_args {
                        runtime_stack_args = false;
                        continue;
                    }
                    if stack_returns_enabled {
                        stack_returns_enabled = false;
                        continue;
                    }
                }
                if !stack_fits {
                    self.report_stack_limit_error();
                }
                break;
            }

            self.asm.set_enable_size_outlining(code_size_rescue);

            let result =
                self.asm.assemble_with_captures(self.capture_evm_ir, self.capture_debug_info);
            if may_need_code_size_rescue
                && !code_size_rescue
                && let Some(limit) = runtime_code_size_limit
                && result.bytecode.len() > limit
                && result.bytecode.len() <= limit * 2
            {
                gas_first_result = Some(result);
                code_size_rescue = true;
                continue;
            }
            let result = if code_size_rescue
                && result.bytecode.len()
                    > runtime_code_size_limit.expect("code-size rescue requires a size limit")
            {
                gas_first_result.take().expect("code-size rescue must retain the gas-first runtime")
            } else {
                result
            };
            self.runtime_immutable_refs = result.immutable_refs;
            return GeneratedCode {
                bytecode: result.bytecode,
                evm_ir: result.evm_ir,
                debug_info: result.debug_info,
            };
        }
    }

    fn reset_runtime_codegen(&mut self, module: &Module) {
        self.function_return_counts =
            module.functions.iter().map(|func| func.returns.len()).collect();
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Runtime);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        self.block_labels.clear();
        self.function_labels.clear();
        self.empty_stop_functions.clear_to(module.functions.len());
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.restorable_internal_frames.clear_to(module.functions.len());
        self.static_frame_functions.clear_to(module.functions.len());
        self.static_frame_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.pending_static_allocs.clear();
        self.runtime_free_memory_consts.clear();
        self.runtime_entry_reachability.clear();
        self.runtime_entry_funcs.clear();
        self.current_internal_function = None;
        self.block_copies.clear();
        self.stack_phi_sources.clear();
        self.static_call_abis.clear();
        self.recursive_stack_functions.clear_to(module.functions.len());
        self.recursive_frame_functions.clear_to(module.functions.len());
        self.recursive_frame_edges.clear();
        self.recursion_reaching_functions.clear_to(module.functions.len());
        self.function_stack_peaks.clear();
        self.icall_stack_edges.clear();
        self.runtime_stack_args = true;
        self.stack_returns_enabled = true;
        self.emitting_entry = false;
        self.reset_switch_gas_code_growth();
    }

    /// Validates the complete physical stack, including words intentionally
    /// hidden below each function's scheduler model. The local high-water
    /// marks are exact for the emitted bodies; call-edge propagation is
    /// conservative for tail calls, which may carry any locally observed
    /// stack into their target. Recursive regions are excluded from the
    /// optimization before emission because their incoming prefix is
    /// intentionally unbounded.
    pub(super) fn caller_stack_prefixes_fit(
        &self,
        module: &Module,
        max_stack_depth: usize,
    ) -> bool {
        let Some(entry_id) = module.dispatch_entry() else {
            return self.function_stack_peaks.values().all(|&peak| peak <= max_stack_depth);
        };

        self.stack_prefixes_fit_from(module, entry_id, max_stack_depth)
    }

    pub(super) fn report_stack_limit_error(&self) {
        self.gcx
            .dcx()
            .err(format!(
                "codegen cannot keep the generated EVM stack within {MAX_STACK_DEPTH} words"
            ))
            .emit();
    }

    pub(super) fn stack_prefixes_fit_from(
        &self,
        module: &Module,
        entry_id: FunctionId,
        max_stack_depth: usize,
    ) -> bool {
        if self.function_stack_peaks.values().any(|&peak| peak > max_stack_depth) {
            return false;
        }

        let mut incoming: IndexVec<FunctionId, Option<usize>> =
            index_vec![None; module.functions.len()];
        incoming[entry_id] = Some(0);
        for _ in 0..module.functions.len() {
            let mut changed = false;
            for edge in &self.icall_stack_edges {
                if self.recursive_stack_functions.contains(edge.caller)
                    || self.recursive_stack_functions.contains(edge.callee)
                    || !self.function_stack_peaks.contains_key(&edge.callee)
                {
                    continue;
                }
                let Some(base) = incoming[edge.caller] else { continue };
                let candidate = base.saturating_add(edge.preserved_words).saturating_add(1);
                // Before JUMP consumes its destination, the caller briefly holds the preserved
                // prefix, return address, complete argument tuple, and target label. Arguments
                // become part of the callee's modeled stack (or are consumed by its prologue), so
                // only the preserved prefix and return address propagate as its hidden prefix.
                let entry_transient = edge.argument_words.saturating_add(1);
                // After the callee returns, multiword stack-return adoption stages the
                // buffer pointer and one address above the returned tuple, peaking at
                // `base + preserved + arity + 2` = `candidate + arity + 1`.
                let adoption_transient = self
                    .stack_return_plan(edge.callee)
                    .map_or(0, |plan| if plan.arity > 1 { plan.arity + 1 } else { 0 });
                if candidate.saturating_add(entry_transient.max(adoption_transient))
                    > max_stack_depth
                {
                    return false;
                }
                if incoming[edge.callee].is_none_or(|current| candidate > current) {
                    incoming[edge.callee] = Some(candidate);
                    changed = true;
                }
            }

            for (caller, func) in module.functions.iter_enumerated() {
                if self.recursive_stack_functions.contains(caller) {
                    continue;
                }
                let Some(base) = incoming[caller] else { continue };
                let carried = self.function_stack_peaks.get(&caller).copied().unwrap_or(0);
                for block in &func.blocks {
                    let Some(Terminator::TailCall { function: callee, .. }) = &block.terminator
                    else {
                        continue;
                    };
                    if self.recursive_stack_functions.contains(*callee)
                        || !self.function_stack_peaks.contains_key(callee)
                    {
                        continue;
                    }
                    let candidate = base.saturating_add(carried);
                    // Tail calls carry the caller stack and briefly push only
                    // the target label; they do not add a return address.
                    if candidate.saturating_add(1) > max_stack_depth {
                        return false;
                    }
                    if incoming[*callee].is_none_or(|current| candidate > current) {
                        incoming[*callee] = Some(candidate);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        incoming.iter_enumerated().all(|(func_id, incoming)| {
            incoming.is_none_or(|incoming| {
                incoming
                    .saturating_add(self.function_stack_peaks.get(&func_id).copied().unwrap_or(0))
                    <= max_stack_depth
            })
        })
    }

    /// Emits a runtime from final-phase MIR.
    ///
    /// Selector matching, receive/fallback routing, and callvalue checks all
    /// live in the MIR `entry`, whose `tail_call`s jump to the ABI wrappers.
    fn emit_runtime(&mut self, module: &Module, call_graph: &CallGraphInfo) {
        let Some(entry_id) = module.dispatch_entry() else {
            assert!(
                !module.functions.iter().any(Self::is_external_entry),
                "evm-shaped module with a runtime interface must have a MIR `entry` function"
            );
            return;
        };

        let mut classified_recursive_frames = DenseBitSet::new_empty(module.functions.len());
        for (root, _) in module.functions.iter_enumerated() {
            if !call_graph.is_recursive(root) || classified_recursive_frames.contains(root) {
                continue;
            }
            let component = call_graph.recursive_component(root);
            classified_recursive_frames.union(&component);
            let supported = if component.count() == 1 {
                Self::uses_reentrant_static_frame(root, &module.functions[root])
            } else {
                // The validated mutual-recursion shape is JSON-style Yul:
                // each helper returns a two-word tuple and recursive edges go
                // through another component member. A direct multi-return
                // self edge could overwrite child results while restoring the
                // suspended copy of that same frame, so keep it dynamic.
                component.iter().all(|func_id| {
                    let func = &module.functions[func_id];
                    func.attributes.is_yul
                        && func.returns.len() == 2
                        && Self::static_frame_offsets_are_local(func)
                        && !Self::has_direct_self_call(func_id, func)
                })
            };
            if supported {
                self.recursive_frame_functions.union(&component);
                for caller in component.iter() {
                    for callee in component.iter() {
                        self.recursive_frame_edges.insert((caller, callee));
                    }
                }
            }
        }

        for (func_id, func) in module.functions.iter_enumerated() {
            if func.blocks.len() == 1
                && func.blocks[BlockId::ENTRY].instructions.is_empty()
                && matches!(func.blocks[BlockId::ENTRY].terminator, Some(Terminator::Stop))
            {
                self.empty_stop_functions.insert(func_id);
            }
            if call_graph.is_recursive(func_id) {
                self.recursive_stack_functions.insert(func_id);
                self.recursive_stack_functions.union(&call_graph.reachable_callees_from([func_id]));
            }
            if call_graph.is_recursive(func_id)
                || call_graph
                    .reachable_callees_from([func_id])
                    .iter()
                    .any(|callee| call_graph.is_recursive(callee))
            {
                self.recursion_reaching_functions.insert(func_id);
            }
        }
        let internal_targets = call_graph.reachable_callees_from(
            module.functions.iter_enumerated().filter_map(|(func_id, func)| {
                (func_id == entry_id || Self::is_external_entry(func)).then_some(func_id)
            }),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            if !func.attributes.may_return_memory
                && !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference())
            {
                self.restorable_internal_frames.insert(func_id);
            }
            // Internal functions get compile-time-fixed frames. Recursive
            // activations reuse their function's scratch frame after carrying
            // the suspended activation's live state on the EVM stack.
            if func_id != entry_id
                && !Self::is_external_entry(func)
                && Self::is_runtime_function(func)
                && (!call_graph.is_recursive(func_id)
                    || self.recursive_frame_functions.contains(func_id))
                && Self::static_frame_offsets_are_local(func)
            {
                self.static_frame_functions.insert(func_id);
            }
        }
        if self.runtime_stack_args {
            self.compute_stack_arg_masks(module);
            let stack_arg_values = self.collect_canonical_stack_arg_values(module);
            self.compute_resident_stack_args(module, &stack_arg_values);
            let stack_arg_uses = self.collect_stack_arg_uses(module);
            self.compute_lazy_stack_args(module, &stack_arg_values, &stack_arg_uses);
            self.compute_direct_stack_args(module, &stack_arg_values, &stack_arg_uses);
        }
        self.compute_stack_return_plans(module);
        // Labels for every tail-call and internal-call target.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id {
                continue;
            }
            let needs_body = Self::is_external_entry(func)
                || (Self::is_runtime_function(func) && internal_targets.contains(func_id));
            if needs_body {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }
        }

        // The MIR entry only dispatches. External wrappers initialize the free-memory pointer on
        // demand with a floor sized for their own reachable static frames.
        self.in_internal_function = false;
        self.emitting_entry = true;
        self.generate_function_body(entry_id, &module.functions[entry_id]);
        self.emitting_entry = false;
        self.record_function_spill_size(entry_id);
        self.runtime_entry_funcs.push(entry_id);

        // External entries, reached only through `tail_call` jumps.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id || !Self::is_external_entry(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.mark_debug_function_invoke(func);
            self.in_internal_function = false;
            self.emit_entry_free_memory_start(module, call_graph, func_id);
            self.generate_function_body(func_id, func);
            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Internal-call targets.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id
                || Self::is_external_entry(func)
                || !Self::is_runtime_function(func)
            {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.mark_debug_function_invoke(func);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func_id, func);
            self.in_internal_function = false;
            self.current_internal_function = None;
            self.record_function_spill_size(func_id);
        }

        self.resolve_pending_frame_size_consts(module);
        self.resolve_static_frames(module);
    }
}
