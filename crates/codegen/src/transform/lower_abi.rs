//! ABI phase lowering: materialize calldata decode / returndata encode as MIR.
//!
//! In `built`/`optimized` MIR an external function takes typed MIR arguments and
//! returns typed values; the calldata decode and returndata encode happen
//! implicitly in the backend. This pass makes that explicit, moving the ABI
//! boundary into MIR itself (the ABI phase of the sketch in [`MirPhase`]).
//!
//! For each external entry `f(x0: T0, .., xn: Tn)`, it:
//!
//! 1. copies the original into a fresh internal function `f.body` with its parameter list preserved
//!    when there are internal callers, and
//! 2. strips `f`'s MIR parameter list, keeping its selector and its `Value::Arg` entries. Scalar
//!    arguments remain lazy ABI head words; dynamic calldata arguments remain logical slices until
//!    `lower-slices` projects their pointer and length. Value-carrying returns are ABI-encoded
//!    according to the function's return layout and terminate with `returndata`.
//!
//! The wrapper keeps argument materialization lazy so values used after a
//! branch can still be rematerialized instead of spilled. Dynamic return
//! encoding becomes a semantic `abi_encode` operation here and lowers later;
//! static returns use the fixed low-memory return buffer directly. Internal
//! call sites that targeted a wrapped function are retargeted to its extracted
//! raw-return body, so internal calls to public functions keep their convention.
//!
//! The phase transition is all-or-nothing: if any value-returning external
//! function lacks a matching ABI return layout, the module is left untouched
//! and does not advance, so an `abi`-phase module always means every external
//! function is a complete wrapper.
//!
//! The `fallback(bytes calldata) returns (bytes memory)` form is a separate
//! raw-data boundary: it gets an argument-free dispatch wrapper and an
//! internal body that terminates with unencoded returndata.
//!
//! Together with [`super::lower_dispatch::LowerDispatch`], which routes a selector switch
//! to these argument-free wrappers, this materializes the ABI boundary before
//! EVM codegen. Both passes must complete before the backend runs.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiParamLayout, AbiParamLayoutRef, AbiParamLocation, AbiParamType, AbiType,
        AbiWordValidator, AllocationSemantics, ArgIdx, BlockId, FrameMode, FrameSlotKind, Function,
        FunctionBuilder, FunctionId, InstId, InstKind, MangledSymbol, MemoryObjectKind,
        MemoryObjectLayout, MirPhase, MirType, Module, PanicCode, SliceLocation, Terminator, Value,
        ValueId,
        utils::{remap_block_order, repair_reachability_phis},
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_interface::{Ident, Span, Symbol, kw};
use solar_sema::{Gcx, hir::Visibility};

/// ABI phase lowering pass.
pub(crate) struct LowerAbi;

impl MirPass for LowerAbi {
    fn name(&self) -> &'static str {
        "lower-abi"
    }

    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        module.phase <= MirPhase::Optimized
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        LowerAbiCx::default().run(module, gcx.sess.opts.evm_version)
    }
}

/// Statistics from ABI wrapper lowering.
#[derive(Clone, Debug, Default)]
struct LowerAbiStats {
    /// Number of external functions wrapped.
    wrapped: usize,
    /// Number of value-carrying returns rewritten to ABI returndata encoding.
    encoded_returns: usize,
    /// Number of external functions whose live returns lack a matching ABI
    /// layout. Any non-zero count makes the whole pass bail.
    skipped_returns: usize,
    /// Number of internal call sites retargeted from a wrapped function to its
    /// extracted body.
    retargeted_calls: usize,
    /// Number of wrappers that received a prologue callvalue check because
    /// the dispatch entry cannot hoist one.
    injected_checks: usize,
    /// Number of constructors whose deferred ABI inputs were materialized.
    decoded_constructors: usize,
}

#[derive(Debug, Default)]
struct LowerAbiCx {
    stats: LowerAbiStats,
    aggregate_helpers: FxHashMap<AbiParamLayout, FunctionId>,
    aggregate_type_helpers: FxHashMap<AbiParamType, FunctionId>,
}

