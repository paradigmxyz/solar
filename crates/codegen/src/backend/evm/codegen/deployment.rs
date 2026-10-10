//! Deployment bytecode, constructor arguments, and immutable patching.

use super::{
    ArtifactKind, AssembledCode, CallGraphInfo, DenseBitSet, EmbeddedBytecodes, EvmArtifact,
    EvmCodegen, EvmMemoryLayout, ImmutableEncoding, ImmutableId, ImmutableRef, Module,
    OptimizationMode, PendingRuntime, StackOp, U256, WORD_BYTES, immutable_push_type_size,
    immutable_staging_addr, immutable_staging_base, immutable_staging_end, ir, op,
};
use crate::{
    link::{CodeKind, LibraryTable},
    mir::{MirPhase, analysis::cold_functions},
};
use alloy_primitives::Bytes;
use solar_interface::Symbol;

impl<'gcx> EvmCodegen<'gcx> {
    /// Optimizes `module` and schedules its runtime code.
    ///
    /// Returns whether the module has code to finish. If so, [`Self::finish_module`]
    /// completes the artifact once the embedded bytecode is available.
    /// Otherwise the artifact is empty.
    pub(crate) fn schedule_module(&mut self, module: &mut Module) -> bool {
        // Interfaces have no code. An internal-only library keeps its rejecting
        // dispatch stub, like `solc`.
        if module.is_interface {
            return false;
        }
        if let Some(func) = module.functions.iter().find(|func| func.blocks.is_empty()) {
            panic!("cannot codegen MIR function `{}` without an entry block", func.name);
        }
        self.reset_for_module(module);
        if self.run_pipeline {
            self.run_optimization_passes(module);
        }
        if self.gcx.dcx().has_errors().is_err() {
            return false;
        }
        if self.emit_unsupported(module) {
            return false;
        }
        self.immutable_staging_base = immutable_staging_base(module);
        self.immutable_encodings.clear();
        for (id, immutable) in module.iter_immutables() {
            let encoding =
                immutable.ty.immutable_encoding().expect("validated immutable declaration");
            let allocated = self.immutable_encodings.push(encoding);
            debug_assert_eq!(allocated, id);
        }
        // Late CFG passes can leave critical edges whose phi inputs would
        // replace values still live on a sibling edge, so give each such edge
        // its own block.
        for func in &mut module.functions {
            Self::split_phi_critical_edges(func);
        }
        if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            for func in &mut module.functions {
                func.canonicalize_argument_uses();
                if matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size) {
                    func.canonicalize_immediate_uses();
                }
            }
        }
        let Ok(lowered) = module.as_lowered(self.gcx.dcx()) else {
            return false;
        };
        let module = &*lowered;
        // Runtime and constructor emission inspect the same final MIR. Compute module-wide facts
        // once instead of rebuilding them for each artifact and caller-stack retry.
        let call_graph = CallGraphInfo::new(module);
        (self.heap_pointer_return_functions, self.heap_pointer_args) =
            Self::collect_heap_pointer_facts(module);
        // The switch planner prices tail calls to empty bodies as shared terminals.
        self.empty_stop_functions = Self::empty_stop_functions(module);
        self.cold_functions = if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            DenseBitSet::new_empty(module.functions.len())
        } else {
            cold_functions(module)
        };

        // First schedule the runtime code and run its EVM IR pipeline. Only final
        // assembly waits for the bytecode of contracts it embeds.
        self.schedule_runtime_code(&lowered, &call_graph);
        self.asm.optimize();
        if self.gcx.dcx().has_errors().is_err() {
            return false;
        }
        self.pending_runtime = Some(PendingRuntime { call_graph });
        true
    }

    /// Completes the artifact of a module scheduled by [`Self::schedule_module`], linking in the
    /// bytecode of the contracts it embeds.
    pub(crate) fn finish_module(
        &mut self,
        module: &Module,
        bytecodes: &EmbeddedBytecodes,
    ) -> EvmArtifact {
        let PendingRuntime { call_graph } =
            self.pending_runtime.take().expect("module must be scheduled first");
        debug_assert_eq!(module.phase(), MirPhase::Lowered);
        let mut libraries = module.libraries.clone();
        let AssembledCode {
            bytecode: runtime_bytecode,
            immutable_refs,
            library_relocations: runtime_library_relocations,
            evm_ir: runtime_evm_ir,
            debug_info: runtime_debug_info,
        } = self.asm.assemble_linked(
            bytecodes,
            &mut libraries,
            self.capture_evm_ir,
            self.capture_debug_info,
        );
        let runtime_bytecode = Bytes::from(runtime_bytecode);
        let runtime_len = runtime_bytecode.len();
        let runtime_name =
            Symbol::intern(&format!("{}_{}", module.name.name, CodeKind::Runtime.keyword()));
        let runtime_data = ir::Data {
            library_relocations: runtime_library_relocations.clone(),
            ..ir::Data::new(runtime_bytecode.clone(), Some(runtime_name))
        };

        // The constructor copies the runtime code to memory and patches the
        // immutable placeholders with the staged words before
        // returning. Copy to offset 0 unless that would overwrite the immutable
        // staging area before the patch loop reads it.
        let copy_base = Self::runtime_copy_base(module, runtime_len, &immutable_refs);

        // Generate constructor initialization and the deployment postlude as
        // one control-flow graph and optimize it once.
        //
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
        // [program data]        ; constructor data, then the runtime code last
        // [constructor args]    ; appended by the deployer
        self.emit_deployment_prefix(
            module,
            &call_graph,
            runtime_data,
            &libraries,
            copy_base,
            &immutable_refs,
        );
        self.asm.optimize();
        let deploy_code = self.asm.assemble_linked(
            bytecodes,
            &mut libraries,
            self.capture_evm_ir,
            self.capture_debug_info,
        );
        let has_constructor_args = module
            .functions
            .iter()
            .any(|func| func.attributes.is_constructor && !func.params.is_empty());
        assert!(
            !has_constructor_args
                || deploy_code.bytecode.is_empty()
                || deploy_code.bytecode.ends_with(&runtime_bytecode),
            "constructor arguments must start where the runtime code ends"
        );

        // The returned runtime artifact keeps the zero placeholders, like
        // solc's `deployedBytecode` for contracts with immutables.
        EvmArtifact {
            libraries,
            deployment: deploy_code.bytecode,
            runtime: runtime_bytecode.into(),
            deployment_library_relocations: deploy_code.library_relocations,
            runtime_library_relocations,
            immutable_references: immutable_refs,
            deployment_evm_ir: deploy_code.evm_ir,
            runtime_evm_ir,
            deployment_debug_info: deploy_code.debug_info,
            runtime_debug_info,
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
        runtime: ir::DataId,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) {
        // Copy runtime code from creation code to memory at `copy_base`.
        self.asm.emit_push(U256::from(runtime_len as u64));
        self.asm.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push_data(ir::DataRef::new(runtime, 0));
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
    /// The args are ABI-encoded and appended after the runtime code, which is the
    /// last data entry of the deployment bytecode.
    #[tracing::instrument(
        name = "stack_scheduling",
        level = "debug",
        skip_all,
        fields(artifact = "deployment")
    )]
    fn emit_deployment_prefix(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        runtime_data: ir::Data,
        libraries: &LibraryTable,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) {
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Constructor);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        let runtime_len = runtime_data.bytes.linked().len();
        let runtime = self.asm.append_runtime_code(runtime_data, libraries);

        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        let implicit_constructor_revert = constructor.is_none().then(|| self.asm.new_label());
        if let Some(revert) = implicit_constructor_revert {
            self.asm.emit_op(op::CALLVALUE);
            self.asm.emit_push_label(revert);
            self.asm.emit_op(op::JUMPI);
        }

        if let Some((ctor_id, ctor)) = constructor {
            self.reset_artifact(module);

            let internal_targets = call_graph.reachable_callees_from(std::iter::once(ctor_id));
            let heap_prefix = Self::heap_prefix_offsets(module);
            let heap_guard = internal_targets
                .iter()
                .chain([ctor_id])
                .map(|func_id| heap_prefix.guard(func_id, &module.functions[func_id]))
                .max()
                .unwrap_or(0);

            // Constructor locals, immutable staging, and spills occupy fixed
            // compiler-owned regions. The ABI blob starts after their exact
            // post-emission end. The heap prefix follows the complete ABI blob.
            let constructor_fixed_memory_end = self.asm.new_deferred_const();
            // The arguments start where the runtime code, the last data entry, ends.
            // Their size reads `CODESIZE`, which makes the data layout observable, so
            // data packing keeps every entry in place.
            let constructor_arg_offset = (!ctor.params.is_empty()).then(|| {
                let runtime_end = u32::try_from(runtime_len).expect("runtime code exceeds `u32`");
                ir::DataRef::new(runtime, runtime_end)
            });
            self.constructor_heap_start = Some((constructor_fixed_memory_end, heap_guard));

            // Constructor arguments load from the copied ABI blob.
            self.in_constructor = true;

            // Constructor args are appended after generated deployment bytecode.
            // Copy the complete blob above every fixed compiler-owned region,
            // then place the free-memory pointer after its word-aligned end.
            if let Some(arg_offset) = constructor_arg_offset {
                self.constructor_args_base_const = Some(constructor_fixed_memory_end);
                self.constructor_args_offset = Some(arg_offset);
                self.emit_constructor_args_size(arg_offset);
                self.asm.emit_stack_op(StackOp::Dup(1));
                self.asm.emit_push_data(arg_offset); // code offset
                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_op(op::CODECOPY);
            }
            // mstore(FMP_SLOT, heap_start)
            self.emit_constructor_heap_start();
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MSTORE);

            // Every ordinary completion of the constructor body (which includes SSTORE for
            // initializers) jumps to one label so branch layout cannot strand the deployment
            // postlude behind a non-final STOP.
            let constructor_exit = self.asm.new_label();
            self.constructor_exit = Some(constructor_exit);
            self.emit_constructor(module, call_graph, ctor_id, &internal_targets);
            let constructor_spill_size = self.function_spill_size(ctor_id);
            let spill_end =
                self.constructor_fixed_memory_end(module.immutable_count(), constructor_spill_size);
            // The helpers' fixed frames follow the constructor's spill area.
            let fixed_memory_end = self.place_constructor_static_frames(module, spill_end);
            if fixed_memory_end.checked_add(heap_guard).is_none() {
                self.gcx
                    .dcx()
                    .err("constructor heap prefix exceeds the addressable memory range")
                    .span(ctor.name_span)
                    .emit();
            }
            self.asm.set_deferred_const(constructor_fixed_memory_end, U256::from(fixed_memory_end));

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_args_base_const = None;
            self.constructor_args_offset = None;
            self.constructor_heap_start = None;
            self.constructor_exit = None;

            self.asm.define_label(constructor_exit);
        }

        self.emit_deployment_postlude(module, runtime, runtime_len, copy_base, immutable_refs);
        if let Some(revert) = implicit_constructor_revert {
            self.asm.define_label(revert);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_op(op::REVERT);
        }
    }

    /// Emits the constructor's initial free memory pointer, consuming the size of the argument
    /// blob from the stack when the constructor takes arguments.
    pub(super) fn emit_constructor_heap_start(&mut self) {
        let (fixed_memory_end, heap_guard) = self
            .constructor_heap_start
            .expect("constructor heap start is recorded before its code");
        if self.constructor_args_offset.is_some() {
            // heap_start = align32(fixed_memory_end + args_size)
            self.asm.emit_push_deferred(fixed_memory_end);
            self.asm.emit_op(op::ADD);
            self.asm.emit_push(U256::from(EvmMemoryLayout::WORD_SIZE - 1));
            self.asm.emit_op(op::ADD);
            self.asm.emit_push(U256::MAX - U256::from(EvmMemoryLayout::WORD_SIZE - 1));
            self.asm.emit_op(op::AND);
        } else {
            // heap_start = fixed_memory_end
            self.asm.emit_push_deferred(fixed_memory_end);
        }
        // heap_start += heap_guard
        if heap_guard != 0 {
            self.asm.emit_push(U256::from(heap_guard));
            self.asm.emit_op(op::ADD);
        }
    }
}
