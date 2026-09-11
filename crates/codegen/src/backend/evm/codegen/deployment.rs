//! Deployment bytecode, constructor arguments, and immutable patching.

use super::{
    ArtifactKind, CallGraphInfo, DeferredConst, DenseBitSet, EvmArtifact, EvmCodegen,
    EvmMemoryLayout, GeneratedCode, ImmutableEncoding, ImmutableId, ImmutableRef, MAX_STACK_DEPTH,
    MirPhase, Module, OptimizationMode, StackOp, U256, WORD_BYTES, immutable_push_type_size,
    immutable_staging_addr, immutable_staging_base, immutable_staging_end, op,
};
use crate::backend::assembler::PreparedAssembly;

struct PreparedDeploymentPrefix {
    assembly: PreparedAssembly,
    constructor_arg_offset: Option<DeferredConst>,
    runtime_offset: DeferredConst,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen unless disabled.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        let artifact = self.generate_deployment_artifact(module);
        (artifact.deployment, artifact.runtime)
    }

    #[tracing::instrument(
        name = "evm_codegen",
        level = "debug",
        skip_all,
        fields(module = %module.name),
    )]

    pub(super) fn generate_deployment_artifact(&mut self, module: &mut Module) -> EvmArtifact {
        // Interfaces have no code. An internal-only library keeps its rejecting
        // dispatch stub, like `solc`.
        if module.is_interface {
            return EvmArtifact::default();
        }
        if let Some(func) = module.functions.iter().find(|func| func.blocks.is_empty()) {
            panic!("cannot codegen MIR function `{}` without an entry block", func.name);
        }
        self.reset_for_module(module);
        self.run_optimization_passes(module);
        if self.emit_unsupported(module) {
            return EvmArtifact::default();
        }
        if module.phase != MirPhase::EvmShaped {
            self.gcx
                .dcx()
                .err(format!(
                    "EVM codegen requires MIR in the `evm-shaped` phase, stopped at `{}`",
                    module.phase.name()
                ))
                .span(module.name.span)
                .emit();
            return EvmArtifact::default();
        }
        self.immutable_staging_base = immutable_staging_base(module);
        self.immutable_encodings.clear();
        for (id, immutable) in module.iter_immutables() {
            let encoding =
                immutable.ty.immutable_encoding().expect("validated immutable declaration");
            let allocated = self.immutable_encodings.push(encoding);
            debug_assert_eq!(allocated, id);
        }
        // Phi elimination places a predecessor's parallel copies before its
        // terminator, which executes them on every outgoing edge. Late CFG
        // passes can leave critical edges whose copies would clobber values
        // still live on a sibling edge, so give each such edge its own block.
        let call_graph = CallGraphInfo::new(module);
        self.stack_only_memory_functions =
            Self::collect_stack_only_memory_functions(module, &call_graph).into();
        for (id, func) in module.functions.iter_mut_enumerated() {
            let stack_phis = self.stack_only_memory_functions.contains(id)
                || (func.attributes.is_yul && func.returns.len() > 1);
            Self::split_phi_critical_edges(func, stack_phis);
        }
        if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            for func in &mut module.functions {
                func.canonicalize_argument_uses();
                if matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size) {
                    func.canonicalize_immediate_uses();
                }
            }
        }
        // Runtime and constructor emission inspect the same final MIR. Compute module-wide facts
        // once instead of rebuilding them for each artifact and caller-stack retry.
        self.msize_observed_functions =
            Self::collect_msize_observed_functions(module, &call_graph).into();
        self.heap_pointer_return_functions = Self::collect_heap_pointer_return_functions(module);
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            for (id, func) in module.functions.iter_mut_enumerated() {
                if self.stack_only_memory_functions.contains(id)
                    || (func.attributes.is_yul && func.returns.len() > 1)
                {
                    // arg N (each use) -> one canonical arg N
                    func.canonicalize_argument_uses();
                }
            }
        }
        self.spill_clobber_functions = self.collect_spill_clobber_functions(module).into();
        self.cold_functions = if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            DenseBitSet::new_empty(module.functions.len())
        } else {
            Self::collect_cold_functions(module)
        };

        // First generate the runtime code
        let runtime_code = self.generate_runtime_code(module, &call_graph);
        let runtime_len = runtime_code.bytecode.len();
        let immutable_refs = std::mem::take(&mut self.runtime_immutable_refs);

        // The constructor copies the runtime code to memory and patches the
        // immutable placeholders with the staged words before
        // returning. Copy to offset 0 unless that would overwrite the immutable
        // staging area before the patch loop reads it.
        let copy_base = Self::runtime_copy_base(module, runtime_len, &immutable_refs);

        // Generate constructor initialization and the deployment postlude as
        // one control-flow graph and optimize it once. Constructor arguments
        // are appended after the generated deployment prefix, so their offset
        // and the runtime-code offset depend on its final push widths. Only
        // repeat final assembly while both offsets stabilize.
        let prepared_deploy_code = self.prepare_deployment_prefix(
            module,
            &call_graph,
            runtime_len,
            copy_base,
            &immutable_refs,
        );
        let mut deploy_code_len = 0usize;
        let mut constructor_arg_offset = runtime_len;
        let mut deploy_code = self.assemble_deployment_prefix(
            &prepared_deploy_code,
            constructor_arg_offset,
            deploy_code_len,
        );
        for _ in 0..8 {
            let next_deploy_code_len = deploy_code.bytecode.len();
            let next_arg_offset = next_deploy_code_len + runtime_len;
            if next_deploy_code_len == deploy_code_len && next_arg_offset == constructor_arg_offset
            {
                break;
            }
            deploy_code_len = next_deploy_code_len;
            constructor_arg_offset = next_arg_offset;
            deploy_code = self.assemble_deployment_prefix(
                &prepared_deploy_code,
                constructor_arg_offset,
                deploy_code_len,
            );
        }

        // Deploy code structure:
        // [constructor_code]    ; run constructor (SSTOREs + immutable staging)
        // PUSH<n> runtime_len   ; size to copy from creation code
        // DUP1                  ; duplicate for the final RETURN size
        // PUSH<n> offset        ; where runtime starts
        // PUSH<n> copy_base     ; memory destination
        // CODECOPY              ; copy runtime to memory
        // [immutable patches]   ; patch staged words into the PUSH<N> placeholders
        // PUSH<n> copy_base     ; memory offset
        // RETURN                ; return the runtime code
        let mut deploy_bytecode = deploy_code.bytecode;
        deploy_bytecode.extend_from_slice(&runtime_code.bytecode);

        // The returned runtime artifact keeps the zero placeholders, like
        // solc's `deployedBytecode` for contracts with immutables.
        EvmArtifact {
            deployment: deploy_bytecode,
            runtime: runtime_code.bytecode,
            immutable_references: immutable_refs,
            deployment_evm_ir: deploy_code.evm_ir,
            runtime_evm_ir: runtime_code.evm_ir,
            deployment_debug_info: deploy_code.debug_info,
            runtime_debug_info: runtime_code.debug_info,
        }
    }

    pub(super) fn runtime_copy_base(
        module: &Module,
        runtime_len: usize,
        immutable_refs: &[ImmutableRef],
    ) -> u64 {
        let patched_end = immutable_refs.iter().fold(runtime_len, |end, immutable_ref| {
            let patch_size = if immutable_ref.type_size.bytes() == 1 { 1 } else { WORD_BYTES };
            end.max(
                immutable_ref
                    .code_offset
                    .checked_add(1 + patch_size)
                    .expect("immutable patch offset overflow"),
            )
        });
        let staging_base = immutable_staging_base(module);
        if !immutable_refs.is_empty() && patched_end as u64 > staging_base {
            immutable_staging_end(staging_base, module.immutable_count())
        } else {
            0
        }
    }

    fn emit_deployment_postlude(
        &mut self,
        module: &Module,
        runtime_offset: DeferredConst,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) {
        // Copy runtime code from creation code to memory at `copy_base`.
        self.asm.emit_push(U256::from(runtime_len as u64));
        self.asm.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push_deferred(runtime_offset);
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::CODECOPY);

        // Patch each `PUSH<N>` placeholder with its staged immutable value.
        for r in immutable_refs {
            let encoding = module
                .immutable_type(r.id)
                .immutable_encoding()
                .expect("validated immutable declaration");
            debug_assert_eq!(
                immutable_push_type_size(
                    encoding,
                    self.gcx.sess.opts.optimization,
                    self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
                ),
                r.type_size
            );
            self.emit_immutable_patch(copy_base, *r, encoding);
        }

        // Return the patched runtime code; the DUP'd length is still on the stack.
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::RETURN);
    }

    fn emit_immutable_patch(
        &mut self,
        copy_base: u64,
        immutable_ref: ImmutableRef,
        encoding: ImmutableEncoding,
    ) {
        let byte_width = immutable_ref.type_size.bytes();
        let destination = copy_base + immutable_ref.code_offset as u64 + 1;

        self.asm.emit_push(U256::from(immutable_staging_addr(
            self.immutable_staging_base,
            immutable_ref.id,
        )));
        self.asm.emit_op(op::MLOAD);

        if byte_width == 1 {
            if matches!(encoding, ImmutableEncoding::LeftAligned(_)) {
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(op::BYTE);
            }
            self.asm.emit_push(U256::from(destination));
            self.asm.emit_op(op::MSTORE8);
            return;
        }

        if byte_width < WORD_BYTES as u8 {
            let trailing_bits = usize::from(WORD_BYTES as u8 - byte_width) * 8;
            match encoding {
                ImmutableEncoding::LeftAligned(_) => {
                    self.asm.emit_push(U256::MAX << trailing_bits);
                    self.asm.emit_op(op::AND);
                }
                ImmutableEncoding::Unsigned(_) | ImmutableEncoding::Signed(_) => {
                    self.asm.emit_push(U256::from(trailing_bits));
                    self.asm.emit_op(op::SHL);
                }
            }

            // Preserve the runtime bytes following the short placeholder. An
            // unaligned MLOAD/MSTORE pair works even across word boundaries.
            self.asm.emit_push(U256::from(destination));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::MAX >> (usize::from(byte_width) * 8));
            self.asm.emit_op(op::AND);
            self.asm.emit_op(op::OR);
        }

        self.asm.emit_push(U256::from(destination));
        self.asm.emit_op(op::MSTORE);
    }

    pub(super) fn emit_load_immutable(&mut self, id: ImmutableId) {
        if self.in_constructor {
            // The running constructor's own placeholders are never patched.
            self.asm.emit_push(U256::from(immutable_staging_addr(self.immutable_staging_base, id)));
            self.asm.emit_op(op::MLOAD);
            return;
        }

        let encoding = self.immutable_encodings[id];
        let type_size = immutable_push_type_size(
            encoding,
            self.gcx.sess.opts.optimization,
            self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
        );
        let byte_width = type_size.bytes();
        self.asm.emit_push_immutable(id, type_size);
        if byte_width == WORD_BYTES as u8 {
            return;
        }
        match encoding {
            ImmutableEncoding::Unsigned(_) => {}
            ImmutableEncoding::Signed(_) => {
                self.asm.emit_push(U256::from(byte_width - 1));
                self.asm.emit_op(op::SIGNEXTEND);
            }
            ImmutableEncoding::LeftAligned(_) => {
                self.asm.emit_push(U256::from((WORD_BYTES as u8 - byte_width) * 8));
                self.asm.emit_op(op::SHL);
            }
        }
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn prepare_deployment_prefix(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) -> PreparedDeploymentPrefix {
        // Constructor and runtime ABI choices belong to separate emission attempts. Preserve
        // constructor fallback decisions across retries, but do not inherit runtime decisions.
        self.disabled_stack_only_functions.clear_to(module.functions.len());
        let switch_gas_code_growth_remaining = self.switch_gas_code_growth_remaining;
        loop {
            self.switch_gas_code_growth_remaining = switch_gas_code_growth_remaining;
            let disabled_stack_only_functions = self.disabled_stack_only_functions.count();
            self.disabled_stack_only_at_attempt_start = self.disabled_stack_only_functions.clone();
            let (constructor_arg_offset, runtime_offset) = self.emit_deployment_prefix(
                module,
                call_graph,
                runtime_len,
                copy_base,
                immutable_refs,
            );
            if self.disabled_stack_only_functions.count() > disabled_stack_only_functions {
                continue;
            }
            if let Some((ctor_id, _)) =
                module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
                && !self.stack_prefixes_fit_from(module, ctor_id, MAX_STACK_DEPTH)
            {
                self.report_stack_limit_error();
            }
            return PreparedDeploymentPrefix {
                assembly: self.asm.prepare(self.capture_evm_ir, self.capture_debug_info),
                constructor_arg_offset,
                runtime_offset,
            };
        }
    }

    fn emit_deployment_prefix(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) -> (Option<DeferredConst>, DeferredConst) {
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Constructor);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        let runtime_offset = self.asm.new_deferred_const();

        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        let implicit_constructor_revert = constructor.is_none().then(|| self.asm.new_label());
        if let Some(revert) = implicit_constructor_revert {
            self.asm.emit_op(op::CALLVALUE);
            self.asm.emit_push_label(revert);
            self.asm.emit_op(op::JUMPI);
        }

        let constructor_arg_offset = if let Some((ctor_id, ctor)) = constructor {
            let internal_targets = call_graph.reachable_callees_from([ctor_id]);
            // Deployment and runtime execute in separate EVM memory instances.
            self.asm.require_source_memory(
                std::iter::once(ctor_id)
                    .chain(internal_targets.iter())
                    .any(|id| module.functions[id].attributes.unrestricted_memory),
            );
            if module.immutable_count() != 0 && ctor.returns.is_empty() {
                self.asm.require_private_memory();
            }
            // Generate constructor bytecode
            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();
            self.function_labels.clear();
            self.function_spill_sizes.clear();
            self.pending_frame_size_consts.clear();
            self.restorable_internal_frames.clear_to(module.functions.len());
            self.static_frame_functions.clear_to(module.functions.len());
            self.static_call_abis.clear();
            self.runtime_stack_args = false;
            // Constructor prefixes have their own stack-depth check below.
            self.preserve_caller_stack =
                std::iter::once(ctor_id).chain(internal_targets.iter()).any(|id| {
                    self.stack_only_memory_functions.contains(id)
                        || (module.functions[id].attributes.is_yul
                            && module.functions[id].returns.len() > 1)
                        || !self.compute_spill_hazard_insts(&module.functions[id]).is_empty()
                });
            self.static_frame_addr_consts.clear();
            self.external_spill_addr_consts.clear();
            self.pending_static_allocs.clear();
            self.runtime_free_memory_consts.clear();
            self.runtime_entry_reachability.clear();
            self.runtime_entry_funcs.clear();
            self.current_internal_function = None;
            self.stack_phi_sources.clear();
            self.function_stack_peaks.clear();
            self.icall_stack_edges.clear();

            for (func_id, func) in module.functions.iter_enumerated() {
                if !func.attributes.may_return_memory
                    && !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference())
                {
                    self.restorable_internal_frames.insert(func_id);
                }
            }

            // Set constructor context for LoadArg handling.
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;
            if self.preserve_caller_stack {
                for id in internal_targets.iter() {
                    if !call_graph.is_recursive(id)
                        && Self::static_frame_offsets_are_local(&module.functions[id])
                    {
                        self.static_frame_functions.insert(id);
                    }
                }
                self.runtime_stack_args = true;
                self.stack_returns_enabled = true;
                let mut callers = internal_targets.clone();
                callers.insert(ctor_id);
                self.compute_stack_arg_masks(module, &callers);
                let values = self.collect_canonical_stack_arg_values(module);
                self.compute_resident_stack_args(module, &values, &callers);
                let uses = self.collect_stack_arg_uses(module);
                self.compute_lazy_stack_args(module, &values, &uses);
                self.compute_direct_stack_args(module, &values, &uses);
                self.compute_stack_return_plans(module);
                self.compute_recursive_stack_abis(module, call_graph);
                self.validate_memory_call_abis(module, &internal_targets);
            }

            for func_id in &internal_targets {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }

            // Constructor locals, immutable staging, and spills occupy fixed
            // compiler-owned regions. The ABI blob starts after their exact
            // post-emission end, and dynamic allocations start after the blob.
            let constructor_fixed_memory_end = self.asm.new_deferred_const();
            let constructor_arg_offset =
                (!ctor.params.is_empty()).then(|| self.asm.new_deferred_const());

            // Constructor args are appended after generated deployment bytecode.
            // Copy the complete blob above every fixed compiler-owned region,
            // then place the free-memory pointer after its word-aligned end.
            if let Some(arg_offset) = constructor_arg_offset {
                self.constructor_args_base_const = Some(constructor_fixed_memory_end);
                self.constructor_args_offset_const = Some(arg_offset);
                self.asm.emit_push_deferred(arg_offset);
                self.asm.emit_op(op::CODESIZE);
                self.asm.emit_op(op::SUB); // size = CODESIZE - arg_offset
                self.asm.emit_stack_op(StackOp::Dup(1));
                self.asm.emit_push_deferred(arg_offset); // code offset
                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                // This blob is ABI decoding input and backing for source memory objects.
                // Scalar arguments reload immutable initcode when source assembly can
                // overwrite the blob; it must not become a private scalar reload home.
                self.asm.emit_source_op(op::CODECOPY);

                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_op(op::ADD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::WORD_SIZE - 1));
                self.asm.emit_op(op::ADD);
                self.asm.emit_push(U256::MAX - U256::from(EvmMemoryLayout::WORD_SIZE - 1));
                self.asm.emit_op(op::AND);
                self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
                self.asm.emit_source_op(op::MSTORE);
            } else {
                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
                self.asm.emit_source_op(op::MSTORE);
            }

            if !internal_targets.is_empty() {
                let constructor_entry = self.asm.new_label();
                self.emit_push_label(constructor_entry);
                self.asm.emit_op(op::JUMP);

                for (func_id, func) in module.functions.iter_enumerated() {
                    if !internal_targets.contains(func_id) {
                        continue;
                    }
                    let label = self.function_labels[&func_id];
                    self.asm.define_label(label);
                    self.mark_debug_function_invoke(func);
                    self.asm
                        .require_source_memory(self.stack_only_memory_functions.contains(func_id));
                    self.emit_stack_arg_prologue(func_id, func);
                    self.in_internal_function = true;
                    self.current_internal_function = Some(func_id);
                    self.generate_function_body(func_id, func);
                    self.in_internal_function = false;
                    self.current_internal_function = None;
                    self.record_function_spill_size(func_id);
                }

                self.asm.define_label(constructor_entry);
            }

            // Generate the constructor body (which includes SSTORE for
            // initializers). Every ordinary completion jumps to one label so
            // branch layout cannot strand the deployment postlude behind a
            // non-final STOP.
            let constructor_exit = self.asm.new_label();
            self.constructor_exit = Some(constructor_exit);
            self.mark_debug_function_invoke(ctor);
            self.asm.require_source_memory(self.stack_only_memory_functions.contains(ctor_id));
            self.generate_function_body(ctor_id, ctor);
            let constructor_spill_size = self.record_function_spill_size(ctor_id);
            let mut fixed_end =
                self.constructor_fixed_memory_end(module.immutable_count(), constructor_spill_size);
            for id in self.static_frame_functions.iter() {
                for (&(function, offset), &(address, _)) in &self.static_frame_addr_consts {
                    if function == id {
                        self.asm.set_deferred_const(address, U256::from(fixed_end + offset));
                    }
                }
                fixed_end += self.emitted_frame_size(module, id);
            }
            self.asm.set_deferred_const(constructor_fixed_memory_end, U256::from(fixed_end));

            self.resolve_pending_frame_size_consts(module);

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_args_base_const = None;
            self.constructor_args_offset_const = None;
            self.constructor_exit = None;
            self.constructor_param_count = 0;

            self.asm.define_label(constructor_exit);
            constructor_arg_offset
        } else {
            None
        };

        self.asm.require_source_memory(false);
        self.emit_deployment_postlude(
            module,
            runtime_offset,
            runtime_len,
            copy_base,
            immutable_refs,
        );
        if let Some(revert) = implicit_constructor_revert {
            self.asm.define_label(revert);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_op(op::REVERT);
        }
        (constructor_arg_offset, runtime_offset)
    }

    fn assemble_deployment_prefix(
        &mut self,
        prepared: &PreparedDeploymentPrefix,
        constructor_arg_offset: usize,
        runtime_offset: usize,
    ) -> GeneratedCode {
        let mut deferred_values = Vec::with_capacity(2);
        if let Some(id) = prepared.constructor_arg_offset {
            deferred_values.push((id, U256::from(constructor_arg_offset)));
        }
        deferred_values.push((prepared.runtime_offset, U256::from(runtime_offset)));
        let result = self.asm.assemble_prepared(&prepared.assembly, &deferred_values);
        GeneratedCode {
            bytecode: result.bytecode,
            evm_ir: result.evm_ir,
            debug_info: result.debug_info,
        }
    }
}