impl LowerAbiCx {
    fn run(&mut self, module: &mut Module, evm_version: EvmVersion) -> bool {
        self.stats = LowerAbiStats::default();

        // Idempotent: only `built`/`optimized` modules have an implicit ABI
        // boundary to materialize.
        if module.phase >= MirPhase::Abi {
            return false;
        }

        let mut targets = Vec::new();
        let mut constructors = Vec::new();
        let mut wrapped_constructors = Vec::new();
        let mut bytes_fallback = None;
        let mut has_decodes = false;
        let mut has_revert_returndata = false;
        let mut has_returndata_sizes = false;
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) {
                targets.push(id);
                self.stats.skipped_returns += usize::from(!can_encode_live_returns(func));
            }
            if is_constructor(func) && func.abi_params.is_some() {
                if Self::can_decode_constructor_params(func) {
                    constructors.push(id);
                } else if Self::can_wrap_constructor_params(func) {
                    wrapped_constructors.push(id);
                } else {
                    return false;
                }
            }
            if func.attributes.is_fallback && is_bytes_fallback(func) {
                if !can_lower_bytes_fallback_returns(func) {
                    return false;
                }
                bytes_fallback = Some(id);
            }
            for inst_id in func.instructions() {
                has_decodes |= matches!(func.inst(inst_id).kind, InstKind::AbiDecode { .. });
                has_returndata_sizes |= matches!(func.inst(inst_id).kind, InstKind::ReturndataSize);
                if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
                    internally_called.insert(function);
                }
            }
            has_revert_returndata |= func
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(Terminator::RevertReturndata)));
        }

        // All-or-nothing: `abi` means *every* bodied external function is a
        // wrapper. If any return lacks the semantic layout required to encode
        // it, leave the module untouched instead of advancing to a phase the
        // content does not satisfy.
        if self.stats.skipped_returns != 0 {
            return false;
        }

        if targets.is_empty()
            && constructors.is_empty()
            && wrapped_constructors.is_empty()
            && !has_decodes
            && !has_revert_returndata
            && !has_returndata_sizes
            && bytes_fallback.is_none()
        {
            let has_selectorless_entry = module.functions.iter().any(|func| {
                is_constructor(func) || func.attributes.is_receive || func.attributes.is_fallback
            });
            if !has_selectorless_entry {
                return false;
            }
            module.advance_phase(MirPhase::Abi);
            return true;
        }

        self.synthesize_shared_aggregate_helpers(module, &targets);
        self.synthesize_shared_aggregate_type_helpers(module, &targets);

        if has_decodes && !self.lower_decode_instructions(module) {
            return false;
        }

        if has_returndata_sizes {
            self.lower_returndata_sizes(module, evm_version);
        }

        if has_revert_returndata {
            self.lower_revert_returndata(module, evm_version);
        }

        // A bytes fallback receives the complete calldata blob and returns raw
        // bytes, rather than an ABI tuple. Keep that boundary explicit: the
        // external fallback becomes an argument-free dispatcher target, while
        // its extracted body terminates with the raw returndata operation.
        if let Some(id) = bytes_fallback
            && !self.wrap_bytes_fallback(module, id)
        {
            return false;
        }

        // Most external functions are never called internally. Only those
        // that are need a second, parameterized body; cloning every wrapper
        // needlessly grows the MIR consumed by all subsequent lowering and
        // backend passes.
        // When the dispatch entry cannot hoist a single callvalue check, each
        // rejecting wrapper carries its own. The check belongs to the wrapper's
        // prologue (falling through into the body) rather than to a guard block
        // in the selector switch, which would pay an extra jump per case.
        // `lower-dispatch` shares the predicate and routes selector cases
        // unguarded.
        let hoist_callvalue = callvalue.hoists();

        for id in constructors {
            self.decode_constructor_params(module.function_mut(id));
            self.stats.decoded_constructors += 1;
        }
        for id in wrapped_constructors {
            let layout = module.function(id).abi_params.clone();
            self.inject_abi_prologue(module.function_mut(id), layout.as_ref(), true, true);
            let func = module.function_mut(id);
            func.abi_params = None;
            func.abi_param_locations = None;
            func.abi_args_lazy = false;
            self.stats.decoded_constructors += 1;
        }

        let mut body_of_wrapper = FxHashMap::default();
        for id in targets {
            if let Some(body_id) = self.wrap_function(module, id, internally_called.contains(id)) {
                body_of_wrapper.insert(id, body_id);
            }
            self.stats.wrapped += 1;
            if !hoist_callvalue && super::utils::rejects_callvalue(module.function(id)) {
                Self::inject_callvalue_check(module.function_mut(id));
                self.stats.injected_checks += 1;
            }
        }

        // Internal calls to a wrapped public/external function must keep the
        // original call semantics: retarget them to the extracted body. The
        // wrappers' own calls already target the bodies and are not affected.
        if !body_of_wrapper.is_empty() {
            for func in module.functions.iter_mut() {
                func.for_each_instruction_mut(|_, inst| {
                    if let InstKind::InternalCall { function, .. } = &mut inst.kind
                        && let Some(&body_id) = body_of_wrapper.get(function)
                    {
                        *function = body_id;
                        self.stats.retargeted_calls += 1;
                    }
                });
            }
        }

        module.advance_phase(MirPhase::Abi);
        true
    }

    /// Materializes a failed external-call revert at the ABI boundary.
    fn lower_revert_returndata(&self, module: &mut Module, evm_version: EvmVersion) {
        for func in module.functions.iter_mut() {
            let blocks: Vec<_> = func.blocks.indices().collect();
            for block in blocks {
                if !matches!(func.blocks[block].terminator, Some(Terminator::RevertReturndata)) {
                    continue;
                }

                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                let zero = builder.imm_u256(U256::ZERO);
                if evm_version.supports_returndata() {
                    let size = builder.returndatasize();
                    builder.returndatacopy(zero, zero, size);
                    builder.revert(zero, size);
                } else {
                    builder.revert(zero, zero);
                }
            }
        }
    }

    /// Materializes the volatile returndata size query at the ABI boundary.
    fn lower_returndata_sizes(&self, module: &mut Module, evm_version: EvmVersion) {
        if evm_version.supports_returndata() {
            for func in module.functions.iter_mut() {
                let instructions: Vec<_> = func.instructions().collect();
                for inst in instructions {
                    if matches!(func.inst(inst).kind, InstKind::ReturndataSize) {
                        func.inst_mut(inst).kind = InstKind::ReturnDataSize;
                    }
                }
            }
            return;
        }

        for func in module.functions.iter_mut() {
            let blocks: Vec<_> = func.blocks.indices().collect();
            for block in blocks {
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let terminator = func.blocks[block].terminator.take();
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                let mut replacements = FxHashMap::default();
                for inst in instructions {
                    if !matches!(builder.func().inst(inst).kind, InstKind::ReturndataSize) {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    }

                    let result = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("returndata size must produce a value");
                    let size = builder.imm_u256(U256::ZERO);
                    replacements.insert(result, size);
                }
                super::lower_abi_encode::move_terminator(&mut builder, block, terminator);
                func.replace_uses_canonicalized(&replacements);
            }
            let _ = repair_reachability_phis(func);
        }
    }

    fn can_decode_constructor_params(func: &Function) -> bool {
        let Some(layout) = &func.abi_params else { return false };
        func.abi_args_lazy
            && func.params.len() == layout.types.len()
            && layout.types.iter().zip(&func.params).all(|(abi_ty, &param_ty)| {
                Self::is_constructor_array_element(abi_ty) && abi_ty.mir_type() == param_ty
            })
            && layout.types.iter().any(|ty| matches!(ty, AbiParamType::FixedArray { .. }))
            && layout
                .types
                .iter()
                .try_fold(0_u64, |words, ty| words.checked_add(Self::constructor_param_words(ty)?))
                .and_then(|words| {
                    let bytes = words.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
                    (bytes != 0).then_some(words)
                })
                .and_then(|words| usize::try_from(words).ok())
                .is_some()
    }

    fn can_wrap_constructor_params(func: &Function) -> bool {
        let Some(layout) = &func.abi_params else { return false };
        func.abi_args_lazy
            && func.params.len() == layout.types.len()
            && layout.types.iter().zip(&func.params).all(|(abi_ty, &param_ty)| {
                (Self::is_constructor_word(abi_ty) || Self::is_supported_aggregate(abi_ty))
                    && abi_ty.mir_type() == param_ty
            })
    }

    fn is_constructor_array_element(ty: &AbiParamType) -> bool {
        Self::is_constructor_word(ty)
            || matches!(
                ty,
                AbiParamType::FixedArray { element, len }
                    if *len <= u64::from(u16::MAX)
                        && Self::is_constructor_array_element(element)
            )
    }

    fn constructor_param_words(ty: &AbiParamType) -> Option<u64> {
        if Self::is_constructor_word(ty) {
            return Some(1);
        }
        let AbiParamType::FixedArray { element, len } = ty else { return None };
        len.checked_mul(Self::constructor_param_words(element)?)
    }

    fn lower_decode_instructions(&self, module: &mut Module) -> bool {
        for func in module.functions.iter() {
            for inst_id in func.instructions() {
                if let InstKind::AbiDecode { layout, .. } = &func.inst(inst_id).kind
                    && (layout.types.is_empty() || layout.checked_head_size().is_none())
                {
                    return false;
                }
            }
        }

        let mut decode_counts = FxHashMap::default();
        for func in module.functions.iter() {
            for inst_id in func.instructions() {
                let InstKind::AbiDecode { layout, .. } = &func.inst(inst_id).kind else {
                    continue;
                };
                *decode_counts.entry(layout.clone()).or_insert(0) += 1;
            }
        }
        let mut static_decode_helpers = FxHashMap::default();
        let mut aggregate_decode_helpers = FxHashMap::default();
        for (layout, count) in decode_counts {
            if count >= 2 && layout.types.len() == 1 && !layout.types[0].is_dynamic() {
                let helper = self.synthesize_static_decode_helper(module, layout.clone());
                static_decode_helpers.insert(layout.clone(), helper);
            } else if count >= 2 && layout.types.iter().any(AbiParamType::is_dynamic) {
                let helper = self.synthesize_aggregate_decode_helper(module, layout.clone());
                aggregate_decode_helpers.insert(layout.clone(), helper);
            }
        }

        let mut changed = false;
        for func in module.functions.iter_mut() {
            let has_decode = func
                .instructions()
                .any(|inst| matches!(func.inst(inst).kind, InstKind::AbiDecode { .. }));
            if !has_decode {
                continue;
            }

            let mut replacements = FxHashMap::default();
            let blocks: Vec<_> = func.blocks.indices().collect();
            for block in blocks {
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let terminator = func.blocks[block].terminator.take();
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                for inst in instructions {
                    let decoded = match &builder.func().inst(inst).kind {
                        InstKind::AbiDecode { data, layout } => Some((
                            super::lower_abi_encode::resolve(*data, &replacements),
                            layout.clone(),
                        )),
                        _ => None,
                    };
                    let Some((data, layout)) = decoded else {
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(inst);
                        continue;
                    };

                    let result = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("ABI decode must produce a value");
                    if let Some(&helper) = static_decode_helpers.get(layout.as_ref()) {
                        let value = builder.internal_call(
                            helper,
                            vec![data],
                            layout.types[0].mir_type(),
                            1,
                        );
                        replacements.insert(result, value);
                        changed = true;
                        continue;
                    }
                    if let Some(&helper) = aggregate_decode_helpers.get(layout.as_ref()) {
                        let value = builder.internal_call(
                            helper,
                            vec![data],
                            layout.types[0].mir_type(),
                            layout.types.len(),
                        );
                        replacements.insert(result, value);
                        changed = true;
                        continue;
                    }

                    let base = builder.memory_object_data(data, MemoryObjectKind::Bytes);
                    let length = builder.memory_object_len(data, MemoryObjectKind::Bytes);
                    let Some(values) = Self::decode_memory_tuple(
                        &mut builder,
                        base,
                        length,
                        layout.as_ref(),
                        false,
                    ) else {
                        return false;
                    };
                    replacements.insert(result, values[0]);

                    if values.len() > 1 {
                        let words = values.len() as u64;
                        let (object, object_layout) =
                            builder.alloc_word_array(words, AllocationSemantics::INTERNAL);
                        let base = builder.memory_object_data(object, MemoryObjectKind::FixedArray);
                        builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, base);
                        for (index, value) in values.iter().copied().enumerate().skip(1) {
                            let index = builder.imm_u64(index as u64);
                            builder.memory_object_store_element(
                                object,
                                object_layout,
                                index,
                                value,
                            );
                        }
                    }
                    changed = true;
                }
                super::lower_abi_encode::move_terminator(&mut builder, block, terminator);
            }
            func.replace_uses_canonicalized(&replacements);
            let repaired = repair_reachability_phis(func);
            changed |= repaired;
        }
        changed
    }

    fn synthesize_shared_aggregate_helpers(&mut self, module: &mut Module, targets: &[FunctionId]) {
        let mut counts = FxHashMap::<AbiParamLayout, usize>::default();
        for &id in targets {
            let func = module.function(id);
            let Some(layout) = func.abi_params.as_ref() else { continue };
            if !Self::can_share_aggregate_helper(func, layout) {
                continue;
            }
            *counts.entry(layout.clone()).or_default() += 1;
        }

        for (layout, count) in counts {
            if count < 2 {
                continue;
            }
            let helper = self.synthesize_calldata_aggregate_helper(module, layout.clone());
            self.aggregate_helpers.insert(layout, helper);
        }
    }

    fn synthesize_shared_aggregate_type_helpers(
        &mut self,
        module: &mut Module,
        targets: &[FunctionId],
    ) {
        let mut counts = FxHashMap::<AbiParamType, usize>::default();
        for &id in targets {
            let func = module.function(id);
            let Some(layout) = func.abi_params.as_ref() else { continue };
            for (ty, &arg_type) in layout.types.iter().zip(&func.params) {
                if Self::is_supported_aggregate(ty) && matches!(arg_type, MirType::MemoryObject(_))
                {
                    *counts.entry(ty.clone()).or_default() += 1;
                }
            }
        }
        for (ty, count) in counts {
            if count < 2 {
                continue;
            }
            let helper = self.synthesize_calldata_aggregate_type_helper(module, ty.clone());
            self.aggregate_type_helpers.insert(ty, helper);
        }
    }

    fn can_share_aggregate_helper(func: &Function, layout: &AbiParamLayout) -> bool {
        Self::can_share_aggregate_args(layout, &func.params.iter().copied().collect::<Vec<_>>())
    }

    fn can_share_aggregate_args(layout: &AbiParamLayout, arg_types: &[MirType]) -> bool {
        layout.types.iter().enumerate().all(|(index, ty)| {
            !Self::is_supported_aggregate(ty)
                || matches!(arg_types.get(index), Some(MirType::MemoryObject(_)))
        }) && layout.types.iter().any(Self::is_supported_aggregate)
    }

    fn synthesize_calldata_aggregate_helper(
        &self,
        module: &mut Module,
        layout: AbiParamLayout,
    ) -> FunctionId {
        let name = format!("__decode_calldata_{}", module.functions.len());
        let mut function = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let input = builder.add_param(MirType::Slice(SliceLocation::Calldata));
            let base = builder.slice_ptr(input);
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let mut values = Vec::new();
            let mut head_offset = 0_u64;
            for ty in &layout.types {
                if Self::is_supported_aggregate(ty) {
                    let offset = builder.imm_u64(head_offset);
                    let head = builder.add(base, offset);
                    let value = Self::decode_aggregate_argument(
                        &mut builder,
                        ty,
                        ty.mir_type(),
                        head,
                        base,
                        input_end,
                        false,
                        &mut current,
                        true,
                        false,
                    );
                    builder.add_return(ty.mir_type());
                    values.push(value);
                }
                head_offset += ty.checked_head_size().expect("ABI head size exceeds u64 range");
            }
            builder.ret(values);
        }
        module.add_function(function)
    }

    fn synthesize_calldata_aggregate_type_helper(
        &self,
        module: &mut Module,
        ty: AbiParamType,
    ) -> FunctionId {
        let name = format!("__decode_calldata_type_{}", module.functions.len());
        let mut function = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let head = builder.add_param(MirType::uint256());
            let tuple_base = builder.add_param(MirType::uint256());
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let value = Self::decode_aggregate_argument(
                &mut builder,
                &ty,
                ty.mir_type(),
                head,
                tuple_base,
                input_end,
                false,
                &mut current,
                true,
                false,
            );
            builder.add_return(ty.mir_type());
            builder.ret([value]);
        }
        module.add_function(function)
    }

    fn synthesize_static_decode_helper(
        &self,
        module: &mut Module,
        layout: AbiParamLayoutRef,
    ) -> FunctionId {
        let name = format!("__decode_static_{}", module.functions.len());
        let mut function = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        let result_ty = layout.types[0].mir_type();
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let data = builder.add_param(MirType::MemoryObject(MemoryObjectKind::Bytes));
            builder.add_return(result_ty);
            let base = builder.memory_object_data(data, MemoryObjectKind::Bytes);
            let length = builder.memory_object_len(data, MemoryObjectKind::Bytes);
            let values =
                Self::decode_memory_tuple(&mut builder, base, length, layout.as_ref(), false)
                    .expect("checked static ABI layout");
            builder.ret(values);
        }
        module.add_function(function)
    }

    fn synthesize_aggregate_decode_helper(
        &self,
        module: &mut Module,
        layout: AbiParamLayoutRef,
    ) -> FunctionId {
        let name = format!("__decode_aggregate_{}", module.functions.len());
        let mut function = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let data = builder.add_param(MirType::MemoryObject(MemoryObjectKind::Bytes));
            for ty in &layout.types {
                builder.add_return(ty.mir_type());
            }
            let base = builder.memory_object_data(data, MemoryObjectKind::Bytes);
            let length = builder.memory_object_len(data, MemoryObjectKind::Bytes);
            let values =
                Self::decode_memory_tuple(&mut builder, base, length, layout.as_ref(), false)
                    .expect("checked aggregate ABI layout");
            builder.ret(values);
        }
        module.add_function(function)
    }

    /// Materializes fixed constructor inputs while preserving the physical
    /// word parameters consumed by deployment codegen.
    fn decode_constructor_params(&self, func: &mut Function) {
        let layout = func.abi_params.clone().expect("checked constructor ABI layout");
        let old_entry = BlockId::ENTRY;
        let arg_uses = func.arg_uses();
        let physical_words = layout
            .types
            .iter()
            .map(Self::constructor_param_words)
            .try_fold(0_u64, |words, next| words.checked_add(next?))
            .expect("checked constructor ABI word count");
        let head_size = physical_words
            .checked_mul(EvmMemoryLayout::WORD_SIZE)
            .expect("checked constructor ABI head size");
        let mut params = IndexVec::with_capacity(physical_words as usize);
        for ty in &layout.types {
            Self::push_constructor_param_types(&mut params, ty);
        }
        func.set_params(params);
        let physical_indices = func.params.indices().collect::<Vec<_>>();
        let physical_args =
            physical_indices.into_iter().map(|index| func.alloc_arg(index)).collect::<Vec<_>>();

        let mut replacements = FxHashMap::default();
        let guard = {
            let mut builder = FunctionBuilder::new(func);
            let guard = builder.create_block();
            let decode = builder.create_block();
            let revert = builder.create_block();
            builder.switch_to_block(guard);
            let base = builder.constructor_args_base();
            let end = builder.constructor_args_end();
            let head_size = builder.imm_u64(head_size);
            let required = builder.add(base, head_size);
            let overflow = builder.lt(required, base);
            let short = builder.gt(required, end);
            let invalid = builder.or(overflow, short);
            builder.branch(invalid, revert, decode);

            builder.switch_to_block(revert);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);

            builder.switch_to_block(decode);
            let mut physical_index = 0;
            for (logical_index, ty) in layout.types.iter().enumerate() {
                let value = Self::decode_constructor_param(
                    &mut builder,
                    ty,
                    &physical_args,
                    &mut physical_index,
                );
                for &use_value in arg_uses.get(ArgIdx::new(logical_index)).into_iter().flatten() {
                    replacements.insert(use_value, value);
                }
            }
            debug_assert_eq!(physical_index, physical_args.len());
            builder.jump(old_entry);
            guard
        };
        func.replace_uses_canonicalized(&replacements);
        func.abi_params = None;
        func.abi_param_locations = None;
        func.abi_args_lazy = false;
        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        remap_block_order(func, &order);
    }

    fn decode_constructor_param(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        physical_args: &[ValueId],
        physical_index: &mut usize,
    ) -> ValueId {
        if let AbiParamType::Scalar(scalar) = ty {
            let value = physical_args[*physical_index];
            *physical_index += 1;
            return Self::validate_constructor_word(builder, value, *scalar);
        }
        if let AbiParamType::Enum { variants, .. } = ty {
            let value = physical_args[*physical_index];
            *physical_index += 1;
            return Self::validate_constructor_enum(builder, value, *variants);
        }

        let AbiParamType::FixedArray { element, len } = ty else {
            unreachable!("checked constructor ABI parameter")
        };
        let size = builder.imm_u64(len.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        let layout = MemoryObjectLayout::word_fixed_array(*len);
        let ptr = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        for index in 0..*len {
            let value =
                Self::decode_constructor_param(builder, element, physical_args, physical_index);
            let index = builder.imm_u64(index);
            builder.memory_object_store_element(ptr, layout, index, value);
        }
        ptr
    }

    /// Rewrites one external function into a self-decoding form, keeping a
    /// pristine copy for internal callers.
    ///
    /// The original function keeps its selector and loses its MIR parameter
    /// and return lists, but its `Value::Arg` entries stay in place. Scalar arguments
    /// continue to denote ABI head words, while logical calldata slices are
    /// projected by `lower-slices`; both forms preserve lazy per-use
    /// rematerialization, so wrapper arguments do not spill.
    /// Materializing the loads as eager MIR instructions instead was measured
    /// to cost real bytes: an instruction result is not rematerializable, so
    /// every multi-use or cross-block argument bought spill traffic the
    /// `Arg` form avoids. The explicit-decode representation returns when
    /// slices provide explicit high-level decode semantics without changing
    /// that backend property. Return values are ABI-encoded in place, and no
    /// internal call is introduced on the external path. When the function has
    /// internal callers, a pristine `.body` copy with raw returns and parameters
    /// preserved is appended and those callers are retargeted to it.
    fn wrap_function(
        &mut self,
        module: &mut Module,
        wrapper_id: FunctionId,
        needs_body: bool,
    ) -> Option<FunctionId> {
        let original = module.function(wrapper_id).clone();
        let lazy_args = original.abi_args_lazy;
        let abi_params = original.abi_params.clone();
        let call_body = needs_body
            && Self::can_call_body(&original, abi_params.as_ref())
            && original.blocks[BlockId::ENTRY].instructions.first().copied().is_some();
        let original_entry_inst = original.blocks[BlockId::ENTRY].instructions.first().copied();
        // The copy must precede wrapper mutation and callvalue injection so
        // internal callers keep the original function semantics.
        let body_id = needs_body.then(|| {
            let mut body = original.clone();
            body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
            body.name_span = Span::DUMMY;
            body.selector = None;
            body.abi_returns = None;
            body.abi_return_params = None;
            body.abi_params = None;
            body.abi_param_locations = None;
            body.abi_args_lazy = false;
            body.attributes.visibility = Visibility::Internal;
            body.for_each_instruction_mut(|_, inst| inst.metadata.set_abi_validation(false));
            module.add_function(body)
        });

        if lazy_args || abi_params.is_some() {
            self.inject_abi_prologue(
                module.function_mut(wrapper_id),
                abi_params.as_ref(),
                lazy_args,
                false,
            );
        }
        if call_body {
            Self::replace_body_with_call(
                module.function_mut(wrapper_id),
                body_id.expect("body clone for a calling wrapper"),
                original_entry_inst.expect("calling wrapper has an entry instruction"),
                original.returns[0],
            );
        }
        let return_params = module.function(wrapper_id).abi_return_params.clone();
        self.stats.encoded_returns += encode_live_returns(
            module.function_mut(wrapper_id),
            return_params.as_ref(),
            abi_params.as_ref(),
            lazy_args,
        );

        // External wrappers take no MIR arguments; constructor parameters
        // retain their physical ABI head words for deployment codegen.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.params.clear();
        wrapper.returns.clear();
        wrapper.abi_returns = None;
        wrapper.abi_return_params = None;
        wrapper.abi_params = None;
        wrapper.abi_param_locations = None;
        wrapper.abi_args_lazy = false;
        body_id
    }

    fn can_call_body(func: &Function, abi_params: Option<&AbiParamLayout>) -> bool {
        let Some(layout) = abi_params else { return false };
        layout.types.len() == func.params.len()
            && layout.types.iter().all(Self::is_constructor_word)
            && func.params.iter().all(|ty| {
                !matches!(ty, MirType::Function | MirType::MemoryObject(_) | MirType::Slice(_))
            })
            && func.returns.len() == 1
            && !matches!(func.returns[0], MirType::MemoryObject(_) | MirType::Slice(_))
            && func.blocks.iter().all(|block| {
                !matches!(&block.terminator, Some(Terminator::Return { values }) if values.len() != 1)
            })
    }

    fn replace_body_with_call(
        func: &mut Function,
        body_id: FunctionId,
        original_entry_inst: InstId,
        return_ty: MirType,
    ) {
        let Some(block) = func
            .blocks
            .indices()
            .find(|&block| func.blocks[block].instructions.first() == Some(&original_entry_inst))
        else {
            return;
        };
        func.blocks[block].instructions.clear();
        func.blocks[block].terminator = None;
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        let indices = builder.func().arg_indices().collect::<Vec<_>>();
        let args = indices.into_iter().map(|index| builder.func_mut().alloc_arg(index)).collect();
        let result = builder.internal_call(body_id, args, return_ty, 1);
        builder.ret([result]);
        let _ = repair_reachability_phis(builder.func_mut());
    }

    /// Rewrites `fallback(bytes calldata) returns (bytes memory)` into an
    /// argument-free wrapper and an internal body that returns the raw bytes.
    ///
    /// Fallback input is the complete calldata, including any selector-like
    /// prefix. Its return value is not ABI encoded: the body therefore uses a
    /// `return_data` terminator, and the wrapper's resultless call is reshaped
    /// into a tail call after slice lowering.
    fn wrap_bytes_fallback(&mut self, module: &mut Module, fallback_id: FunctionId) -> bool {
        let original = module.function(fallback_id).clone();
        let mut body = original.clone();
        body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
        body.name_span = Span::DUMMY;
        body.selector = None;
        body.abi_returns = None;
        body.abi_return_params = None;
        body.abi_params = None;
        body.abi_param_locations = None;
        body.abi_args_lazy = false;
        body.attributes.visibility = Visibility::Internal;
        body.for_each_instruction_mut(|_, inst| inst.metadata.set_abi_validation(false));
        body.attributes.is_fallback = false;
        body.attributes.is_receive = false;
        if !Self::lower_bytes_fallback_returns(&mut body) {
            return false;
        }
        let body_id = module.add_function(body);

        let mut wrapper = Function::new(Ident::with_dummy_span(original.name.symbol));
        wrapper.name = original.name;
        wrapper.name_span = original.name_span;
        wrapper.attributes = original.attributes;
        {
            let mut builder = FunctionBuilder::new(&mut wrapper);
            let zero = builder.imm_u64(0);
            let length = builder.calldatasize();
            let input = builder.make_slice(zero, length, SliceLocation::Calldata);
            builder.internal_call_void(body_id, vec![input], 0);
            builder.invalid();
        }
        *module.function_mut(fallback_id) = wrapper;
        true
    }

    /// Rewrites every value-carrying fallback return into raw returndata.
    fn lower_bytes_fallback_returns(func: &mut Function) -> bool {
        let blocks: Vec<_> = func.blocks.indices().collect();
        for block in blocks {
            let Some(Terminator::Return { values }) = func.blocks[block].terminator.clone() else {
                continue;
            };
            let Some(&value) = values.first() else { return false };
            if values.len() != 1
                || func.value_ty(value) != Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
            {
                return false;
            }
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block);
            let offset = builder.memory_object_data(value, MemoryObjectKind::Bytes);
            let size = builder.memory_object_len(value, MemoryObjectKind::Bytes);
            builder.ret_data(offset, size);
        }
        true
    }

    /// Materializes deferred ABI arguments and their validation checks.
    fn inject_abi_prologue(
        &self,
        func: &mut Function,
        abi_params: Option<&AbiParamLayout>,
        lazy_args: bool,
        constructor: bool,
    ) {
        let arg_types: Vec<_> = func.params.iter().copied().collect();
        if !constructor
            && arg_types.is_empty()
            && abi_params.is_none_or(|layout| layout.types.is_empty())
        {
            return;
        }

        let old_entry = BlockId::ENTRY;
        let arg_uses = func.arg_uses();
        let abi_param_locations = func.abi_param_locations.clone();
        let mut logical_values = Vec::new();
        let mut replacements = FxHashMap::default();
        if let Some(layout) = abi_params {
            let mut head_offset = 0_u64;
            let mut logical_physical = Vec::with_capacity(layout.types.len());
            for ty in &layout.types {
                logical_physical.push(
                    (!Self::is_supported_aggregate(ty))
                        .then(|| ArgIdx::new((head_offset / 32) as usize)),
                );
                head_offset += ty.checked_head_size().expect("ABI head size exceeds u64 range");
            }
            let preserve_word_types = abi_params.is_some_and(|layout| {
                layout.types.len() == arg_types.len()
                    && layout.types.iter().zip(&arg_types).all(|(ty, &param)| {
                        Self::is_constructor_word(ty)
                            && ty.mir_type() == param
                            && param != MirType::Function
                    })
            });
            let mut params = IndexVec::with_capacity((head_offset / 32) as usize);
            for (index, _) in (0..head_offset / 32).enumerate() {
                params.push(if preserve_word_types {
                    arg_types[index]
                } else {
                    MirType::uint256()
                });
            }
            func.set_params(params);
            logical_values = logical_physical
                .into_iter()
                .map(|index| index.map(|index| func.alloc_arg(index)))
                .collect::<Vec<_>>();
            for (logical, value) in logical_values.iter().enumerate() {
                if let Some(value) = value {
                    if arg_types.get(logical) == Some(&MirType::Function) {
                        continue;
                    }
                    for &use_value in arg_uses.get(ArgIdx::new(logical)).into_iter().flatten() {
                        replacements.insert(use_value, *value);
                    }
                }
            }
        }
        let guard = {
            let mut builder = FunctionBuilder::new(func);
            let guard = builder.create_block();
            let revert = builder.create_block();
            let mut current = guard;

            builder.switch_to_block(current);
            let input_base =
                if constructor { builder.constructor_args_base() } else { builder.imm_u64(4) };
            let input_end =
                if constructor { builder.constructor_args_end() } else { builder.calldatasize() };
            let head_size = abi_params.map_or((arg_types.len() as u64) * 32, |layout| {
                layout.checked_head_size().expect("ABI head size exceeds u64 range")
            });
            let invalid = if constructor {
                let head_size_value = builder.imm_u64(head_size);
                let required = builder.add(input_base, head_size_value);
                let overflow = builder.lt(required, input_base);
                let short = builder.gt(required, input_end);
                builder.or(overflow, short)
            } else {
                let required = builder.imm_u64(4 + head_size);
                builder.lt(input_end, required)
            };
            let next = builder.create_block();
            builder.branch(invalid, revert, next);
            current = next;

            if lazy_args {
                let mut head_offset = 0;
                for (index, &ty) in arg_types.iter().enumerate() {
                    let validator = abi_params
                        .and_then(|layout| layout.types.get(index))
                        .and_then(|layout_ty| match layout_ty {
                            AbiParamType::Enum { variants, .. } => {
                                Some(AbiWordValidator::EnumRange(*variants))
                            }
                            AbiParamType::Scalar(_) => AbiWordValidator::from_mir_type(ty),
                            _ => None,
                        })
                        .or_else(|| AbiWordValidator::from_mir_type(ty));
                    head_offset +=
                        abi_params.and_then(|layout| layout.types.get(index)).map_or(32, |ty| {
                            ty.checked_head_size().expect("ABI head size exceeds u64 range")
                        });
                    let Some(validator) = validator else { continue };
                    builder.switch_to_block(current);
                    // `Value::Arg` values carry the canonicality invariant that this
                    // guard establishes. Read the raw input word so an optimizer
                    // cannot fold the check away before it runs.
                    let offset = if constructor {
                        let offset_value = builder.imm_u64(head_offset - 32);
                        builder.add(input_base, offset_value)
                    } else {
                        builder.imm_u64(4 + head_offset - 32)
                    };
                    let word = Self::load_input_word(&mut builder, offset, input_end, constructor);
                    let valid = validator.condition(&mut builder, word);
                    let next = builder.create_block();
                    builder.branch(valid, next, revert);
                    current = next;
                }
            }

            if let Some(layout) = abi_params
                && !constructor
                && let Some(&helper) = self.aggregate_helpers.get(layout)
                && Self::can_share_aggregate_args(layout, &arg_types)
            {
                let length = builder.sub(input_end, input_base);
                let input = builder.make_slice(input_base, length, SliceLocation::Calldata);
                let aggregate_types = layout
                    .types
                    .iter()
                    .filter(|ty| Self::is_supported_aggregate(ty))
                    .collect::<Vec<_>>();
                let returns = aggregate_types.len();
                let value = builder.internal_call(
                    helper,
                    vec![input],
                    aggregate_types[0].mir_type(),
                    returns,
                );
                let return_base = (returns > 1)
                    .then(|| builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word));
                let return_layout = MemoryObjectLayout::word_fixed_array(returns as u64);
                let mut aggregate_index = 0;
                for (index, ty) in layout.types.iter().enumerate() {
                    if !Self::is_supported_aggregate(ty) {
                        continue;
                    }
                    let value = if aggregate_index == 0 {
                        value
                    } else {
                        let index_value = builder.imm_u64(aggregate_index as u64);
                        builder.memory_object_load_object(
                            return_base.expect("multi-return buffer for aggregate helper"),
                            return_layout,
                            index_value,
                            match ty.mir_type() {
                                MirType::MemoryObject(kind) => kind,
                                _ => unreachable!("aggregate helper returns memory objects"),
                            },
                        )
                    };
                    for &use_value in arg_uses.get(ArgIdx::new(index)).into_iter().flatten() {
                        replacements.insert(use_value, value);
                    }
                    aggregate_index += 1;
                }
            } else if let Some(layout) = abi_params {
                let mut head_offset = 0;
                for (index, ty) in layout.types.iter().enumerate() {
                    let arg_index = ArgIdx::new(index);
                    let uses = arg_uses.get(arg_index).map_or(&[][..], Vec::as_slice);
                    if !Self::is_supported_aggregate(ty) {
                        head_offset +=
                            ty.checked_head_size().expect("ABI head size exceeds u64 range");
                        continue;
                    }
                    let Some(arg_type) = arg_types.get(index).copied() else {
                        head_offset +=
                            ty.checked_head_size().expect("ABI head size exceeds u64 range");
                        continue;
                    };
                    if !matches!(arg_type, MirType::MemoryObject(_) | MirType::Slice(_)) {
                        head_offset +=
                            ty.checked_head_size().expect("ABI head size exceeds u64 range");
                        continue;
                    }
                    let (head, tuple_base) = if constructor {
                        let head_offset_value = builder.imm_u64(head_offset);
                        (builder.add(input_base, head_offset_value), input_base)
                    } else {
                        (builder.imm_u64(4 + head_offset), builder.imm_u64(4))
                    };
                    if uses.is_empty() {
                        let location = abi_param_locations
                            .as_deref()
                            .and_then(|locations| locations.get(index))
                            .copied()
                            // Text MIR and older callers do not carry HIR data locations.
                            // Preserve the historical lazy behavior for those inputs.
                            .unwrap_or(AbiParamLocation::Calldata);
                        if location == AbiParamLocation::Memory {
                            if !constructor
                                && matches!(arg_type, MirType::MemoryObject(_))
                                && let Some(&helper) = self.aggregate_type_helpers.get(ty)
                            {
                                builder.internal_call(helper, vec![head, tuple_base], arg_type, 1);
                            } else {
                                let _ = Self::decode_aggregate_argument(
                                    &mut builder,
                                    ty,
                                    arg_type,
                                    head,
                                    tuple_base,
                                    input_end,
                                    constructor,
                                    &mut current,
                                    false,
                                    false,
                                );
                            }
                        } else if ty.is_dynamic() {
                            Self::validate_aggregate_argument(
                                &mut builder,
                                ty,
                                head,
                                tuple_base,
                                input_end,
                                constructor,
                                &mut current,
                                true,
                            );
                        }
                    } else {
                        let value = if !constructor
                            && matches!(arg_type, MirType::MemoryObject(_))
                            && let Some(&helper) = self.aggregate_type_helpers.get(ty)
                        {
                            builder.internal_call(helper, vec![head, tuple_base], arg_type, 1)
                        } else {
                            Self::decode_aggregate_argument(
                                &mut builder,
                                ty,
                                arg_type,
                                head,
                                tuple_base,
                                input_end,
                                constructor,
                                &mut current,
                                // The wrapper guard already checked the complete
                                // top-level ABI head, including static aggregate
                                // fields and dynamic offsets.
                                true,
                                false,
                            )
                        };
                        for &use_value in uses {
                            replacements.insert(use_value, value);
                        }
                    }
                    head_offset += ty.checked_head_size().expect("ABI head size exceeds u64 range");
                }
            }

            builder.switch_to_block(current);
            for (logical, value) in logical_values.iter().enumerate() {
                if arg_types.get(logical) != Some(&MirType::Function) {
                    continue;
                }
                let Some(value) = value else { continue };
                let shift = builder.imm_u64(64);
                let value = builder.shr(shift, *value);
                for &use_value in arg_uses.get(ArgIdx::new(logical)).into_iter().flatten() {
                    replacements.insert(use_value, value);
                }
            }
            builder.jump(old_entry);

            builder.switch_to_block(revert);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);

            guard
        };
        func.replace_uses_canonicalized(&replacements);
        func.for_each_instruction_mut(|_, inst| inst.metadata.set_abi_validation(false));
        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        remap_block_order(func, &order);
    }

    /// Validates the immediate ABI shape of an aggregate without materializing
    /// its memory representation.
    #[allow(clippy::too_many_arguments)]
    fn validate_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
        head_checked: bool,
    ) {
        builder.switch_to_block(*current);
        let base = if ty.is_dynamic() {
            if !head_checked {
                Self::guard_source_range(builder, head, 32, input_end, constructor, current);
            }
            let offset = Self::load_input_word(builder, head, input_end, constructor);
            Self::guard_source_offset(builder, tuple_base, offset, input_end, constructor, current)
        } else {
            head
        };

        match ty {
            AbiParamType::DynamicArray(element) => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let element_head_size = builder
                    .imm_u64(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                let head_bytes = Self::checked_mul(builder, len, element_head_size, current);
                let data = builder.add(base, word);
                Self::guard_source_range_value(
                    builder,
                    data,
                    head_bytes,
                    input_end,
                    constructor,
                    current,
                );
            }
            AbiParamType::FixedArray { element, len } => {
                let head_size = len.saturating_mul(
                    element.checked_head_size().expect("ABI head size exceeds u64 range"),
                );
                if !head_checked || ty.is_dynamic() {
                    Self::guard_source_range(
                        builder,
                        base,
                        head_size,
                        input_end,
                        constructor,
                        current,
                    );
                }
            }
            AbiParamType::Tuple(fields) => {
                let head_size = fields.iter().fold(0_u64, |size, field| {
                    size.saturating_add(
                        field.checked_head_size().expect("ABI head size exceeds u64 range"),
                    )
                });
                if !head_checked || ty.is_dynamic() {
                    Self::guard_source_range(
                        builder,
                        base,
                        head_size,
                        input_end,
                        constructor,
                        current,
                    );
                }
            }
            AbiParamType::Bytes => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let data = builder.add(base, word);
                Self::guard_source_range_value(builder, data, len, input_end, constructor, current);
            }
            AbiParamType::Scalar(_) | AbiParamType::Enum { .. } => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        arg_type: MirType,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
        head_checked: bool,
        allow_alias: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let base = if ty.is_dynamic() {
            if !head_checked {
                Self::guard_source_range(builder, head, 32, input_end, constructor, current);
            }
            let offset = Self::load_input_word(builder, head, input_end, constructor);
            Self::guard_source_offset(builder, tuple_base, offset, input_end, constructor, current)
        } else {
            head
        };
        match ty {
            AbiParamType::Scalar(scalar) if constructor => {
                Self::decode_input_scalar(builder, *scalar, base, input_end, current, head_checked)
            }
            AbiParamType::Scalar(scalar) => {
                Self::decode_scalar(builder, *scalar, base, current, head_checked)
            }
            AbiParamType::Enum { variants, .. } if constructor => {
                Self::decode_input_enum(builder, *variants, base, input_end, current, head_checked)
            }
            AbiParamType::Enum { variants, .. } => {
                Self::decode_enum(builder, *variants, base, current, head_checked)
            }
            AbiParamType::FixedArray { element, len }
                if Self::is_supported_tuple_field(element) =>
            {
                let head_size = len.saturating_mul(
                    element.checked_head_size().expect("ABI head size exceeds u64 range"),
                );
                if !head_checked || ty.is_dynamic() {
                    Self::guard_source_range(
                        builder,
                        base,
                        head_size,
                        input_end,
                        constructor,
                        current,
                    );
                }
                if matches!(arg_type, MirType::Slice(SliceLocation::Calldata)) {
                    let length = builder.imm_u64(*len);
                    return builder.make_slice(base, length, SliceLocation::Calldata);
                }
                if constructor && allow_alias && Self::is_scalar_or_enum(element) {
                    let mut offset = 0;
                    for _ in 0..*len {
                        let offset_value = builder.imm_u64(offset);
                        let word_pos = builder.add(base, offset_value);
                        let _ = Self::decode_input_scalar_or_enum(
                            builder, element, word_pos, input_end, current, true,
                        );
                        offset +=
                            element.checked_head_size().expect("ABI head size exceeds u64 range");
                    }
                    return base;
                }
                let size = builder.imm_u64(len.saturating_mul(32));
                let ptr = builder.alloc_object(
                    size,
                    MemoryObjectLayout::word_fixed_array(*len),
                    AllocationSemantics::INTERNAL,
                );
                let mut offset = 0;
                for index in 0..*len {
                    let offset_value = builder.imm_u64(offset);
                    let word_pos = builder.add(base, offset_value);
                    let value = match element.as_ref() {
                        AbiParamType::Scalar(scalar) if constructor => Self::decode_input_scalar(
                            builder, *scalar, word_pos, input_end, current, true,
                        ),
                        AbiParamType::Scalar(scalar) => {
                            Self::decode_scalar(builder, *scalar, word_pos, current, true)
                        }
                        AbiParamType::Enum { variants, .. } if constructor => {
                            Self::decode_input_enum(
                                builder, *variants, word_pos, input_end, current, true,
                            )
                        }
                        AbiParamType::Enum { variants, .. } => {
                            Self::decode_enum(builder, *variants, word_pos, current, true)
                        }
                        element => Self::decode_aggregate_argument(
                            builder,
                            element,
                            element.mir_type(),
                            word_pos,
                            base,
                            input_end,
                            constructor,
                            current,
                            true,
                            allow_alias,
                        ),
                    };
                    let elem_index = builder.imm_u64(index);
                    builder.memory_object_store_element(
                        ptr,
                        MemoryObjectLayout::word_fixed_array(*len),
                        elem_index,
                        value,
                    );
                    offset += element.checked_head_size().expect("ABI head size exceeds u64 range");
                }
                ptr
            }
            AbiParamType::DynamicArray(element) if matches!(arg_type, MirType::Slice(_)) => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let element_head_size = builder
                    .imm_u64(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                let bytes = Self::checked_mul(builder, len, element_head_size, current);
                let data = builder.add(base, word);
                Self::guard_source_range_value(
                    builder,
                    data,
                    bytes,
                    input_end,
                    constructor,
                    current,
                );
                let location =
                    if constructor { SliceLocation::Memory } else { SliceLocation::Calldata };
                builder.make_slice(data, len, location)
            }
            AbiParamType::DynamicArray(element) if Self::is_full_word_scalar(element) => {
                let word = builder.imm_u64(32);
                let (len, data, bytes) = Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    32,
                );
                if constructor && allow_alias {
                    // ABI word arrays have the same `[length][words...]`
                    // representation as a memory array. The source remains
                    // live for this decode, so keep it in place instead of
                    // allocating and copying an equivalent object.
                    return base;
                }
                let total = if constructor {
                    Self::checked_add(builder, bytes, word, current)
                } else {
                    builder.add(bytes, word)
                };
                let ptr = builder.alloc_object(
                    total,
                    MemoryObjectLayout::WORD_ARRAY,
                    AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, MemoryObjectKind::DynamicArray);
                let location =
                    if constructor { SliceLocation::Memory } else { SliceLocation::Calldata };
                let source = builder.make_slice(data, bytes, location);
                builder.memory_object_copy_from_slice(ptr, MemoryObjectKind::DynamicArray, source);
                ptr
            }
            AbiParamType::DynamicArray(element)
                if constructor && allow_alias && Self::is_scalar_or_enum(element) =>
            {
                let word = builder.imm_u64(32);
                let (len, data, _) = Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    32,
                );

                let zero = builder.imm_u64(0);
                let one = builder.imm_u64(1);
                let preheader = builder.current_block();
                let header = builder.create_block();
                let body = builder.create_block();
                let done = builder.create_block();
                builder.jump(header);

                builder.switch_to_block(header);
                let index = builder.phi(vec![(preheader, zero)]);
                let more = builder.lt(index, len);
                builder.branch(more, body, done);

                builder.switch_to_block(body);
                let mut element_current = builder.current_block();
                let offset = builder.mul(index, word);
                let position = builder.add(data, offset);
                let _ = Self::decode_input_scalar_or_enum(
                    builder,
                    element,
                    position,
                    input_end,
                    &mut element_current,
                    true,
                );
                builder.switch_to_block(element_current);
                let next = builder.add(index, one);
                let backedge = builder.current_block();
                builder.jump(header);
                builder.add_phi_incoming(index, backedge, next);

                builder.switch_to_block(done);
                *current = done;
                base
            }
            AbiParamType::DynamicArray(element) if matches!(arg_type, MirType::MemoryObject(_)) => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let element_head_size = builder
                    .imm_u64(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                let head_bytes = Self::checked_mul(builder, len, element_head_size, current);
                let head = builder.add(base, word);
                Self::guard_source_range_value(
                    builder,
                    head,
                    head_bytes,
                    input_end,
                    constructor,
                    current,
                );
                let bytes = Self::checked_mul(builder, len, word, current);
                let total = if constructor {
                    Self::checked_add(builder, bytes, word, current)
                } else {
                    builder.add(bytes, word)
                };
                let ptr = builder.alloc_object(
                    total,
                    MemoryObjectLayout::WORD_ARRAY,
                    AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, MemoryObjectKind::DynamicArray);
                let data_base = builder.add(base, word);

                // Dynamic ABI arrays use a head of one word per element. The
                // element value may itself be dynamic, so nested objects are
                // decoded recursively and stored as pointers in this array.
                // Keep the three loop-carried words as MIR phis; materializing
                // a temporary semantic object would add a heap allocation and
                // three loads/stores on every iteration.
                let zero = builder.imm_u64(0);
                let preheader = builder.current_block();
                let cond = builder.create_block();
                let body = builder.create_block();
                let done = builder.create_block();
                builder.jump(cond);

                builder.switch_to_block(cond);
                let remaining = builder.phi(vec![(preheader, len)]);
                let source = builder.phi(vec![(preheader, data_base)]);
                let destination_index = builder.phi(vec![(preheader, zero)]);
                let zero = builder.imm_u64(0);
                let has_next = builder.gt(remaining, zero);
                builder.branch(has_next, body, done);

                builder.switch_to_block(body);
                let mut element_current = builder.current_block();
                let value = Self::decode_aggregate_argument(
                    builder,
                    element,
                    element.mir_type(),
                    source,
                    data_base,
                    input_end,
                    constructor,
                    &mut element_current,
                    true,
                    allow_alias,
                );
                builder.memory_object_store_element(
                    ptr,
                    MemoryObjectLayout::WORD_ARRAY,
                    destination_index,
                    value,
                );
                let one = builder.imm_u64(1);
                let next_remaining = builder.sub(remaining, one);
                let element_head_size = builder
                    .imm_u64(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                let next_source = builder.add(source, element_head_size);
                let next_destination_index = builder.add(destination_index, one);
                builder.switch_to_block(element_current);
                builder.jump(cond);
                builder.add_phi_incoming(remaining, element_current, next_remaining);
                builder.add_phi_incoming(source, element_current, next_source);
                builder.add_phi_incoming(
                    destination_index,
                    element_current,
                    next_destination_index,
                );

                builder.switch_to_block(done);
                *current = done;
                ptr
            }
            AbiParamType::Bytes if matches!(arg_type, MirType::Slice(_)) => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let data = builder.add(base, word);
                Self::guard_source_range_value(builder, data, len, input_end, constructor, current);
                let location =
                    if constructor { SliceLocation::Memory } else { SliceLocation::Calldata };
                builder.make_slice(data, len, location)
            }
            AbiParamType::Bytes => {
                Self::guard_source_range(builder, base, 32, input_end, constructor, current);
                let len = Self::load_input_word(builder, base, input_end, constructor);
                let word = builder.imm_u64(32);
                let thirty_one = builder.imm_u64(31);
                let data = builder.add(base, word);
                Self::guard_source_range_value(builder, data, len, input_end, constructor, current);
                // The source range check bounds calldata lengths before
                // rounding; only memory-input decoding needs the explicit
                // overflow branches used by Solidity's allocator.
                let rounded = if constructor {
                    Self::checked_add(builder, len, thirty_one, current)
                } else {
                    builder.add(len, thirty_one)
                };
                let mask = builder.not(thirty_one);
                let data_size = builder.and(rounded, mask);
                let total = if constructor {
                    Self::checked_add(builder, data_size, word, current)
                } else {
                    builder.add(data_size, word)
                };
                let ptr = builder.alloc_object(
                    total,
                    MemoryObjectLayout::Bytes,
                    AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, MemoryObjectKind::Bytes);
                let src = builder.add(base, word);
                let location =
                    if constructor { SliceLocation::Memory } else { SliceLocation::Calldata };
                let source = builder.make_slice(src, len, location);
                builder.memory_object_copy_from_slice(ptr, MemoryObjectKind::Bytes, source);
                ptr
            }
            AbiParamType::Tuple(fields) if Self::is_supported_aggregate(ty) => {
                // Calldata structs with dynamic fields keep their source base
                // in one trailing word so slice expressions can recover the
                // original calldata location after the fields are copied.
                if !head_checked || ty.is_dynamic() {
                    Self::guard_source_range(
                        builder,
                        base,
                        ty.data_head_size(),
                        input_end,
                        constructor,
                        current,
                    );
                }
                if matches!(arg_type, MirType::Slice(SliceLocation::Calldata)) {
                    let length = builder
                        .imm_u64(ty.checked_head_size().expect("ABI head size exceeds u64 range"));
                    return builder.make_slice(base, length, SliceLocation::Calldata);
                }
                if constructor
                    && allow_alias
                    && matches!(arg_type, MirType::MemoryObject(MemoryObjectKind::Struct))
                    && fields.iter().all(Self::is_scalar_or_enum)
                {
                    let mut offset = 0;
                    for field in fields.iter() {
                        let field_offset = builder.imm_u64(offset);
                        let field_head = builder.add(base, field_offset);
                        let _ = Self::decode_input_scalar_or_enum(
                            builder, field, field_head, input_end, current, true,
                        );
                        offset +=
                            field.checked_head_size().expect("ABI head size exceeds u64 range");
                    }
                    return base;
                }
                let carries_base = !constructor && fields.iter().any(AbiParamType::is_dynamic);
                let storage_fields = fields.len() + usize::from(carries_base);
                let size = builder.imm_u64((storage_fields as u64).saturating_mul(32));
                let layout = MemoryObjectLayout::structure(storage_fields as u64);
                let ptr = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
                let mut offset = 0;
                for (index, field) in fields.iter().enumerate() {
                    let field_offset = builder.imm_u64(offset);
                    let field_head = builder.add(base, field_offset);
                    let value = Self::decode_aggregate_argument(
                        builder,
                        field,
                        field.mir_type(),
                        field_head,
                        base,
                        input_end,
                        constructor,
                        current,
                        true,
                        allow_alias,
                    );
                    builder.memory_object_store_field(ptr, layout, index as u64, value);
                    offset += field.checked_head_size().expect("ABI head size exceeds u64 range");
                }
                if carries_base {
                    builder.memory_object_store_field(ptr, layout, fields.len() as u64, base);
                }
                ptr
            }
            _ => builder.undef(arg_type),
        }
    }

    /// Decodes an ABI tuple from an absolute memory range into semantic MIR values.
    ///
    /// The same routine serves `abi.decode` and external/constructor wrappers. The caller
    /// supplies the tuple base and byte length; this entry point turns that range into the
    /// checked memory-input form used by the recursive aggregate decoder.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decode_memory_tuple(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        length: ValueId,
        layout: &AbiParamLayout,
        allow_alias: bool,
    ) -> Option<Vec<ValueId>> {
        let mut current = builder.current_block();
        let head_size = layout.checked_head_size()?;
        builder.switch_to_block(current);
        let input_end = builder.add(base, length);
        let overflow = builder.lt(input_end, base);
        let head_size = builder.imm_u64(head_size);
        let short = builder.lt(length, head_size);
        let invalid = builder.or(overflow, short);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(invalid, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        current = next;

        let mut values = Vec::with_capacity(layout.types.len());
        let static_layout = layout.types.iter().all(|ty| !ty.is_dynamic());
        let mut head_offset = 0_u64;
        for ty in &layout.types {
            let offset = builder.imm_u64(head_offset);
            let head = builder.add(base, offset);
            let value = if static_layout {
                Self::decode_static_memory_argument(builder, ty, head, &mut current)
            } else {
                Self::decode_aggregate_argument(
                    builder,
                    ty,
                    ty.mir_type(),
                    head,
                    base,
                    input_end,
                    true,
                    &mut current,
                    true,
                    allow_alias,
                )
            };
            values.push(value);
            head_offset = head_offset
                .checked_add(ty.checked_head_size().expect("ABI head size exceeds u64 range"))?;
        }
        Some(values)
    }

    fn decode_static_memory_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        match ty {
            AbiParamType::Scalar(scalar) => {
                let value = builder.mload(head);
                let value = if let Some(validator) = AbiWordValidator::from_mir_type(*scalar) {
                    let valid = validator.condition(builder, value);
                    let next = builder.create_block();
                    let revert = builder.create_block();
                    builder.branch(valid, next, revert);
                    builder.switch_to_block(revert);
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);
                    builder.switch_to_block(next);
                    *current = next;
                    value
                } else {
                    value
                };
                if *scalar == MirType::Function {
                    let shift = builder.imm_u64(64);
                    builder.shr(shift, value)
                } else {
                    value
                }
            }
            AbiParamType::Enum { variants, .. } => {
                let value = builder.mload(head);
                let variants = builder.imm_u64(*variants);
                let valid = builder.lt(value, variants);
                let next = builder.create_block();
                let revert = builder.create_block();
                builder.branch(valid, next, revert);
                builder.switch_to_block(revert);
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
                builder.switch_to_block(next);
                *current = next;
                value
            }
            AbiParamType::FixedArray { element, len } => {
                let size = builder.imm_u64(len.saturating_mul(32));
                let layout = MemoryObjectLayout::word_fixed_array(*len);
                let object = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
                let mut offset = 0;
                for index in 0..*len {
                    let offset_value = builder.imm_u64(offset);
                    let field = builder.add(head, offset_value);
                    let value =
                        Self::decode_static_memory_argument(builder, element, field, current);
                    let index_value = builder.imm_u64(index);
                    builder.memory_object_store_element(object, layout, index_value, value);
                    offset += element.checked_head_size().expect("ABI head size exceeds u64 range");
                }
                object
            }
            AbiParamType::Tuple(fields) => {
                let size = builder.imm_u64((fields.len() as u64).saturating_mul(32));
                let layout = MemoryObjectLayout::structure(fields.len() as u64);
                let object = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
                let mut offset = 0;
                for (index, field) in fields.iter().enumerate() {
                    let offset_value = builder.imm_u64(offset);
                    let field_head = builder.add(head, offset_value);
                    let value =
                        Self::decode_static_memory_argument(builder, field, field_head, current);
                    builder.memory_object_store_field(object, layout, index as u64, value);
                    offset += field.checked_head_size().expect("ABI head size exceeds u64 range");
                }
                object
            }
            AbiParamType::Bytes | AbiParamType::DynamicArray(_) => {
                unreachable!("dynamic ABI values are not in a static tuple")
            }
        }
    }

    fn load_calldata_word(builder: &mut FunctionBuilder<'_>, position: ValueId) -> ValueId {
        builder.calldataload(position)
    }

    fn decode_scalar(
        builder: &mut FunctionBuilder<'_>,
        scalar: MirType,
        position: ValueId,
        current: &mut BlockId,
        head_checked: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if !head_checked {
            Self::guard_calldata_range(builder, position, 32, current);
        }
        let value = Self::load_calldata_word(builder, position);
        if let Some(validator) = AbiWordValidator::from_mir_type(scalar) {
            let valid = validator.condition(builder, value);
            let next = builder.create_block();
            let revert = builder.create_block();
            builder.branch(valid, next, revert);
            builder.switch_to_block(revert);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);
            builder.switch_to_block(next);
            *current = next;
        }
        if scalar == MirType::Function {
            let shift = builder.imm_u64(64);
            return builder.shr(shift, value);
        }
        value
    }

    fn decode_enum(
        builder: &mut FunctionBuilder<'_>,
        variants: u64,
        position: ValueId,
        current: &mut BlockId,
        head_checked: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if !head_checked {
            Self::guard_calldata_range(builder, position, 32, current);
        }
        let value = Self::load_calldata_word(builder, position);
        let valid = AbiWordValidator::EnumRange(variants).condition(builder, value);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(valid, next, revert);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        value
    }

    fn load_input_word(
        builder: &mut FunctionBuilder<'_>,
        position: ValueId,
        input_end: ValueId,
        constructor: bool,
    ) -> ValueId {
        if constructor {
            let length = builder.sub(input_end, position);
            let slice = builder.make_slice(position, length, SliceLocation::Memory);
            let zero = builder.imm_u64(0);
            builder.memory_slice_load_word(slice, zero)
        } else {
            Self::load_calldata_word(builder, position)
        }
    }

    fn load_input_dynamic_array(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
        element_head_size: u64,
    ) -> (ValueId, ValueId, ValueId) {
        Self::guard_source_range(builder, base, 32, input_end, constructor, current);
        let len = Self::load_input_word(builder, base, input_end, constructor);
        let word = builder.imm_u64(32);
        let head_bytes = if element_head_size == 32 {
            Self::checked_mul(builder, len, word, current)
        } else {
            let element_head_size = builder.imm_u64(element_head_size);
            Self::checked_mul(builder, len, element_head_size, current)
        };
        let data = builder.add(base, word);
        Self::guard_source_range_value(builder, data, head_bytes, input_end, constructor, current);
        (len, data, head_bytes)
    }

    fn decode_input_scalar_or_enum(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        position: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        head_checked: bool,
    ) -> ValueId {
        match ty {
            AbiParamType::Scalar(scalar) => Self::decode_input_scalar(
                builder,
                *scalar,
                position,
                input_end,
                current,
                head_checked,
            ),
            AbiParamType::Enum { variants, .. } => Self::decode_input_enum(
                builder,
                *variants,
                position,
                input_end,
                current,
                head_checked,
            ),
            _ => unreachable!("scalar or enum ABI type expected"),
        }
    }

    fn decode_input_scalar(
        builder: &mut FunctionBuilder<'_>,
        scalar: MirType,
        position: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        head_checked: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if !head_checked {
            Self::guard_input_range(builder, position, 32, input_end, current);
        }
        let length = builder.sub(input_end, position);
        let slice = builder.make_slice(position, length, SliceLocation::Memory);
        let zero = builder.imm_u64(0);
        let value = builder.memory_slice_load_word(slice, zero);
        if let Some(validator) = AbiWordValidator::from_mir_type(scalar) {
            let valid = validator.condition(builder, value);
            let next = builder.create_block();
            let revert = builder.create_block();
            builder.branch(valid, next, revert);
            builder.switch_to_block(revert);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);
            builder.switch_to_block(next);
            *current = next;
        }
        if scalar == MirType::Function {
            let shift = builder.imm_u64(64);
            return builder.shr(shift, value);
        }
        value
    }

    fn decode_input_enum(
        builder: &mut FunctionBuilder<'_>,
        variants: u64,
        position: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        head_checked: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if !head_checked {
            Self::guard_input_range(builder, position, 32, input_end, current);
        }
        let length = builder.sub(input_end, position);
        let slice = builder.make_slice(position, length, SliceLocation::Memory);
        let zero = builder.imm_u64(0);
        let value = builder.memory_slice_load_word(slice, zero);
        let valid = AbiWordValidator::EnumRange(variants).condition(builder, value);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(valid, next, revert);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        value
    }

    fn guard_source_range(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: u64,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
    ) {
        if constructor {
            Self::guard_input_range(builder, start, size, input_end, current);
        } else {
            Self::guard_calldata_range(builder, start, size, current);
        }
    }

    fn guard_source_range_value(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
    ) {
        if constructor {
            Self::guard_input_range_value(builder, start, size, input_end, current);
        } else {
            Self::guard_calldata_range_value(builder, start, size, current);
        }
    }

    fn guard_source_offset(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
    ) -> ValueId {
        if constructor {
            Self::guard_input_offset(builder, base, offset, input_end, current)
        } else {
            Self::guard_calldata_offset(builder, base, offset, current)
        }
    }

    fn guard_input_range(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: u64,
        input_end: ValueId,
        current: &mut BlockId,
    ) {
        let size = builder.imm_u64(size);
        Self::guard_input_range_value(builder, start, size, input_end, current);
    }

    fn guard_input_range_value(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
    ) {
        builder.switch_to_block(*current);
        // All callers establish `start <= input_end` before checking a tail
        // range, so compare against the remaining input instead of forming a
        // potentially overflowing end pointer.
        let remaining = builder.sub(input_end, start);
        let invalid = builder.gt(size, remaining);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(invalid, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
    }

    fn guard_input_offset(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let target = builder.add(base, offset);
        // `base` is the start of a range already checked against `input_end`.
        let remaining = builder.sub(input_end, base);
        let invalid = builder.gt(offset, remaining);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(invalid, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        target
    }

    fn guard_calldata_range(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: u64,
        current: &mut BlockId,
    ) {
        let size = builder.imm_u64(size);
        Self::guard_calldata_range_value(builder, start, size, current);
    }

    fn guard_calldata_range_value(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: ValueId,
        current: &mut BlockId,
    ) {
        builder.switch_to_block(*current);
        let calldata_size = builder.calldatasize();
        // `start` is derived from a checked calldata head or tail offset.
        let remaining = builder.sub(calldata_size, start);
        let invalid = builder.gt(size, remaining);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(invalid, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
    }

    fn guard_calldata_offset(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let target = builder.add(base, offset);
        let calldata_size = builder.calldatasize();
        // `base` is the start of a range already checked against calldata.
        let remaining = builder.sub(calldata_size, base);
        let invalid = builder.gt(offset, remaining);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(invalid, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        target
    }

    fn checked_add(
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let result = builder.add(lhs, rhs);
        let overflow = builder.lt(result, lhs);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(overflow, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        result
    }

    fn checked_mul(
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let result = builder.mul(lhs, rhs);
        let rhs_zero = builder.iszero(rhs);
        let quotient = builder.div(result, rhs);
        let exact = builder.eq(quotient, lhs);
        let valid = builder.or(rhs_zero, exact);
        let overflow = builder.iszero(valid);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(overflow, revert, next);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        *current = next;
        result
    }

    fn is_supported_aggregate(ty: &AbiParamType) -> bool {
        matches!(
            ty,
            AbiParamType::FixedArray { element, .. }
                if Self::is_supported_tuple_field(element)
        ) || matches!(
            ty,
            AbiParamType::DynamicArray(element)
                if Self::is_supported_tuple_field(element)
        ) || matches!(ty, AbiParamType::Bytes)
            || matches!(
                ty,
                AbiParamType::Tuple(fields)
                    if fields.iter().all(Self::is_supported_tuple_field)
            )
    }

    fn is_supported_tuple_field(ty: &AbiParamType) -> bool {
        matches!(ty, AbiParamType::Scalar(_) | AbiParamType::Enum { .. } | AbiParamType::Bytes)
            || matches!(
                ty,
                AbiParamType::FixedArray { element, .. }
                    if Self::is_supported_tuple_field(element)
            )
            || matches!(
                ty,
                AbiParamType::DynamicArray(element)
                    if Self::is_supported_tuple_field(element)
            )
            || matches!(ty, AbiParamType::Tuple(fields) if fields.iter().all(Self::is_supported_tuple_field))
    }

    fn is_constructor_word(ty: &AbiParamType) -> bool {
        matches!(ty, AbiParamType::Scalar(_) | AbiParamType::Enum { .. })
    }

    fn is_full_word_scalar(ty: &AbiParamType) -> bool {
        matches!(
            ty,
            AbiParamType::Scalar(scalar)
                if *scalar == MirType::uint256()
                    || *scalar == MirType::int256()
                    || *scalar == MirType::bytes32()
        )
    }

    fn is_scalar_or_enum(ty: &AbiParamType) -> bool {
        Self::is_constructor_word(ty) && ty.mir_type() != MirType::Function
    }

    fn push_constructor_param_types(params: &mut IndexVec<ArgIdx, MirType>, ty: &AbiParamType) {
        match ty {
            AbiParamType::Scalar(scalar) => {
                params.push(*scalar);
            }
            AbiParamType::Enum { ty, .. } => {
                params.push(*ty);
            }
            AbiParamType::FixedArray { element, len } => {
                for _ in 0..*len {
                    Self::push_constructor_param_types(params, element);
                }
            }
            _ => unreachable!("checked constructor ABI parameter"),
        }
    }

    fn validate_constructor_word(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        ty: MirType,
    ) -> ValueId {
        let Some(validator) = AbiWordValidator::from_mir_type(ty) else { return value };
        let value = Self::validate_constructor_value(builder, value, validator);
        if ty == MirType::Function {
            let shift = builder.imm_u64(64);
            return builder.shr(shift, value);
        }
        value
    }

    fn validate_constructor_enum(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        variants: u64,
    ) -> ValueId {
        Self::validate_constructor_value(builder, value, AbiWordValidator::EnumRange(variants))
    }

    fn validate_constructor_value(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        validator: AbiWordValidator,
    ) -> ValueId {
        let valid = validator.condition(builder, value);
        let next = builder.create_block();
        let revert = builder.create_block();
        builder.branch(valid, next, revert);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);
        builder.switch_to_block(next);
        value
    }

    /// Prepends `if callvalue() != 0 { revert(0, 0) }` to a wrapper.
    ///
    /// The new guard block becomes the entry and falls through into the old
    /// body, so the check costs no extra jump. Injected after the `.body` copy
    /// is taken: internal callers never pay the check.
    fn inject_callvalue_check(func: &mut Function) {
        let old_entry = BlockId::ENTRY;
        let mut builder = FunctionBuilder::new(func);
        let guard = builder.create_block();
        let revert = builder.create_block();
        builder.switch_to_block(guard);
        let value = builder.callvalue();
        builder.branch(value, revert, old_entry);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);

        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        remap_block_order(func, &order);
    }
}

/// Decodes a memory-backed ABI tuple through the shared ABI-layer decoder.
pub(crate) fn decode_memory_tuple(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    length: ValueId,
    layout: &AbiParamLayout,
    allow_alias: bool,
) -> Option<Vec<ValueId>> {
    LowerAbiCx::decode_memory_tuple(builder, base, length, layout, allow_alias)
}

/// An external entry with a body and a selector — the shape a regular wrapper
/// is built for. Receive/fallback entries have no selector; bytes fallbacks use
/// the separate raw-data wrapper above.
fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor
}

/// Whether a fallback uses Solidity's raw bytes input/output ABI.
fn is_bytes_fallback(func: &Function) -> bool {
    func.params.len() == 1
        && matches!(func.params[ArgIdx::new(0)], MirType::Slice(SliceLocation::Calldata))
        && matches!(func.returns.as_slice(), [MirType::MemoryObject(MemoryObjectKind::Bytes)])
}

/// Whether every value-carrying fallback return can use raw bytes returndata.
fn can_lower_bytes_fallback_returns(func: &Function) -> bool {
    func.blocks.iter().all(|block| {
        let Some(Terminator::Return { values }) = &block.terminator else { return true };
        values.len() == 1
            && func.value_ty(values[0]) == Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
    })
}

/// Keep the reserved-name fallback for text MIR produced before constructor
/// attributes were serialized.
fn is_constructor(func: &Function) -> bool {
    func.attributes.is_constructor || func.name.symbol == kw::Constructor
}

/// Whether every value-carrying return has a matching semantic ABI layout.
fn can_encode_live_returns(func: &Function) -> bool {
    func.blocks.iter().all(|block| {
        let Some(Terminator::Return { values }) = &block.terminator else {
            return true;
        };
        values.is_empty()
            || func.abi_returns.as_ref().is_some_and(|layout| layout.types.len() == values.len())
    })
}

fn canonical_input_for_arg<'a>(
    func: &Function,
    value: ValueId,
    input_params: Option<&'a AbiParamLayout>,
) -> Option<&'a AbiParamType> {
    let Value::Arg(index) = func.value(value) else { return None };
    let mut physical = 0;
    for ty in input_params?.types.iter() {
        if LowerAbiCx::is_constructor_word(ty) && physical == index.index() {
            return Some(ty);
        }
        physical += (ty.checked_head_size().expect("ABI head size exceeds u64 range")
            / EvmMemoryLayout::WORD_SIZE) as usize;
    }
    None
}

fn canonical_input_covers_return(input: &AbiParamType, output: &AbiParamType) -> bool {
    match (input, output) {
        (AbiParamType::Scalar(input), AbiParamType::Scalar(output)) => {
            canonical_scalar_covers_return(*input, *output)
        }
        (AbiParamType::Enum { ty: input, .. }, AbiParamType::Scalar(output)) => {
            canonical_scalar_covers_return(*input, *output)
        }
        (
            AbiParamType::Enum { ty: input, variants: input_variants },
            AbiParamType::Enum { ty: output, variants: output_variants },
        ) => input == output && input_variants == output_variants,
        _ => false,
    }
}

fn canonical_scalar_covers_return(input: MirType, output: MirType) -> bool {
    match (input, output) {
        (MirType::UInt(input), MirType::UInt(output)) => input.bits() <= output.bits(),
        (MirType::UInt(input), MirType::Address) => input.bits() <= 160,
        (MirType::Address, MirType::UInt(output)) => output.bits() >= 160,
        (MirType::Int(input), MirType::Int(output)) => input.bits() <= output.bits(),
        (MirType::FixedBytes(input), MirType::FixedBytes(output)) => {
            input.bytes() <= output.bytes()
        }
        (MirType::Bool, MirType::Bool | MirType::UInt(_) | MirType::Int(_) | MirType::Address) => {
            true
        }
        (MirType::Function, MirType::Function) => true,
        _ => input == output,
    }
}

fn reuses_validated_input(
    func: &Function,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    lazy_args: bool,
    output: &AbiParamType,
) -> bool {
    lazy_args
        && canonical_input_for_arg(func, value, input_params)
            .is_some_and(|input| canonical_input_covers_return(input, output))
}

fn canonicalize_return_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    value: ValueId,
) -> ValueId {
    if !ty.needs_return_cleanup() {
        return value;
    }

    match ty {
        AbiParamType::Scalar(ty) => AbiWordValidator::from_return_mir_type(*ty)
            .map_or(value, |validator| validator.cleanup(builder, value)),
        AbiParamType::Enum { ty, variants } => {
            let limit = builder.imm_u64(*variants);
            let valid = builder.lt(value, limit);
            let invalid = builder.iszero(valid);
            builder.panic_if(invalid, PanicCode::EnumConversion);
            AbiWordValidator::from_return_mir_type(*ty)
                .map_or(value, |validator| validator.cleanup(builder, value))
        }
        AbiParamType::Bytes => value,
        AbiParamType::Tuple(fields) => {
            let fields_len = fields.len() as u64;
            let size = builder.imm_u64(fields_len.saturating_mul(EvmMemoryLayout::WORD_SIZE));
            let layout = MemoryObjectLayout::structure(fields_len);
            let output = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
            for (index, field_ty) in fields.iter().enumerate() {
                let field_value = builder.memory_object_load_field(value, layout, index as u64);
                let field_value = canonicalize_return_value(builder, field_ty, field_value);
                builder.memory_object_store_field(output, layout, index as u64, field_value);
            }
            output
        }
        AbiParamType::FixedArray { element, len } => {
            let size = builder.imm_u64(len.saturating_mul(EvmMemoryLayout::WORD_SIZE));
            let layout = MemoryObjectLayout::word_fixed_array(*len);
            let output = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                let element_value = builder.memory_object_load_element(value, layout, index_value);
                let element_value = canonicalize_return_value(builder, element, element_value);
                builder.memory_object_store_element(output, layout, index_value, element_value);
            }
            output
        }
        AbiParamType::DynamicArray(element) => {
            let layout = MemoryObjectLayout::WORD_ARRAY;
            let length = builder.memory_object_len(value, MemoryObjectKind::DynamicArray);
            let one = builder.imm_u64(1);
            let mut current = builder.current_block();
            let words = LowerAbiCx::checked_add(builder, length, one, &mut current);
            let word = builder.imm_u64(EvmMemoryLayout::WORD_SIZE);
            let size = LowerAbiCx::checked_mul(builder, words, word, &mut current);
            let output = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
            builder.set_memory_object_len(output, length, MemoryObjectKind::DynamicArray);

            let preheader = builder.current_block();
            let header = builder.create_block();
            let body = builder.create_block();
            let exit = builder.create_block();
            builder.jump(header);

            builder.switch_to_block(header);
            let zero = builder.imm_u64(0);
            let index = builder.phi(vec![(preheader, zero)]);
            let more = builder.lt(index, length);
            builder.branch(more, body, exit);

            builder.switch_to_block(body);
            let element_value = builder.memory_object_load_element(value, layout, index);
            let element_value = canonicalize_return_value(builder, element, element_value);
            builder.memory_object_store_element(output, layout, index, element_value);
            let next = builder.add(index, one);
            let backedge = builder.current_block();
            builder.jump(header);
            builder.add_phi_incoming(index, backedge, next);

            builder.switch_to_block(exit);
            output
        }
    }
}

/// Rewrites value-carrying returns into a semantic ABI encode followed by
/// `returndata(slice_ptr(encoded), slice_len(encoded))`.
fn encode_live_returns(
    func: &mut Function,
    return_params: Option<&AbiParamLayout>,
    input_params: Option<&AbiParamLayout>,
    lazy_args: bool,
) -> usize {
    let Some(layout) = func.abi_returns.clone() else { return 0 };
    if !layout.types.iter().any(AbiType::is_dynamic) {
        // Static return data occupies the low-memory ABI buffer. Keep the
        // backend spill area above it so a cross-block value cannot be
        // overwritten while the return tuple is encoded.
        func.external_static_return_size = layout.head_size();
    }
    let block_ids: Vec<_> = func.blocks.indices().collect();
    let return_types = func.returns.clone();
    let return_params = return_params.map(|layout| layout.types.clone());
    let mut encoded_returns = 0;
    for block_id in block_ids {
        let values = match func.blocks[block_id].terminator.take() {
            Some(Terminator::Return { values }) if !values.is_empty() => values.into_vec(),
            Some(terminator) => {
                func.blocks[block_id].terminator = Some(terminator);
                continue;
            }
            None => continue,
        };
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block_id);
        let values = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let Some(ty) = return_params
                    .as_ref()
                    .and_then(|params| params.get(index).cloned())
                    .or_else(|| return_types.get(index).copied().map(AbiParamType::Scalar))
                else {
                    return value;
                };
                if reuses_validated_input(builder.func(), value, input_params, lazy_args, &ty) {
                    value
                } else {
                    canonicalize_return_value(&mut builder, &ty, value)
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        if layout.types.iter().any(AbiType::is_dynamic) {
            let encoded = builder.abi_encode(layout.clone(), None, values);
            let offset = builder.slice_ptr(encoded);
            let size = builder.slice_len(encoded);
            builder.ret_data(offset, size);
        } else {
            let offset = builder.imm_u64(EvmMemoryLayout::HEAP_START);
            let size = super::lower_abi_encode::encode_static_tuple(
                &mut builder,
                &values,
                &layout.types,
                offset,
            );
            builder.ret_data(offset, size);
        }
        encoded_returns += 1;
    }
    encoded_returns
}
