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
        AbiLayout, AbiParamLayout, AbiParamLayoutRef, AbiParamLocation, AbiParamType, AbiType,
        AbiWordValidator, AllocationKind, AllocationSemantics, ArgIdx, BlockId, FrameMode,
        FrameSlotKind, Function, FunctionBuilder, FunctionId, InstId, InstKind, MangledSymbol,
        MemoryObjectKind, MemoryObjectLayout, MirPhase, MirType, Module, PanicCode, RevertReason,
        SliceLocation, Terminator, Value, ValueId,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, RevertStrings};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Ident, Span, Symbol, sym};

/// ABI phase lowering pass.
pub(crate) struct LowerAbi;

impl MirPass for LowerAbi {
    fn name(&self) -> &'static str {
        "lower-abi"
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase <= MirPhase::Optimized
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        LowerAbiCx { revert_strings: gcx.sess.opts.revert_strings, ..Default::default() }.run(
            module,
            gcx.sess.opts.evm_version,
            gcx.sess.opts.optimization.is_gas(),
        )
    }
}

#[derive(Debug, Default)]
struct LowerAbiCx {
    aggregate_type_helpers: FxHashMap<AbiParamType, FunctionId>,
    calldata_slice_helper: Option<FunctionId>,
    return_cleanup_helpers: FxHashMap<AbiParamType, FunctionId>,
    function_params: IndexVec<FunctionId, Vec<MirType>>,
    has_bitwise_shifting: bool,
    /// How compiler-generated decoding reverts are encoded.
    revert_strings: RevertStrings,
}

#[derive(Clone, Copy)]
struct DecodeOptions<'a> {
    constructor: bool,
    input_end: ValueId,
    head_checked: bool,
    allow_alias: bool,
    validate_array_elements: bool,
    helpers: Option<&'a FxHashMap<AbiParamType, FunctionId>>,
    has_bitwise_shifting: bool,
    /// The debug message for an out-of-range head offset, which depends on whether the value is
    /// a tuple element, a struct member, or an array element.
    offset_reason: RevertReason,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReturnValueSource {
    Scalar,
    Memory,
}

#[derive(Default)]
struct CanonicalCallProof<'a> {
    module: Option<&'a Module>,
    visiting: FxHashSet<(FunctionId, AbiParamType)>,
}

struct StaticBytesReturn {
    len: u64,
    word: U256,
}

impl DecodeOptions<'_> {
    fn new(constructor: bool, input_end: ValueId, has_bitwise_shifting: bool) -> Self {
        Self {
            constructor,
            input_end,
            head_checked: false,
            allow_alias: false,
            validate_array_elements: true,
            helpers: None,
            has_bitwise_shifting,
            offset_reason: RevertReason::InvalidTupleOffset,
        }
    }

    fn checked(self) -> Self {
        Self { head_checked: true, ..self }
    }

    /// Decodes a value nested in an aggregate whose offsets report `offset_reason`.
    fn nested(self, offset_reason: RevertReason) -> Self {
        Self { offset_reason, ..self }
    }
}

impl LowerAbiCx {
    fn run(&mut self, module: &mut Module, evm_version: EvmVersion, gas_mode: bool) -> bool {
        // Idempotent: only `built`/`optimized` modules have an implicit ABI
        // boundary to materialize.
        if module.phase >= MirPhase::Abi {
            return false;
        }
        self.has_bitwise_shifting = evm_version.has_bitwise_shifting();
        self.function_params =
            module.functions.iter().map(|func| func.params.iter().copied().collect()).collect();

        let mut targets = Vec::new();
        let mut constructors = Vec::new();
        let mut bytes_fallback = None;
        let mut has_decodes = false;
        let mut has_revert_returndata = false;
        let mut rejecting_constructor = None;
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) {
                targets.push(id);
                if !can_encode_live_returns(func) {
                    return false;
                }
            }
            if func.attributes.is_constructor {
                if super::utils::rejects_callvalue(func) {
                    rejecting_constructor = Some(id);
                }
                if func.abi_params.is_some() {
                    if Self::can_wrap_constructor_params(func) {
                        constructors.push(id);
                    } else {
                        return false;
                    }
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
                if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
                    internally_called.insert(function);
                }
            }
            has_revert_returndata |= func
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(Terminator::RevertReturndata)));
        }

        if targets.is_empty()
            && constructors.is_empty()
            && !has_decodes
            && !has_revert_returndata
            && bytes_fallback.is_none()
            && rejecting_constructor.is_none()
        {
            module.advance_phase(MirPhase::Abi);
            return true;
        }

        self.synthesize_shared_calldata_slice_helpers(module, &targets);
        self.synthesize_shared_aggregate_type_helpers(module, &targets);
        if gas_mode {
            self.synthesize_shared_return_cleanup_helpers(module, &targets);
        }
        let canonical_return_calls = if gas_mode {
            find_canonical_return_calls(module, &targets, &self.return_cleanup_helpers)
        } else {
            FxHashSet::default()
        };

        if has_decodes && !self.lower_decode_instructions(module) {
            return false;
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
        // unguarded. Libraries never check value, since a `DELEGATECALL`
        // sees the caller's; `lower-dispatch` guards their non-view entries
        // against direct calls instead.
        let hoist_callvalue = !module.is_library && callvalue.hoists();

        for id in constructors {
            let layout = module.function_mut(id).abi_params.take();
            self.inject_abi_prologue(module.function_mut(id), layout.as_ref(), true, false);
            Self::clear_abi_inputs(module.function_mut(id));
        }
        if let Some(id) = rejecting_constructor {
            self.inject_callvalue_check(module.function_mut(id));
        }

        let mut body_of_wrapper = FxHashMap::default();
        for id in targets {
            if let Some(body_id) = self.wrap_function(
                module,
                id,
                internally_called.contains(id),
                gas_mode,
                &canonical_return_calls,
            ) {
                body_of_wrapper.insert(id, body_id);
            }
            if !module.is_library
                && !hoist_callvalue
                && super::utils::rejects_callvalue(module.function(id))
            {
                self.inject_callvalue_check(module.function_mut(id));
            }
        }

        // Internal calls to a wrapped public/external function must keep the
        // original call semantics: retarget them to the extracted body. The
        // wrappers' own calls already target the bodies and are not affected.
        if !body_of_wrapper.is_empty() {
            for func in module.functions.iter_mut() {
                func.for_each_instruction_mut(|_, inst| {
                    if let InstKind::ICall { function, .. } = &mut inst.kind
                        && let Some(&body_id) = body_of_wrapper.get(function)
                    {
                        *function = body_id;
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
            let blocks = func.blocks.indices();
            for block in blocks {
                if !matches!(func.blocks[block].terminator, Some(Terminator::RevertReturndata)) {
                    continue;
                }

                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                builder.inherit_terminator_debug_context(block);
                let zero = builder.imm(U256::ZERO);
                if evm_version.supports_returndata() {
                    // size = returndatasize()
                    // returndatacopy(0, 0, size)
                    // revert(0, returndatasize())
                    let copy_size = builder.returndatasize();
                    builder.returndatacopy_abi_return(zero, zero, copy_size);
                    let revert_size = builder.returndatasize();
                    builder.revert(zero, revert_size);
                } else {
                    // revert(0, 0)
                    builder.revert_with(RevertReason::Empty);
                }
            }
        }
    }

    fn can_wrap_constructor_params(func: &Function) -> bool {
        let Some(layout) = &func.abi_params else { return false };
        func.params.len() == layout.types.len()
            && layout
                .types
                .iter()
                .zip(&func.params)
                .all(|(abi_ty, &param_ty)| abi_ty.mir_type() == param_ty)
    }

    /// Rewrites value-carrying returns into a semantic ABI encode followed by
    /// `returndata(slice_ptr(encoded), slice_len(encoded))`.
    fn encode_live_returns(
        &self,
        func: &mut Function,
        return_params: Option<&AbiParamLayout>,
        input_params: Option<&AbiParamLayout>,
        canonical_calls: &FxHashSet<(FunctionId, AbiParamType)>,
        static_bytes_return: Option<StaticBytesReturn>,
    ) {
        let cleanup_helpers = &self.return_cleanup_helpers;
        let has_bitwise_shifting = self.has_bitwise_shifting;
        let revert_strings = self.revert_strings;
        let Some(mut layout) = func.abi_returns.take() else { return };
        let return_blocks = func
            .blocks
            .indices()
            .filter(|&block| {
                matches!(func.blocks[block].terminator, Some(Terminator::Return { ref values }) if !values.is_empty())
            })
            .collect::<Vec<_>>();
        if let Some(value) = static_bytes_return {
            let [return_block] = return_blocks.as_slice() else {
                unreachable!("static bytes return has one return block")
            };
            encode_static_bytes_return(func, *return_block, value);
            return;
        }
        let calldata_returns = if let [return_block] = return_blocks.as_slice() {
            let values = match &func.blocks[*return_block].terminator {
                Some(Terminator::Return { values }) => values.to_vec(),
                _ => unreachable!("return block collected above"),
            };
            let (replacements, calldata_indices) =
                reuse_direct_calldata_returns(func, *return_block, &values, &layout);
            if !calldata_indices.is_empty() {
                let layout = std::sync::Arc::make_mut(&mut layout);
                for index in calldata_indices {
                    *layout.types.get_mut(index).expect("ABI return shape exists") =
                        AbiType::Bytes(SliceLocation::Calldata);
                }
            }
            replacements
        } else {
            FxHashMap::default()
        };
        if !layout.types.iter().any(crate::mir::AbiType::is_dynamic) {
            // Static return data occupies the low-memory ABI buffer. Keep the
            // backend spill area above it so a cross-block value cannot be
            // overwritten while the return tuple is encoded.
            func.external_static_return_size = layout.head_size();
        }
        let return_types = func.returns.clone();
        for block_id in return_blocks {
            let values = match func.blocks[block_id].terminator.take() {
                Some(Terminator::Return { values }) => values.into_vec(),
                _ => unreachable!("return block changed unexpectedly"),
            };
            let mut builder = FunctionBuilder::new(func).with_revert_strings(revert_strings);
            builder.switch_to_block(block_id);
            // abi_encode(values) !metadata(return span)
            // returndata(encoded_ptr, encoded_len) !metadata(return span)
            builder.inherit_terminator_debug_context(block_id);
            let values = values
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    let value = calldata_returns.get(&value).copied().unwrap_or(value);
                    let Some(ty) = return_params
                        .and_then(|layout| layout.types.get(index).cloned())
                        .or_else(|| return_types.get(index).copied().map(AbiParamType::Scalar))
                    else {
                        return value;
                    };
                    if return_types.get(index) == Some(&MirType::Slice(SliceLocation::Calldata))
                        && matches!(ty, AbiParamType::Tuple(_) | AbiParamType::FixedArray { .. })
                    {
                        return materialize_calldata_return(
                            &mut builder,
                            &ty,
                            value,
                            has_bitwise_shifting,
                        );
                    }
                    if let Some(&helper) = cleanup_helpers.get(&ty)
                        && builder.func().value_ty(value) == Some(ty.mir_type())
                    {
                        if is_canonical_return_call(
                            builder.func(),
                            block_id,
                            &ty,
                            value,
                            canonical_calls,
                        ) || is_canonical_return_value(
                            builder.func(),
                            &ty,
                            value,
                            input_params,
                            ReturnValueSource::Scalar,
                        ) {
                            value
                        } else {
                            builder.icall(helper, vec![value], ty.mir_type(), 1)
                        }
                    } else {
                        canonicalize_return_value(
                            &mut builder,
                            &ty,
                            value,
                            input_params,
                            ReturnValueSource::Scalar,
                        )
                    }
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            if layout.types.iter().any(crate::mir::AbiType::is_dynamic) {
                let encoded = builder.abi_encode(layout.clone(), None, values);
                let offset = builder.slice_ptr(encoded);
                let size = builder.slice_len(encoded);
                builder.ret_data(offset, size);
            } else {
                let offset = builder.imm(EvmMemoryLayout::HEAP_START);
                let size = super::lower_abi_encode::encode_static_tuple(
                    &mut builder,
                    &values,
                    &layout.types,
                    offset,
                );
                builder.ret_data(offset, size);
            }
        }
    }

    /// Creates a builder for `func` that encodes revert reasons per the session's settings.
    fn builder<'a>(&self, func: &'a mut Function) -> FunctionBuilder<'a> {
        FunctionBuilder::new(func).with_revert_strings(self.revert_strings)
    }

    fn lower_decode_instructions(&self, module: &mut Module) -> bool {
        let mut decode_counts = FxHashMap::default();
        let mut memory_type_counts = FxHashMap::default();
        let mut decode_functions = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            for inst_id in func.instructions() {
                let InstKind::AbiDecode { data: _, layout } = &func.inst(inst_id).kind else {
                    continue;
                };
                decode_functions.insert(func_id);
                if layout.types.is_empty() || layout.checked_head_size().is_none() {
                    return false;
                }
                let count = decode_counts.entry(layout.clone()).or_insert(0);
                if *count == 0 {
                    for ty in &layout.types {
                        count_dynamic_tuple_types(ty, 1, &mut memory_type_counts);
                    }
                }
                *count += 1;
            }
        }

        let mut memory_types = memory_type_counts
            .into_iter()
            .filter_map(|(ty, (count, first))| (count >= 2).then_some((first, ty)))
            .collect::<Vec<_>>();
        memory_types.sort_by_key(|(first, ty)| (abi_param_type_depth(ty), *first));

        let mut memory_type_helpers = FxHashMap::default();
        for (_, ty) in memory_types {
            let helper = synthesize_memory_decode_helper(
                self.revert_strings,
                module,
                &ty,
                &memory_type_helpers,
                self.has_bitwise_shifting,
            );
            memory_type_helpers.insert(ty, helper);
        }

        let mut decode_helpers = FxHashMap::default();
        for (layout, count) in decode_counts {
            if count >= 2 && layout.types.len() == 1 && !layout.types[0].is_dynamic() {
                let helper = self.synthesize_decode_helper(
                    module,
                    layout.clone(),
                    sym::decode_static,
                    &memory_type_helpers,
                );
                decode_helpers.insert(layout, helper);
            } else if count >= 2 && layout.types.iter().any(AbiParamType::is_dynamic) {
                let helper = self.synthesize_decode_helper(
                    module,
                    layout.clone(),
                    sym::decode_aggregate,
                    &memory_type_helpers,
                );
                decode_helpers.insert(layout, helper);
            }
        }

        for func_id in decode_functions.iter() {
            let func = module.function_mut(func_id);
            let mut replacements = FxHashMap::default();
            let mut builder = self.builder(func);
            let blocks = builder.func().blocks.indices();
            for block in blocks {
                let instructions =
                    std::mem::take(&mut builder.func_mut().blocks[block].instructions);
                let (terminator, terminator_metadata) =
                    builder.func_mut().blocks[block].take_terminator();
                builder.switch_to_block(block);
                for inst in instructions {
                    let InstKind::AbiDecode { data, layout } = &builder.func().inst(inst).kind
                    else {
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(inst);
                        continue;
                    };
                    let data = crate::mir::utils::resolve_replacement(*data, &replacements);
                    let layout = layout.clone();

                    let result = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("ABI decode must produce a value");
                    let data = if matches!(builder.func().value_ty(data), Some(MirType::MemPtr)) {
                        Self::materialize_static_decode_bytes(&mut builder, data, &layout)
                    } else {
                        data
                    };
                    if let Some(&helper) = decode_helpers.get(layout.as_ref()) {
                        let return_count = layout.types.len();
                        let value = builder.icall(
                            helper,
                            vec![data],
                            layout.types[0].mir_type(),
                            return_count,
                        );
                        replacements.insert(result, value);
                        continue;
                    }

                    let base = builder.memory_object_data(data, MemoryObjectKind::Bytes);
                    let length = builder.memory_object_len(data, MemoryObjectKind::Bytes);
                    let Some(values) = decode_memory_tuple(
                        &mut builder,
                        base,
                        length,
                        layout.as_ref(),
                        (!memory_type_helpers.is_empty()).then_some(&memory_type_helpers),
                        self.has_bitwise_shifting,
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
                            let index = builder.imm(index as u64);
                            builder.memory_object_store_element(
                                object,
                                object_layout,
                                index,
                                value,
                            );
                        }
                    }
                }
                super::lower_abi_encode::move_terminator(
                    &mut builder,
                    block,
                    terminator,
                    terminator_metadata,
                );
            }
            builder.func_mut().replace_uses_canonicalized(&replacements);
            let _ = crate::mir::utils::repair_reachability_phis(builder.func_mut());
        }
        !decode_functions.is_empty()
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
                if !ty.is_scalar_word() && matches!(arg_type, MirType::MemoryObject(_)) {
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

    fn synthesize_calldata_aggregate_type_helper(
        &self,
        module: &mut Module,
        ty: AbiParamType,
    ) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(sym::decode_calldata_type));
        {
            let mut builder = self.builder(&mut function);
            let head = builder.add_param(MirType::uint256());
            let tuple_base = builder.imm(4);
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let value = Self::decode_aggregate_argument(
                &mut builder,
                &ty,
                ty.mir_type(),
                head,
                tuple_base,
                &mut current,
                DecodeOptions::new(false, input_end, self.has_bitwise_shifting).checked(),
            );
            builder.add_return(ty.mir_type());
            builder.ret([value]);
        }
        module.add_function(function)
    }

    fn synthesize_shared_calldata_slice_helpers(
        &mut self,
        module: &mut Module,
        targets: &[FunctionId],
    ) {
        let count = targets
            .iter()
            .map(|&id| {
                let func = module.function(id);
                let uses = func.arg_uses();
                func.abi_params.as_ref().map_or(0, |layout| {
                    layout
                        .types
                        .iter()
                        .zip(&func.params)
                        .enumerate()
                        .filter(|&(index, (ty, arg_type))| {
                            matches!(ty, AbiParamType::Bytes)
                                && (matches!(arg_type, MirType::Slice(SliceLocation::Calldata))
                                    || (matches!(arg_type, MirType::MemoryObject(_))
                                        && func
                                            .abi_param_locations
                                            .as_deref()
                                            .and_then(|locations| locations.get(index))
                                            == Some(&AbiParamLocation::Memory)
                                        && uses
                                            .get(ArgIdx::new(index))
                                            .is_none_or(|uses| uses.is_empty())))
                        })
                        .count()
                })
            })
            .sum::<usize>();
        if count >= 2 {
            self.calldata_slice_helper = Some(self.synthesize_calldata_slice_helper(module));
        }
    }

    fn synthesize_shared_return_cleanup_helpers(
        &mut self,
        module: &mut Module,
        targets: &[FunctionId],
    ) {
        let mut counts = FxHashMap::<AbiParamType, usize>::default();
        for &id in targets {
            let Some(layout) = module.function(id).abi_return_params.as_ref() else { continue };
            for ty in &layout.types {
                if !ty.is_scalar_word() && ty.needs_return_cleanup() {
                    *counts.entry(ty.clone()).or_default() += 1;
                }
            }
        }
        for (ty, count) in counts {
            if count < 2 {
                continue;
            }
            let helper = self.synthesize_return_cleanup_helper(module, &ty);
            self.return_cleanup_helpers.insert(ty, helper);
        }
    }

    fn synthesize_return_cleanup_helper(
        &self,
        module: &mut Module,
        ty: &AbiParamType,
    ) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(sym::cleanup_return));
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let value = builder.add_param(ty.mir_type());
            builder.add_return(ty.mir_type());
            let value =
                canonicalize_return_value(&mut builder, ty, value, None, ReturnValueSource::Scalar);
            builder.ret([value]);
        }
        module.add_function(function)
    }

    fn synthesize_calldata_slice_helper(&self, module: &mut Module) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(sym::decode_calldata_slice));
        {
            let mut builder = self.builder(&mut function);
            let head = builder.add_param(MirType::uint256());
            let tuple_base = builder.imm(4);
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let (base, _, _) = Self::decode_calldata_bytes_slice_values(
                &mut builder,
                head,
                tuple_base,
                input_end,
                &mut current,
                true,
                RevertReason::InvalidTupleOffset,
            );
            builder.add_return(MirType::uint256());
            builder.ret([base]);
        }
        module.add_function(function)
    }

    fn synthesize_decode_helper(
        &self,
        module: &mut Module,
        layout: AbiParamLayoutRef,
        name: Symbol,
        memory_type_helpers: &FxHashMap<AbiParamType, FunctionId>,
    ) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(name));
        {
            let mut builder = self.builder(&mut function);
            let data = builder.add_param(MirType::MemoryObject(MemoryObjectKind::Bytes));
            for ty in &layout.types {
                builder.add_return(ty.mir_type());
            }
            let base = builder.memory_object_data(data, MemoryObjectKind::Bytes);
            let length = builder.memory_object_len(data, MemoryObjectKind::Bytes);
            let values = decode_memory_tuple(
                &mut builder,
                base,
                length,
                layout.as_ref(),
                (!memory_type_helpers.is_empty()).then_some(memory_type_helpers),
                self.has_bitwise_shifting,
            )
            .expect("checked ABI layout");
            builder.ret(values);
        }
        module.add_function(function)
    }

    fn materialize_static_decode_bytes(
        builder: &mut FunctionBuilder<'_>,
        data: ValueId,
        layout: &AbiParamLayout,
    ) -> ValueId {
        let size = builder.imm(layout.checked_head_size().expect("static ABI layout"));
        let object = builder.alloc_bytes_object(size, AllocationSemantics::INTERNAL);
        let source = builder.make_slice(data, size, SliceLocation::Memory);
        builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
    }

    fn validate_memory_tuple_input(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        length: ValueId,
        head_size: u64,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let input_end = builder.add(base, length);
        let overflow = builder.lt(input_end, base);
        let head_size = builder.imm(head_size);
        let short = builder.lt(length, head_size);
        let invalid = builder.or(overflow, short);
        *current = builder.revert_if(invalid, RevertReason::TupleDataTooShort);
        input_end
    }

    /// Rewrites one external function into a self-decoding form, keeping a
    /// pristine copy for internal callers.
    ///
    /// The original function keeps its selector and loses its MIR parameter
    /// and return lists, but its `Value::Arg` entries stay in place. Scalar arguments
    /// continue to denote ABI head words, while logical calldata slices are
    /// projected by `lower-slices`. Keeping these values rematerializable avoids
    /// spills across branches and repeated uses. Return values are ABI-encoded
    /// in place, and no internal call is introduced on the external path. When
    /// the function has internal callers, a pristine `.body` copy with raw
    /// returns and parameters preserved is appended and those callers are
    /// retargeted to it.
    fn wrap_function(
        &mut self,
        module: &mut Module,
        wrapper_id: FunctionId,
        needs_body: bool,
        gas_mode: bool,
        canonical_return_calls: &FxHashSet<(FunctionId, AbiParamType)>,
    ) -> Option<FunctionId> {
        let original = module.function(wrapper_id).clone();
        let static_bytes_return = static_bytes_return(&original);
        // Keep the external body in place only when several dynamic
        // aggregates can benefit from calldata aliases. A single aggregate
        // does not repay the duplicate body in the gas/size trade-off.
        let keep_external_body = gas_mode
            && original.abi_params.as_ref().is_some_and(|layout| {
                layout.types.iter().filter(|ty| ty.is_dynamic()).count() >= 3
            });
        let original_entry_inst = original.blocks[BlockId::ENTRY].instructions.first().copied();
        let call_body = !keep_external_body
            && needs_body
            && Self::can_call_body(&original, original.abi_params.as_ref())
            && original_entry_inst.is_some();
        let abi_params = original.abi_params.clone();
        // The copy must precede wrapper mutation and callvalue injection so
        // internal callers keep the original function semantics.
        let body_id = needs_body.then(|| module.add_function(Self::internal_body(&original)));

        let logical_values = self.inject_abi_prologue(
            module.function_mut(wrapper_id),
            abi_params.as_ref(),
            false,
            call_body,
        );
        if call_body && logical_values.iter().all(Option::is_some) {
            Self::replace_body_with_call(
                module.function_mut(wrapper_id),
                body_id.expect("body clone for a calling wrapper"),
                original_entry_inst.expect("calling wrapper has an entry instruction"),
                &original.returns,
                logical_values.into_iter().map(Option::unwrap).collect(),
            );
        }
        let return_params = module.function_mut(wrapper_id).abi_return_params.take();
        self.encode_live_returns(
            module.function_mut(wrapper_id),
            return_params.as_ref(),
            abi_params.as_ref(),
            canonical_return_calls,
            static_bytes_return,
        );

        // External wrappers take no MIR arguments; constructor parameters
        // retain their physical ABI head words for deployment codegen.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.params.clear();
        wrapper.returns.clear();
        Self::clear_abi_metadata(wrapper);
        body_id
    }

    fn can_call_body(func: &Function, abi_params: Option<&AbiParamLayout>) -> bool {
        let Some(layout) = abi_params else { return false };
        layout.types.len() == func.params.len()
            && layout.types.iter().zip(&func.params).all(|(abi_ty, &param_ty)| {
                (abi_ty.is_scalar_word()
                    && !matches!(param_ty, MirType::Function | MirType::MemoryObject(_)))
                    || (!abi_ty.is_scalar_word()
                        && matches!(param_ty, MirType::MemoryObject(_) | MirType::Slice(_)))
            })
            && !func
                .returns
                .iter()
                .any(|&ty| matches!(ty, MirType::Slice(SliceLocation::Returndata)))
            && (func.returns.len() <= 1
                || func.returns.iter().all(|&ty| !matches!(ty, MirType::Slice(_))))
            && func.blocks.iter().all(|block| {
                !matches!(&block.terminator, Some(Terminator::Return { values }) if values.len() != func.returns.len())
            })
    }

    fn internal_body(original: &Function) -> Function {
        let mut body = original.clone();
        body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
        body.name_span = Span::DUMMY;
        body.selector = None;
        Self::clear_abi_metadata(&mut body);
        body.attributes.visibility = solar_sema::hir::Visibility::Internal;
        body.for_each_instruction_mut(|_, inst| inst.metadata.set_abi_validation(false));
        body
    }

    fn clear_abi_inputs(func: &mut Function) {
        func.abi_params = None;
        func.abi_param_locations = None;
    }

    fn clear_abi_metadata(func: &mut Function) {
        Self::clear_abi_inputs(func);
        func.abi_returns = None;
        func.abi_return_params = None;
    }

    fn replace_body_with_call(
        func: &mut Function,
        body_id: FunctionId,
        original_entry_inst: crate::mir::InstId,
        return_types: &[MirType],
        args: Vec<ValueId>,
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
        if return_types.is_empty() {
            builder.icall_void(body_id, args, 0);
            builder.ret([]);
        } else if return_types.len() == 1 {
            let result = builder.icall(body_id, args, return_types[0], 1);
            builder.ret([result]);
        } else {
            let result = builder.icall(body_id, args, return_types[0], return_types.len());
            let mut values = Vec::with_capacity(return_types.len());
            values.push(result);
            let base = builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word);
            for index in 1..return_types.len() {
                let index_value = builder.imm(index as u64);
                let value = match return_types[index] {
                    MirType::MemoryObject(kind) => builder.memory_object_load_object(
                        base,
                        MemoryObjectLayout::word_fixed_array(return_types.len() as u64),
                        index_value,
                        kind,
                    ),
                    _ => {
                        let position = builder.add_u64_offset(
                            base,
                            u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32),
                        );
                        builder.mload(position)
                    }
                };
                values.push(value);
            }
            builder.ret(values);
        }
        let _ = crate::mir::utils::repair_reachability_phis(builder.func_mut());
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
        let mut body = Self::internal_body(&original);
        body.attributes.is_fallback = false;
        body.attributes.is_receive = false;
        if !Self::lower_bytes_fallback_returns(&mut body) {
            return false;
        }
        body.returns.clear();
        let body_id = module.add_function(body);

        let mut wrapper = Function::new(Ident::with_dummy_span(original.name.symbol));
        wrapper.name = original.name;
        wrapper.name_span = original.name_span;
        wrapper.attributes = original.attributes;
        {
            let mut builder = FunctionBuilder::new(&mut wrapper);
            let zero = builder.imm(0);
            let length = builder.calldatasize();
            let input = builder.make_slice(zero, length, SliceLocation::Calldata);
            builder.icall_void(body_id, vec![input], 0);
            builder.invalid();
        }
        *module.function_mut(fallback_id) = wrapper;
        true
    }

    /// Rewrites every value-carrying fallback return into raw returndata.
    fn lower_bytes_fallback_returns(func: &mut Function) -> bool {
        let blocks = func.blocks.indices();
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
            builder.inherit_terminator_debug_context(block);
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
        abi_params: Option<&crate::mir::AbiParamLayout>,
        constructor: bool,
        force_memory_aggregates: bool,
    ) -> Vec<Option<ValueId>> {
        let arg_types: Vec<_> = func.params.iter().copied().collect();
        if !constructor
            && arg_types.is_empty()
            && abi_params.is_none_or(|layout| layout.types.is_empty())
        {
            return Vec::new();
        }

        let old_entry = BlockId::ENTRY;
        let arg_uses = func.arg_uses();
        let abi_param_locations = func.abi_param_locations.take();
        let mut logical_values = Vec::new();
        let mut replacements = FxHashMap::default();
        let mut slice_values_to_retag = Vec::new();
        if let Some(layout) = abi_params {
            let mut head_offset = 0_u64;
            let mut logical_physical = Vec::with_capacity(layout.types.len());
            for ty in &layout.types {
                logical_physical.push(
                    ty.is_scalar_word()
                        .then(|| crate::mir::ArgIdx::new((head_offset / 32) as usize)),
                );
                head_offset += ty.checked_head_size().expect("ABI head size exceeds u64 range");
            }
            let preserve_word_types = layout.types.len() == arg_types.len()
                && layout.types.iter().zip(&arg_types).all(|(ty, &param)| {
                    ty.is_scalar_word() && ty.mir_type() == param && param != MirType::Function
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
                    for &use_value in
                        arg_uses.get(crate::mir::ArgIdx::new(logical)).into_iter().flatten()
                    {
                        replacements.insert(use_value, *value);
                    }
                }
            }
        }
        let guard = {
            let mut builder = self.builder(func);
            let guard = builder.create_block();
            let revert = builder.create_block();
            // solc's scalar validators revert with empty data even in debug mode, so they only
            // share the "tuple data too short" block while that block has no message.
            let validator_revert =
                if self.revert_strings.is_debug() { builder.create_block() } else { revert };
            let mut current = guard;

            builder.switch_to_block(current);
            let input_base =
                if constructor { builder.constructor_args_base() } else { builder.imm(4) };
            let input_end =
                if constructor { builder.constructor_args_end() } else { builder.calldatasize() };
            let head_size = abi_params.map_or((arg_types.len() as u64) * 32, |layout| {
                layout.checked_head_size().expect("ABI head size exceeds u64 range")
            });
            let invalid = if constructor {
                let required = builder.add_u64_offset(input_base, head_size);
                builder.gt(required, input_end)
            } else {
                let required = builder.imm(4 + head_size);
                builder.lt(input_end, required)
            };
            let next = builder.create_block();
            builder.branch(invalid, revert, next);
            current = next;

            let mut head_offset = 0;
            for (index, &ty) in arg_types.iter().enumerate() {
                let abi_type = abi_params.and_then(|layout| layout.types.get(index));
                let validator = abi_type
                    .and_then(crate::mir::AbiParamType::word_validator)
                    .or_else(|| AbiWordValidator::from_mir_type(ty));
                head_offset += abi_type.map_or(32, |ty| {
                    ty.checked_head_size().expect("ABI head size exceeds u64 range")
                });
                let Some(validator) = validator else { continue };
                builder.switch_to_block(current);
                // `Value::Arg` values carry the canonicality invariant that this
                // guard establishes. Read the raw input word so an optimizer
                // cannot fold the check away before it runs.
                let offset = if constructor {
                    builder.add_u64_offset(input_base, head_offset - 32)
                } else {
                    builder.imm(4 + head_offset - 32)
                };
                let word = Self::load_input_word(&mut builder, offset, constructor);
                let valid = validator.condition(&mut builder, word, self.has_bitwise_shifting);
                let next = builder.create_block();
                builder.branch(valid, next, validator_revert);
                current = next;
            }

            if let Some(layout) = abi_params {
                let mut head_offset = 0;
                for (index, ty) in layout.types.iter().enumerate() {
                    let arg_index = crate::mir::ArgIdx::new(index);
                    let uses = arg_uses.get(arg_index).map_or(&[][..], Vec::as_slice);
                    let head_size =
                        ty.checked_head_size().expect("ABI head size exceeds u64 range");
                    if ty.is_scalar_word()
                        || !arg_types.get(index).is_some_and(|ty| {
                            matches!(ty, MirType::MemoryObject(_) | MirType::Slice(_))
                        })
                    {
                        head_offset += head_size;
                        continue;
                    }
                    let arg_type = arg_types[index];
                    builder.switch_to_block(current);
                    let (head, tuple_base) = if constructor {
                        let head_offset_value = builder.imm(head_offset);
                        (builder.add(input_base, head_offset_value), input_base)
                    } else {
                        (builder.imm(4 + head_offset), builder.imm(4))
                    };
                    let location = abi_param_locations
                        .as_deref()
                        .and_then(|locations| locations.get(index))
                        .copied()
                        // Text MIR does not carry HIR data locations; default to calldata.
                        .unwrap_or(AbiParamLocation::Calldata);
                    let decode_type = if !force_memory_aggregates
                        && !constructor
                        && location == AbiParamLocation::Calldata
                        && self.can_use_calldata_slice(builder.func(), uses, ty, arg_type)
                    {
                        MirType::Slice(SliceLocation::Calldata)
                    } else {
                        arg_type
                    };
                    let validate_array_elements = constructor
                        || (location == AbiParamLocation::Memory
                            && Self::requires_calldata_element_validation(ty))
                        || !matches!(decode_type, MirType::Slice(SliceLocation::Calldata))
                        || self.needs_full_calldata_array_validation(builder.func(), uses, ty);
                    let decode_options = DecodeOptions {
                        validate_array_elements,
                        ..DecodeOptions::new(constructor, input_end, self.has_bitwise_shifting)
                    };
                    if uses.is_empty() {
                        let validate_only = ty.is_dynamic()
                            && (location != AbiParamLocation::Memory
                                || (!constructor && matches!(ty, AbiParamType::Bytes)));
                        if validate_only {
                            // The shared slice helper reports calldata array reasons, so a
                            // memory-bound `bytes` validates inline when reasons are encoded.
                            if location == AbiParamLocation::Memory
                                && !builder.encodes_revert_reasons()
                                && let Some(helper) = self.calldata_slice_helper
                            {
                                builder.icall_void(helper, vec![head], 1);
                            } else {
                                Self::validate_dynamic_aggregate_argument(
                                    &mut builder,
                                    ty,
                                    head,
                                    tuple_base,
                                    input_end,
                                    constructor,
                                    location == AbiParamLocation::Memory,
                                    &mut current,
                                );
                            }
                        } else if location == AbiParamLocation::Memory {
                            if !constructor
                                && matches!(arg_type, MirType::MemoryObject(_))
                                && let Some(&helper) = self.aggregate_type_helpers.get(ty)
                            {
                                let value = builder.icall(helper, vec![head], arg_type, 1);
                                logical_values[index] = Some(value);
                            } else {
                                let value = Self::decode_aggregate_argument(
                                    &mut builder,
                                    ty,
                                    decode_type,
                                    head,
                                    tuple_base,
                                    &mut current,
                                    decode_options,
                                );
                                logical_values[index] = Some(value);
                            }
                        }
                    } else {
                        let value = if !constructor
                            && decode_type == arg_type
                            && matches!(arg_type, MirType::MemoryObject(_))
                            && let Some(&helper) = self.aggregate_type_helpers.get(ty)
                        {
                            builder.icall(helper, vec![head], arg_type, 1)
                        } else if !constructor
                            && decode_type == arg_type
                            && matches!(arg_type, MirType::Slice(SliceLocation::Calldata))
                            && matches!(ty, AbiParamType::Bytes)
                            && let Some(helper) = self.calldata_slice_helper
                        {
                            let base = builder.icall(helper, vec![head], MirType::uint256(), 1);
                            let len = builder.calldataload(base);
                            let data = builder.add_u64_offset(base, 32);
                            builder.make_slice(data, len, SliceLocation::Calldata)
                        } else {
                            Self::decode_aggregate_argument(
                                &mut builder,
                                ty,
                                decode_type,
                                head,
                                tuple_base,
                                &mut current,
                                // The wrapper guard already checked the complete
                                // top-level ABI head, including static aggregate
                                // fields and dynamic offsets.
                                decode_options.checked(),
                            )
                        };
                        logical_values[index] = Some(value);
                        if matches!(decode_type, MirType::Slice(SliceLocation::Calldata))
                            && Self::is_scalar_array(ty)
                        {
                            slice_values_to_retag.push(value);
                        }
                        for &use_value in uses {
                            replacements.insert(use_value, value);
                        }
                    }
                    head_offset += head_size;
                }
            }

            builder.switch_to_block(current);
            for (logical, value) in logical_values.iter().enumerate() {
                if arg_types.get(logical) != Some(&MirType::Function) {
                    continue;
                }
                let Some(value) = value else { continue };
                let shift = builder.imm(64);
                let value = builder.shr(shift, *value);
                for &use_value in
                    arg_uses.get(crate::mir::ArgIdx::new(logical)).into_iter().flatten()
                {
                    replacements.insert(use_value, value);
                }
            }
            builder.jump(old_entry);

            builder.switch_to_block(revert);
            builder.revert_with(RevertReason::TupleDataTooShort);
            if validator_revert != revert {
                // revert(0, 0)
                builder.switch_to_block(validator_revert);
                builder.revert_with(RevertReason::Empty);
            }

            guard
        };
        func.replace_uses_canonicalized(&replacements);
        for value in slice_values_to_retag {
            Self::retag_calldata_slice_values(func, value);
        }
        Self::rewrite_calldata_canonicalization(func);
        func.for_each_instruction_mut(|_, inst| inst.metadata.set_abi_validation(false));
        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
        logical_values
    }

    /// Reuses a validated calldata slice when ABI lowering created only a
    /// canonicalizing copy for an aggregate encoder.
    fn rewrite_calldata_canonicalization(func: &mut Function) {
        let mut rewrites = Vec::new();
        for inst_id in func.instructions() {
            let InstKind::AbiEncode { args, .. } = &func.inst(inst_id).kind else { continue };
            for (index, &object) in args.iter().enumerate() {
                if let Some(source) = Self::calldata_canonicalization_source(func, object) {
                    rewrites.push((inst_id, index, source));
                }
            }
        }
        for (inst_id, index, source) in rewrites {
            let InstKind::AbiEncode { args, .. } = &mut func.inst_mut(inst_id).kind else {
                continue;
            };
            args[index] = source;
            Self::remove_calldata_canonicalization(func, inst_id, source);
        }
    }

    fn remove_calldata_canonicalization(func: &mut Function, encode: InstId, source: ValueId) {
        let Some(encode_block) =
            func.blocks.indices().find(|&block| func.blocks[block].instructions.contains(&encode))
        else {
            return;
        };
        let Some((start, len)) = func.blocks.indices().find_map(|block| {
            func.blocks[block].instructions.iter().copied().find_map(|inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::MemoryObjectLen(object, _)
                    if object == source)
                .then_some((block, inst_id))
            })
        }) else {
            return;
        };
        if start == encode_block || func.blocks[start].instructions.first() != Some(&len) {
            return;
        }
        func.blocks[start].instructions.clear();
        func.blocks[start].terminator = Some(Terminator::Jump(encode_block));
        let _ = crate::mir::utils::repair_reachability_phis(func);
    }

    fn calldata_canonicalization_source(func: &Function, object: ValueId) -> Option<ValueId> {
        let Value::Inst(alloc) = func.value(object) else { return None };
        let InstKind::Alloc { kind: crate::mir::AllocationKind::Object(layout), .. } =
            &func.inst(*alloc).kind
        else {
            return None;
        };
        if !matches!(layout, MemoryObjectLayout::DynamicArray { .. }) {
            return None;
        }

        let mut source = None;
        let mut stores = 0;
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            match &inst.kind {
                InstKind::SetMemoryObjectLen(..) => {}
                InstKind::MemoryObjectStoreElement {
                    object: destination, index, value, ..
                } if *destination == object => {
                    let Value::Inst(and) = func.value(*value) else { return None };
                    let InstKind::And(lhs, rhs) = &func.inst(*and).kind else { return None };
                    let (load, _mask) = if func.value_u256(*rhs).is_some() {
                        (*lhs, *rhs)
                    } else if func.value_u256(*lhs).is_some() {
                        (*rhs, *lhs)
                    } else {
                        return None;
                    };
                    let Value::Inst(load) = func.value(load) else { return None };
                    let InstKind::MemoryObjectLoadElement {
                        object: source_object,
                        index: load_index,
                        ..
                    } = &func.inst(*load).kind
                    else {
                        return None;
                    };
                    if load_index != index {
                        return None;
                    }
                    if let Some(previous) = source
                        && previous != *source_object
                    {
                        return None;
                    }
                    source = Some(*source_object);
                    stores += 1;
                }
                InstKind::AbiEncode { args, .. } if args.contains(&object) => {}
                _ if inst.operands().contains(&object) => return None,
                _ => {}
            }
        }
        let source = source?;
        (stores != 0
            && matches!(func.value_ty(source), Some(MirType::Slice(SliceLocation::Calldata))))
        .then_some(source)
    }

    /// Validates the immediate ABI shape of a dynamic aggregate without
    /// materializing its memory representation.
    ///
    /// `to_memory` selects the debug messages of the memory decoder for a value that solc
    /// would copy to memory, even though the unused value is only validated here.
    #[allow(clippy::too_many_arguments)]
    fn validate_dynamic_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        constructor: bool,
        to_memory: bool,
        current: &mut BlockId,
    ) {
        builder.switch_to_block(*current);
        let offset = Self::load_input_word(builder, head, constructor);
        let base = Self::guard_input_offset(
            builder,
            tuple_base,
            offset,
            input_end,
            ty,
            current,
            RevertReason::InvalidTupleOffset,
            !constructor && !to_memory,
        );

        match ty {
            crate::mir::AbiParamType::DynamicArray(element) => {
                Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    element.checked_head_size().expect("ABI head size exceeds u64 range"),
                );
            }
            crate::mir::AbiParamType::FixedArray { .. } | crate::mir::AbiParamType::Tuple(..) => {
                let reason = Self::aggregate_short_reason(ty, !constructor && !to_memory);
                Self::guard_input_range(
                    builder,
                    base,
                    ty.data_head_size(),
                    input_end,
                    current,
                    reason,
                );
            }
            crate::mir::AbiParamType::Bytes => {
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                Self::guard_bytes_data(builder, data, len, input_end, current, to_memory);
            }
            crate::mir::AbiParamType::Scalar(_) | crate::mir::AbiParamType::Enum { .. } => {
                unreachable!("scalar ABI value is not a dynamic aggregate")
            }
        }
    }

    fn decode_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        arg_type: MirType,
        head: ValueId,
        tuple_base: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) -> ValueId {
        let DecodeOptions {
            constructor,
            input_end,
            head_checked,
            allow_alias,
            validate_array_elements,
            helpers,
            has_bitwise_shifting,
            offset_reason,
        } = options;
        builder.switch_to_block(*current);
        let is_dynamic = ty.is_dynamic();
        let location = if constructor { SliceLocation::Memory } else { SliceLocation::Calldata };
        let to_calldata =
            !constructor && matches!(arg_type, MirType::Slice(SliceLocation::Calldata));
        // The shared helper checks the value's head offset with the tuple reason, so a struct
        // member decodes inline when reasons are encoded.
        if constructor
            && head_checked
            && (!builder.encodes_revert_reasons()
                || offset_reason == RevertReason::InvalidTupleOffset)
            && let Some(&helper) = helpers.and_then(|helpers| helpers.get(ty))
        {
            return builder.icall(helper, vec![head, tuple_base, input_end], ty.mir_type(), 1);
        }
        if !constructor
            && matches!(ty, crate::mir::AbiParamType::Bytes)
            && matches!(arg_type, MirType::Slice(SliceLocation::Calldata))
        {
            let (_, data, len) = Self::decode_calldata_bytes_slice_values(
                builder,
                head,
                tuple_base,
                input_end,
                current,
                head_checked,
                options.offset_reason,
            );
            return builder.make_slice(data, len, SliceLocation::Calldata);
        }
        let base = if is_dynamic {
            if !head_checked {
                Self::guard_input_range(
                    builder,
                    head,
                    32,
                    input_end,
                    current,
                    RevertReason::TupleDataTooShort,
                );
            }
            let offset = Self::load_input_word(builder, head, constructor);
            Self::guard_input_offset(
                builder,
                tuple_base,
                offset,
                input_end,
                ty,
                current,
                options.offset_reason,
                to_calldata,
            )
        } else {
            head
        };
        if !is_dynamic
            && matches!(arg_type, MirType::MemoryObject(_))
            && !constructor
            && !allow_alias
            && matches!(
                ty,
                crate::mir::AbiParamType::FixedArray { .. } | crate::mir::AbiParamType::Tuple(..)
            )
        {
            if !head_checked {
                Self::guard_input_range(
                    builder,
                    base,
                    ty.checked_head_size().expect("ABI head size exceeds u64 range"),
                    input_end,
                    current,
                    Self::aggregate_short_reason(ty, to_calldata),
                );
            }
            return Self::decode_static_calldata_argument(
                builder,
                ty,
                base,
                current,
                has_bitwise_shifting,
            );
        }
        match ty {
            crate::mir::AbiParamType::Scalar(_) | crate::mir::AbiParamType::Enum { .. } => {
                Self::decode_source_scalar(builder, ty, base, current, options)
            }
            crate::mir::AbiParamType::FixedArray { element, len } => {
                let head_size = ty.data_head_size();
                if !head_checked || is_dynamic {
                    let reason = Self::aggregate_short_reason(ty, to_calldata);
                    Self::guard_input_range(builder, base, head_size, input_end, current, reason);
                }
                if matches!(arg_type, MirType::Slice(SliceLocation::Calldata)) {
                    let length = builder.imm(*len);
                    if validate_array_elements && Self::is_scalar_array(ty) {
                        Self::validate_scalar_array(
                            builder, base, element, length, current, options,
                        );
                    }
                    return builder.make_slice(base, length, SliceLocation::Calldata);
                }
                if constructor && allow_alias && Self::is_scalar_or_enum(element) {
                    let length = builder.imm(*len);
                    Self::validate_scalar_array(builder, base, element, length, current, options);
                    return base;
                }
                let (ptr, layout) =
                    builder.alloc_word_array(*len, crate::mir::AllocationSemantics::INTERNAL);
                let length = builder.imm(*len);
                let stride = builder
                    .imm(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                builder.counted_loop(length, |builder, index| {
                    *current = builder.current_block();
                    let offset = builder.mul(index, stride);
                    let word_pos = builder.add(base, offset);
                    let value = if element.is_scalar_word() {
                        Self::decode_source_scalar(
                            builder,
                            element,
                            word_pos,
                            current,
                            options.checked(),
                        )
                    } else {
                        Self::decode_aggregate_argument(
                            builder,
                            element,
                            element.mir_type(),
                            word_pos,
                            base,
                            current,
                            options.checked().nested(RevertReason::InvalidCalldataArrayOffset),
                        )
                    };
                    builder.switch_to_block(*current);
                    let value = Self::encode_memory_scalar(builder, element, value);
                    builder.memory_object_store_element(ptr, layout, index, value);
                });
                *current = builder.current_block();
                ptr
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if matches!(arg_type, MirType::Slice(_)) =>
            {
                let (len, data) = Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    element.checked_head_size().expect("ABI head size exceeds u64 range"),
                );
                if !constructor && validate_array_elements && Self::is_scalar_or_enum(element) {
                    Self::validate_scalar_array(builder, data, element, len, current, options);
                }
                builder.make_slice(data, len, location)
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if Self::is_full_word_scalar(element) =>
            {
                let word = builder.imm(32);
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                let checked_object = if constructor && !allow_alias {
                    let object = builder.alloc_dynamic_word_array(
                        len,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    *current = builder.current_block();
                    Some(object)
                } else {
                    None
                };
                if constructor && allow_alias {
                    // ABI word arrays have the same `[length][words...]`
                    // representation as a memory array. The source remains
                    // live for this decode, so keep it in place instead of
                    // allocating and copying an equivalent object.
                    return base;
                }
                let bytes = Self::checked_mul(builder, len, word, current);
                let (ptr, layout) = if let Some(object) = checked_object {
                    object
                } else {
                    let total = Self::checked_add(builder, bytes, word, current);
                    let layout = crate::mir::MemoryObjectLayout::WORD_ARRAY;
                    let ptr = builder.alloc_object(
                        total,
                        layout,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    builder.set_memory_object_len(ptr, len, layout.kind());
                    (ptr, layout)
                };
                Self::guard_input_dynamic_array(builder, len, data, input_end, current, 32);
                let source = builder.make_slice(data, bytes, location);
                builder.memory_object_copy_from_slice(ptr, layout.kind(), source);
                ptr
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if constructor && allow_alias && Self::is_scalar_or_enum(element) =>
            {
                let (len, data) = Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    32,
                );
                Self::validate_scalar_array(builder, data, element, len, current, options);
                base
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if matches!(arg_type, MirType::MemoryObject(_)) =>
            {
                let checked_decode = constructor && !allow_alias;
                let len = Self::load_input_word(builder, base, constructor);
                let data_base = builder.add_u64_offset(base, 32);
                let checked_object = if checked_decode {
                    let object = builder.alloc_dynamic_word_array(
                        len,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    *current = builder.current_block();
                    Some(object)
                } else {
                    None
                };
                let word = builder.imm(32);
                let bytes = Self::checked_mul(builder, len, word, current);

                let copy_validated =
                    !constructor && validate_array_elements && Self::is_scalar_or_enum(element);
                let (ptr, layout) = if let Some(object) = checked_object {
                    object
                } else {
                    let total = Self::checked_add(builder, bytes, word, current);
                    let layout = crate::mir::MemoryObjectLayout::WORD_ARRAY;
                    let ptr = builder.alloc_object(
                        total,
                        layout,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    builder.set_memory_object_len(ptr, len, layout.kind());
                    (ptr, layout)
                };
                Self::guard_input_dynamic_array(
                    builder,
                    len,
                    data_base,
                    input_end,
                    current,
                    element.checked_head_size().expect("ABI head size exceeds u64 range"),
                );
                if copy_validated {
                    Self::validate_scalar_array(builder, data_base, element, len, current, options);
                    let source = builder.make_slice(data_base, bytes, SliceLocation::Calldata);
                    builder.memory_object_copy_from_slice(ptr, layout.kind(), source);
                    return ptr;
                }

                // Dynamic ABI arrays use a head of one word per element. The
                // element value may itself be dynamic, so nested objects are
                // decoded recursively and stored as pointers in this array.
                // Keep the three loop-carried words as MIR phis; materializing
                // a temporary semantic object would add a heap allocation and
                // three loads/stores on every iteration.
                let zero = builder.imm(0);
                let preheader = builder.current_block();
                let cond = builder.create_block();
                let body = builder.create_block();
                let done = builder.create_block();
                builder.jump(cond);

                builder.switch_to_block(cond);
                let remaining = builder.phi(vec![(preheader, len)]);
                let source = builder.phi(vec![(preheader, data_base)]);
                let destination_index = builder.phi(vec![(preheader, zero)]);
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
                    &mut element_current,
                    options.checked().nested(RevertReason::InvalidCalldataArrayOffset),
                );
                let value = Self::encode_memory_scalar(builder, element, value);
                builder.memory_object_store_element(ptr, layout, destination_index, value);
                let one = builder.imm(1);
                let next_remaining = builder.sub(remaining, one);
                let element_head_size = builder
                    .imm(element.checked_head_size().expect("ABI head size exceeds u64 range"));
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
            crate::mir::AbiParamType::Bytes if matches!(arg_type, MirType::Slice(_)) => {
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                Self::guard_bytes_data(builder, data, len, input_end, current, false);
                builder.make_slice(data, len, location)
            }
            crate::mir::AbiParamType::Bytes => {
                let len = Self::load_input_word(builder, base, constructor);
                let word = builder.imm(32);
                let data = builder.add(base, word);
                let layout = crate::mir::MemoryObjectLayout::Bytes;
                if !constructor {
                    let total = Self::checked_padded_size(builder, len, current);
                    let ptr = builder.alloc_object(
                        total,
                        layout,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    builder.set_memory_object_len(ptr, len, layout.kind());
                    Self::guard_bytes_data(builder, data, len, input_end, current, true);
                    let source = builder.make_slice(data, len, location);
                    builder.memory_object_copy_from_slice(ptr, layout.kind(), source);
                    return ptr;
                }
                let checked_object = if constructor && !allow_alias {
                    let object = builder.alloc_bytes_object(
                        len,
                        crate::mir::AllocationSemantics::SOLIDITY_UNINITIALIZED,
                    );
                    *current = builder.current_block();
                    Some(object)
                } else {
                    None
                };
                Self::guard_bytes_data(builder, data, len, input_end, current, true);
                let ptr = if let Some(object) = checked_object {
                    object
                } else {
                    let total = Self::checked_padded_size(builder, len, current);
                    let ptr = builder.alloc_object(
                        total,
                        layout,
                        crate::mir::AllocationSemantics::INTERNAL,
                    );
                    builder.set_memory_object_len(ptr, len, layout.kind());
                    ptr
                };
                let source = builder.make_slice(data, len, location);
                builder.memory_object_copy_from_slice(ptr, layout.kind(), source);
                ptr
            }
            crate::mir::AbiParamType::Tuple(fields) => {
                // Calldata structs with dynamic fields keep their source base
                // in one trailing word so slice expressions can recover the
                // original calldata location after the fields are copied.
                if !head_checked || is_dynamic {
                    let reason = Self::aggregate_short_reason(ty, to_calldata);
                    Self::guard_input_range(
                        builder,
                        base,
                        ty.data_head_size(),
                        input_end,
                        current,
                        reason,
                    );
                }
                if matches!(arg_type, MirType::Slice(SliceLocation::Calldata)) {
                    let length = builder
                        .imm(ty.checked_head_size().expect("ABI head size exceeds u64 range"));
                    return builder.make_slice(base, length, SliceLocation::Calldata);
                }
                if constructor
                    && allow_alias
                    && matches!(arg_type, MirType::MemoryObject(MemoryObjectKind::Struct))
                    && fields.iter().all(Self::is_scalar_or_enum)
                {
                    let mut offset = 0;
                    for field in fields.iter() {
                        let field_head = builder.add_u64_offset(base, offset);
                        let _ = Self::decode_source_scalar(
                            builder,
                            field,
                            field_head,
                            current,
                            options.checked(),
                        );
                        offset +=
                            field.checked_head_size().expect("ABI head size exceeds u64 range");
                    }
                    return base;
                }
                let carries_base = !constructor && ty.has_dynamic_child();
                let storage_fields = fields.len() + usize::from(carries_base);
                let (ptr, layout) = builder.alloc_word_struct(
                    storage_fields as u64,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                let mut offset = 0;
                for (index, field) in fields.iter().enumerate() {
                    let field_head = builder.add_u64_offset(base, offset);
                    let value = Self::decode_aggregate_argument(
                        builder,
                        field,
                        field.mir_type(),
                        field_head,
                        base,
                        current,
                        options.checked().nested(RevertReason::InvalidStructOffset),
                    );
                    let value = Self::encode_memory_scalar(builder, field, value);
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

    fn validate_scalar_array(
        builder: &mut FunctionBuilder<'_>,
        data: ValueId,
        element: &crate::mir::AbiParamType,
        len: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) {
        let word = builder.imm(32);
        builder.switch_to_block(*current);
        builder.counted_loop(len, |builder, index| {
            let offset = builder.mul(index, word);
            let position = builder.add(data, offset);
            let mut element_current = builder.current_block();
            let _ = Self::decode_source_scalar(
                builder,
                element,
                position,
                &mut element_current,
                options.checked(),
            );
            builder.switch_to_block(element_current);
        });
        *current = builder.current_block();
    }

    fn decode_static_aggregate(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        head: ValueId,
        mut decode: impl FnMut(
            &mut FunctionBuilder<'_>,
            &crate::mir::AbiParamType,
            ValueId,
            &mut BlockId,
        ) -> ValueId,
    ) -> ValueId {
        match ty {
            crate::mir::AbiParamType::FixedArray { element, len } => {
                let (object, layout) =
                    builder.alloc_word_array(*len, crate::mir::AllocationSemantics::INTERNAL);
                let length = builder.imm(*len);
                let stride = builder
                    .imm(element.checked_head_size().expect("ABI head size exceeds u64 range"));
                builder.counted_loop(length, |builder, index| {
                    let offset = builder.mul(index, stride);
                    let field = builder.add(head, offset);
                    let mut current = builder.current_block();
                    let value = decode(builder, element, field, &mut current);
                    builder.switch_to_block(current);
                    let value = Self::encode_memory_scalar(builder, element, value);
                    builder.memory_object_store_element(object, layout, index, value);
                });
                object
            }
            crate::mir::AbiParamType::Tuple(fields) => {
                let (object, layout) = builder.alloc_word_struct(
                    fields.len() as u64,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                let mut offset = 0;
                let mut current = builder.current_block();
                for (index, field) in fields.iter().enumerate() {
                    let field_head = builder.add_u64_offset(head, offset);
                    let value = decode(builder, field, field_head, &mut current);
                    builder.switch_to_block(current);
                    let value = Self::encode_memory_scalar(builder, field, value);
                    builder.memory_object_store_field(object, layout, index as u64, value);
                    offset += field.checked_head_size().expect("ABI head size exceeds u64 range");
                }
                object
            }
            crate::mir::AbiParamType::Bytes | crate::mir::AbiParamType::DynamicArray(_) => {
                unreachable!("dynamic ABI value in static aggregate")
            }
            crate::mir::AbiParamType::Scalar(_) | crate::mir::AbiParamType::Enum { .. } => {
                unreachable!("scalar ABI word handled above")
            }
        }
    }

    fn decode_static_memory_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        head: ValueId,
        current: &mut BlockId,
        has_bitwise_shifting: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if let Some(validator) = ty.word_validator() {
            let value = builder.mload(head);
            let valid = validator.condition(builder, value, has_bitwise_shifting);
            *current = builder.revert_if_zero(valid, RevertReason::Empty);
            return Self::normalize_abi_word(builder, ty, value);
        }
        if matches!(ty, AbiParamType::Scalar(_)) {
            return builder.mload(head);
        }
        let value =
            Self::decode_static_aggregate(builder, ty, head, |builder, ty, head, current| {
                Self::decode_static_memory_argument(
                    builder,
                    ty,
                    head,
                    current,
                    has_bitwise_shifting,
                )
            });
        *current = builder.current_block();
        value
    }

    fn decode_calldata_bytes_slice_values(
        builder: &mut FunctionBuilder<'_>,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        head_checked: bool,
        offset_reason: RevertReason,
    ) -> (ValueId, ValueId, ValueId) {
        builder.switch_to_block(*current);
        if !head_checked {
            Self::guard_input_range(
                builder,
                head,
                32,
                input_end,
                current,
                RevertReason::TupleDataTooShort,
            );
        }
        let offset = builder.calldataload(head);
        let base = builder.add(tuple_base, offset);
        let word = builder.imm(32);
        let target_end = builder.add(base, word);
        let max_offset = builder.imm(u64::MAX);
        let offset_overflow = builder.gt(offset, max_offset);
        let head_out_of_range = builder.gt(target_end, input_end);
        let head_invalid = builder.or(offset_overflow, head_out_of_range);
        if builder.encodes_revert_reasons() {
            // Report solc's checks separately: an unencodable offset belongs to the enclosing
            // tuple, struct, or array, then the head range, length, and stride checks.
            *current = builder.revert_if(offset_overflow, offset_reason);
            *current =
                builder.revert_if(head_out_of_range, RevertReason::InvalidCalldataArrayOffset);
            let len = builder.calldataload(base);
            let data = builder.add(base, word);
            Self::guard_bytes_data(builder, data, len, input_end, current, false);
            return (base, data, len);
        }
        let len = builder.calldataload(base);
        let data = builder.add(base, word);
        let remaining = builder.sub(input_end, data);
        let tail_invalid = builder.gt(len, remaining);
        let invalid = builder.or(head_invalid, tail_invalid);
        *current = builder.revert_if(invalid, RevertReason::InvalidCalldataArrayOffset);
        (base, data, len)
    }

    fn decode_static_calldata_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        head: ValueId,
        current: &mut BlockId,
        has_bitwise_shifting: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let mut valid = builder.imm(1);
        let value =
            Self::decode_static_calldata_value(builder, ty, head, &mut valid, has_bitwise_shifting);
        let invalid = builder.iszero(valid);
        *current = builder.revert_if(invalid, RevertReason::Empty);
        value
    }

    fn decode_static_calldata_value(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        head: ValueId,
        valid: &mut ValueId,
        has_bitwise_shifting: bool,
    ) -> ValueId {
        if let Some(validator) = ty.word_validator() {
            let value = builder.calldataload(head);
            let condition = validator.condition(builder, value, has_bitwise_shifting);
            *valid = builder.and(*valid, condition);
            return Self::normalize_abi_word(builder, ty, value);
        }
        if matches!(ty, crate::mir::AbiParamType::Scalar(_)) {
            return builder.calldataload(head);
        }
        if let AbiParamType::FixedArray { element, len } = ty
            && Self::is_scalar_or_enum(element)
            && element.mir_type() != MirType::Function
        {
            let (object, layout) = builder.alloc_word_array(*len, AllocationSemantics::INTERNAL);
            if element.word_validator().is_some() {
                let length = builder.imm(*len);
                let stride = builder.imm(EvmMemoryLayout::WORD_SIZE);
                builder.counted_loop(length, |builder, index| {
                    let offset = builder.mul(index, stride);
                    let element_head = builder.add(head, offset);
                    let mut current = builder.current_block();
                    Self::decode_static_calldata_argument(
                        builder,
                        element,
                        element_head,
                        &mut current,
                        has_bitwise_shifting,
                    );
                    builder.switch_to_block(current);
                });
            }
            let byte_length = builder.imm(
                len.checked_mul(EvmMemoryLayout::WORD_SIZE).expect("checked static ABI array size"),
            );
            let source = builder.make_slice(head, byte_length, SliceLocation::Calldata);
            builder.memory_object_copy_from_slice(object, layout.kind(), source);
            return object;
        }
        Self::decode_static_aggregate(builder, ty, head, |builder, ty, head, current| {
            Self::decode_static_calldata_argument(builder, ty, head, current, has_bitwise_shifting)
        })
    }

    fn load_input_word(
        builder: &mut FunctionBuilder<'_>,
        position: ValueId,
        constructor: bool,
    ) -> ValueId {
        if constructor { builder.mload(position) } else { builder.calldataload(position) }
    }

    fn load_input_dynamic_array(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
        element_head_size: u64,
    ) -> (ValueId, ValueId) {
        let len = Self::load_input_word(builder, base, constructor);
        let data = builder.add_u64_offset(base, 32);
        Self::guard_input_dynamic_array(builder, len, data, input_end, current, element_head_size);
        (len, data)
    }

    fn guard_input_dynamic_array(
        builder: &mut FunctionBuilder<'_>,
        len: ValueId,
        data: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        element_head_size: u64,
    ) {
        builder.switch_to_block(*current);
        // Checking the quotient before multiplying proves both that the
        // multiplication cannot wrap and that the complete element head fits
        // in the input range.
        let remaining = builder.sub(input_end, data);
        let max_len = if element_head_size == 32 {
            let shift = builder.imm(5);
            builder.shr(shift, remaining)
        } else {
            let element_head_size = builder.imm(element_head_size);
            builder.div(remaining, element_head_size)
        };
        if builder.encodes_revert_reasons() {
            // solc reports an unencodable length before the stride check.
            let max_offset = builder.imm(u64::MAX);
            let too_long = builder.gt(len, max_offset);
            *current = builder.revert_if(too_long, RevertReason::InvalidCalldataArrayLength);
        }
        let invalid = builder.gt(len, max_len);
        *current = builder.revert_if(invalid, RevertReason::InvalidCalldataArrayStride);
    }

    fn decode_source_scalar(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        position: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) -> ValueId {
        builder.switch_to_block(*current);
        if !options.head_checked {
            Self::guard_input_range(
                builder,
                position,
                32,
                options.input_end,
                current,
                RevertReason::TupleDataTooShort,
            );
        }
        let value = Self::load_input_word(builder, position, options.constructor);
        if let Some(validator) = ty.word_validator() {
            Self::validate_abi_word(builder, value, validator, options.has_bitwise_shifting);
            *current = builder.current_block();
        }
        Self::normalize_abi_word(builder, ty, value)
    }

    /// Forms the absolute address of a dynamically encoded value from its head `offset` and
    /// checks that its first word lies inside the input.
    ///
    /// In debug mode the check is split like solc's: an unencodable offset reports
    /// `offset_reason`, and the target range check reports the message of the value's own
    /// decoder. `to_calldata` selects the message for a struct that stays in calldata.
    #[allow(clippy::too_many_arguments)]
    fn guard_input_offset(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        input_end: ValueId,
        ty: &crate::mir::AbiParamType,
        current: &mut BlockId,
        offset_reason: RevertReason,
        to_calldata: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let target = builder.add(base, offset);
        let is_array_like = matches!(
            ty,
            crate::mir::AbiParamType::DynamicArray(_) | crate::mir::AbiParamType::Bytes
        );
        if builder.encodes_revert_reasons() {
            // The same comparisons as below, split so each reports its own message:
            // if offset > 0xffffffffffffffff { revert(Error(offset_reason)) }
            // if offset > input_end - base { revert(Error("struct ... too short" | "stride")) }
            //   or, for arrays and bytes,
            // if target + 32 > input_end { revert(Error("invalid calldata array offset")) }
            let max_offset = builder.imm(u64::MAX);
            let overflow = builder.gt(offset, max_offset);
            *current = builder.revert_if(overflow, offset_reason);
            // solc decodes a fixed array with dynamic elements to memory through its array
            // decoder, which checks the first word before the element head sizes.
            let checks_first_word = is_array_like
                || (!to_calldata && matches!(ty, crate::mir::AbiParamType::FixedArray { .. }));
            let (invalid, reason) = if checks_first_word {
                let target_end = builder.add_u64_offset(target, 32);
                (builder.gt(target_end, input_end), RevertReason::InvalidCalldataArrayOffset)
            } else {
                let remaining = builder.sub(input_end, base);
                (builder.gt(offset, remaining), Self::aggregate_short_reason(ty, to_calldata))
            };
            *current = builder.revert_if(invalid, reason);
            return target;
        }
        if !is_array_like {
            let remaining = builder.sub(input_end, base);
            let invalid = builder.gt(offset, remaining);
            *current = builder.revert_if(invalid, RevertReason::InvalidTupleOffset);
            return target;
        }

        // Every dynamic ABI value starts with a word-sized head. Check that
        // word while forming the absolute address so nested offsets cannot
        // wrap or require a second range guard in the value-specific decoder.
        let target_end = builder.add_u64_offset(target, 32);
        let max_offset = builder.imm(u64::MAX);
        let overflow = builder.gt(offset, max_offset);
        let out_of_range = builder.gt(target_end, input_end);
        let invalid = builder.or(overflow, out_of_range);
        *current = builder.revert_if(invalid, RevertReason::InvalidCalldataArrayOffset);
        target
    }

    /// The debug message for a static aggregate whose head does not fit the input. Like solc,
    /// structs that stay in calldata report "struct calldata too short" and structs decoded to
    /// memory report "struct data too short"; fixed arrays reuse the array stride check.
    fn aggregate_short_reason(ty: &crate::mir::AbiParamType, to_calldata: bool) -> RevertReason {
        match ty {
            crate::mir::AbiParamType::Tuple(..) if to_calldata => {
                RevertReason::StructCalldataTooShort
            }
            crate::mir::AbiParamType::Tuple(..) => RevertReason::StructDataTooShort,
            crate::mir::AbiParamType::FixedArray { .. } => RevertReason::InvalidCalldataArrayStride,
            _ => RevertReason::TupleDataTooShort,
        }
    }

    /// Checks that `len` bytes at `data` lie inside the input.
    ///
    /// `bytes` copied to memory report solc's byte array message; `bytes calldata` keeps the
    /// calldata array checks, reporting an unencodable length separately in debug mode.
    fn guard_bytes_data(
        builder: &mut FunctionBuilder<'_>,
        data: ValueId,
        len: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        to_memory: bool,
    ) {
        if to_memory {
            let reason = RevertReason::InvalidByteArrayLength;
            Self::guard_input_range_value(builder, data, len, input_end, current, reason);
            return;
        }
        if builder.encodes_revert_reasons() {
            builder.switch_to_block(*current);
            let max_offset = builder.imm(u64::MAX);
            let too_long = builder.gt(len, max_offset);
            *current = builder.revert_if(too_long, RevertReason::InvalidCalldataArrayLength);
        }
        let reason = RevertReason::InvalidCalldataArrayStride;
        Self::guard_input_range_value(builder, data, len, input_end, current, reason);
    }

    fn guard_input_range(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: u64,
        input_end: ValueId,
        current: &mut BlockId,
        reason: RevertReason,
    ) {
        let size = builder.imm(size);
        Self::guard_input_range_value(builder, start, size, input_end, current, reason);
    }

    fn guard_input_range_value(
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        size: ValueId,
        input_end: ValueId,
        current: &mut BlockId,
        reason: RevertReason,
    ) {
        builder.switch_to_block(*current);
        // All callers establish `start <= input_end` before checking a tail
        // range, so compare against the remaining input instead of forming a
        // potentially overflowing end pointer.
        let remaining = builder.sub(input_end, start);
        let invalid = builder.gt(size, remaining);
        *current = builder.revert_if(invalid, reason);
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
        builder.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        *current = builder.current_block();
        result
    }

    fn checked_padded_size(
        builder: &mut FunctionBuilder<'_>,
        length: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let rounded = builder.add_u64_offset(length, 63);
        let overflow = builder.lt(rounded, length);
        builder.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        *current = builder.current_block();
        let mask = builder.imm(31);
        let mask = builder.not(mask);
        builder.and(rounded, mask)
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
        builder.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        *current = builder.current_block();
        result
    }

    fn can_use_calldata_slice(
        &self,
        func: &Function,
        uses: &[ValueId],
        ty: &crate::mir::AbiParamType,
        arg_type: MirType,
    ) -> bool {
        if !matches!(arg_type, MirType::MemoryObject(_)) || !Self::can_encode_calldata_slice(ty) {
            return false;
        }
        if func.instructions().any(|inst_id| match &func.inst(inst_id).kind {
            InstKind::MSize | InstKind::Fmp | InstKind::SetFmp(_) => true,
            InstKind::MLoad(address)
            | InstKind::MStore(address, _)
            | InstKind::MStore8(address, _)
                if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
            {
                true
            }
            _ => false,
        }) {
            return false;
        }
        if uses.is_empty()
            && matches!(
                ty,
                crate::mir::AbiParamType::DynamicArray(element)
                    if !Self::is_scalar_or_enum(element)
            )
        {
            return false;
        }

        let mut tainted = DenseBitSet::new_empty(func.num_values());
        for &value in uses {
            tainted.insert(value);
        }
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            if matches!(inst.kind, InstKind::ICall { .. }) {
                return false;
            }
            let tainted_operand = inst.operands().iter().any(|value| tainted.contains(*value));
            if matches!(
                &inst.kind,
                InstKind::MLoad(address)
                    if func.value_u64(*address).is_some() && !tainted_operand
            ) || matches!(&inst.kind, InstKind::Keccak256(offset, size)
                    if func.value_u64(*offset).is_some()
                        && func.value_u64(*size).is_some()
                        && !tainted_operand)
            {
                // A calldata alias skips the ABI copy into memory. Raw memory
                // reads would otherwise observe different contents there.
                return false;
            }
            if !tainted_operand {
                continue;
            }
            let (invalid, propagates) = match &inst.kind {
                InstKind::MemoryObjectData(object, _)
                | InstKind::MemoryObjectFieldAddr { object, .. }
                | InstKind::MemoryObjectElementAddr { object, .. }
                | InstKind::MemoryObjectLoadField { object, .. }
                | InstKind::SetMemoryObjectLen(object, ..)
                | InstKind::MemoryObjectStoreField { object, .. }
                | InstKind::MemoryObjectStoreElement { object, .. }
                | InstKind::MemoryObjectStoreByte { object, .. }
                | InstKind::MemoryObjectStoreWord { object, .. }
                | InstKind::MemoryObjectCopyFromSlice { object, .. }
                | InstKind::MemoryObjectCopyFromSliceAt { object, .. }
                | InstKind::MemoryObjectCopy { destination: object, .. }
                | InstKind::MStore(object, _)
                | InstKind::MStore8(object, _)
                | InstKind::MemoryZero(object, _)
                | InstKind::MCopy(object, _, _)
                | InstKind::CalldataCopy(object, _, _)
                | InstKind::CodeCopy(object, _, _)
                | InstKind::ExtCodeCopy(_, object, _, _)
                | InstKind::ReturnDataCopy(object, _, _)
                    if tainted.contains(*object) =>
                {
                    (true, false)
                }
                InstKind::MemoryObjectStoreField { value, .. }
                | InstKind::MemoryObjectStoreElement { value, .. }
                | InstKind::MemoryObjectStoreByte { value, .. }
                | InstKind::MemoryObjectStoreWord { value, .. }
                | InstKind::MStore(_, value)
                | InstKind::MStore8(_, value)
                    if tainted.contains(*value)
                        && (uses.contains(value)
                            || matches!(
                                func.value_ty(*value),
                                Some(MirType::MemoryObject(_) | MirType::Slice(_))
                            )) =>
                {
                    (true, false)
                }
                InstKind::MemoryObjectLen(object, _)
                | InstKind::MemoryObjectLoadByte { object, .. }
                | InstKind::MemoryObjectLoadElement { object, .. }
                    if tainted.contains(*object) =>
                {
                    (!Self::can_read_calldata_slice_use(&inst.kind, ty), false)
                }
                InstKind::AbiEncode { args, .. }
                    if args.iter().any(|&arg| tainted.contains(arg))
                        && Self::can_encode_calldata_slice(ty) =>
                {
                    (false, false)
                }
                InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::ExtCall { .. }
                | InstKind::ExtDelegateCall { .. }
                | InstKind::ExtStaticCall { .. }
                | InstKind::FrameStore { .. } => (true, false),
                InstKind::MappingSlotMemory(key, _)
                    if tainted.contains(*key) && Self::can_encode_calldata_slice(ty) =>
                {
                    (false, false)
                }
                InstKind::MappingSlotMemory(..) | InstKind::MappingSlotCalldata(..) => {
                    (true, false)
                }
                InstKind::ICall { function, args, .. }
                    if args.iter().enumerate().any(|(index, value)| {
                        tainted.contains(*value)
                            && !matches!(
                                self.function_params
                                    .get(*function)
                                    .and_then(|params| params.get(index)),
                                Some(MirType::Slice(SliceLocation::Calldata))
                            )
                    }) =>
                {
                    (true, false)
                }
                InstKind::ICall { .. } => (false, false),
                // A bytes memory value is commonly used as a raw pointer in inline assembly
                // (`add(data, 0x20)`). Do not replace that pointer with a calldata slice. Keep
                // the older propagation rule for other aggregate operations.
                InstKind::Add(..)
                | InstKind::Sub(..)
                | InstKind::MLoad(_)
                | InstKind::Keccak256(..)
                | InstKind::MemorySliceLoadWord { .. } => (true, false),
                InstKind::Phi(incoming) if Self::is_scalar_array(ty) => {
                    let Some(result) = func.inst_result_value(inst_id) else {
                        return false;
                    };
                    let compatible = incoming
                        .iter()
                        .all(|(_, value)| tainted.contains(*value) || *value == result);
                    (!compatible, compatible)
                }
                InstKind::Select(condition, then_value, else_value)
                    if Self::is_scalar_array(ty) =>
                {
                    let Some(result) = func.inst_result_value(inst_id) else {
                        return false;
                    };
                    let compatible = !tainted.contains(*condition)
                        && [then_value, else_value]
                            .into_iter()
                            .all(|value| tainted.contains(*value) || *value == result);
                    (!compatible, compatible)
                }
                InstKind::Phi(_) | InstKind::Select(..) => (false, true),
                _ => (false, true),
            };
            if invalid {
                return false;
            }
            if propagates
                && let Some(result) = func.inst_result_value(inst_id)
                && !(matches!(
                    ty,
                    crate::mir::AbiParamType::DynamicArray(element)
                        if Self::is_scalar_or_enum(element)
                ) && matches!(inst.kind, InstKind::Alloc { .. }))
            {
                tainted.insert(result);
            }
        }
        true
    }

    /// Retags loop-carried or selected aggregate values that carry the same
    /// calldata slice. ABI lowering replaces the original memory-object
    /// argument uses after MIR construction, so the original result type still
    /// needs to follow that replacement before memory-object lowering.
    fn retag_calldata_slice_values(func: &mut Function, root: ValueId) {
        let mut tainted = DenseBitSet::new_empty(func.num_values());
        tainted.insert(root);
        let inst_ids: Vec<_> = func.instructions().collect();
        loop {
            let mut changed = false;
            for &inst_id in &inst_ids {
                let Some(result) = func.inst_result_value(inst_id) else { continue };
                if tainted.contains(result) {
                    continue;
                }
                let compatible = match &func.inst(inst_id).kind {
                    InstKind::Phi(incoming) => incoming
                        .iter()
                        .all(|(_, value)| tainted.contains(*value) || *value == result),
                    InstKind::Select(condition, then_value, else_value) => {
                        !tainted.contains(*condition)
                            && [then_value, else_value]
                                .into_iter()
                                .all(|value| tainted.contains(*value) || *value == result)
                    }
                    _ => false,
                };
                if !compatible {
                    continue;
                }
                func.inst_mut(inst_id).result_ty = Some(MirType::Slice(SliceLocation::Calldata));
                tainted.insert(result);
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn can_read_calldata_slice_use(kind: &InstKind, ty: &crate::mir::AbiParamType) -> bool {
        match kind {
            InstKind::MemoryObjectLen(..) => {
                matches!(ty, crate::mir::AbiParamType::Bytes) || Self::is_scalar_array(ty)
            }
            InstKind::MemoryObjectLoadByte { .. } => {
                matches!(ty, crate::mir::AbiParamType::Bytes)
            }
            InstKind::MemoryObjectLoadElement { .. } => Self::is_scalar_array(ty),
            _ => false,
        }
    }

    fn is_scalar_array(ty: &crate::mir::AbiParamType) -> bool {
        matches!(
            ty,
            crate::mir::AbiParamType::DynamicArray(element)
                | crate::mir::AbiParamType::FixedArray { element, .. }
                if Self::is_scalar_or_enum(element)
        )
    }

    /// Returns whether a calldata scalar array must be validated in full.
    ///
    /// Calldata indexing validates the selected word at the point of access.
    /// A full pass is only needed when the array itself crosses another ABI or
    /// memory boundary, such as encoding or passing it to another function.
    fn needs_full_calldata_array_validation(
        &self,
        func: &Function,
        uses: &[ValueId],
        ty: &crate::mir::AbiParamType,
    ) -> bool {
        let mut tainted = DenseBitSet::new_empty(func.num_values());
        for &value in uses {
            tainted.insert(value);
        }

        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            if !inst.operands().iter().any(|value| tainted.contains(*value)) {
                continue;
            }

            let result = func.inst_result_value(inst_id);
            let action = match &inst.kind {
                InstKind::AbiEncode { args, .. }
                    if args.iter().any(|&arg| tainted.contains(arg))
                        && Self::can_encode_calldata_slice(ty)
                        && !Self::requires_calldata_element_validation(ty) =>
                {
                    false
                }
                InstKind::AbiEncode { .. }
                | InstKind::MemoryObjectCopyFromSlice { .. }
                | InstKind::MemoryObjectCopyFromSliceAt { .. }
                | InstKind::MemoryObjectCopy { .. }
                | InstKind::MemoryObjectData(..)
                | InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::ExtCall { .. }
                | InstKind::ExtDelegateCall { .. }
                | InstKind::ExtStaticCall { .. } => return true,
                InstKind::ICall { function, args, .. } => {
                    let callee_params = self.function_params.get(*function);
                    let preserves_calldata = args.iter().enumerate().all(|(index, value)| {
                        !tainted.contains(*value)
                            || callee_params.and_then(|params| params.get(index))
                                == Some(&MirType::Slice(SliceLocation::Calldata))
                    });
                    !preserves_calldata
                }
                InstKind::SliceLen(_)
                | InstKind::SlicePtr(_)
                | InstKind::CalldataSliceLoadWord { .. }
                | InstKind::MemorySliceLoadWord { .. }
                | InstKind::MemoryObjectLen(..)
                | InstKind::MemoryObjectLoadByte { .. } => false,
                InstKind::MemoryObjectLoadField { .. }
                | InstKind::MemoryObjectLoadElement { .. } => result.is_some_and(|value| {
                    func.value_ty(value).is_some_and(|ty| {
                        matches!(ty, MirType::MemoryObject(_) | MirType::Slice(_))
                    })
                }),
                InstKind::MemoryObjectFieldAddr { .. }
                | InstKind::MemoryObjectElementAddr { .. } => false,
                _ => result.is_some(),
            };
            if action {
                if let Some(result) = result {
                    tainted.insert(result);
                } else {
                    return true;
                }
            } else if result.is_none()
                && !matches!(
                    inst.kind,
                    InstKind::MemoryObjectFieldAddr { .. }
                        | InstKind::MemoryObjectElementAddr { .. }
                )
            {
                return true;
            }
        }

        for block in &func.blocks {
            let Some(terminator) = &block.terminator else { continue };
            if !terminator.operands().iter().any(|value| tainted.contains(*value)) {
                continue;
            }
            if matches!(
                terminator,
                Terminator::Return { .. }
                    | Terminator::ReturnData { .. }
                    | Terminator::TailCall { .. }
                    | Terminator::Revert { .. }
            ) {
                return true;
            }
        }
        false
    }

    fn can_encode_calldata_slice(ty: &crate::mir::AbiParamType) -> bool {
        matches!(ty, crate::mir::AbiParamType::Bytes) || Self::is_scalar_array(ty)
    }

    fn requires_calldata_element_validation(ty: &crate::mir::AbiParamType) -> bool {
        matches!(
            ty,
            crate::mir::AbiParamType::DynamicArray(element)
                | crate::mir::AbiParamType::FixedArray { element, .. }
                if Self::is_scalar_or_enum(element) && !Self::is_full_word_scalar(element)
        )
    }

    fn is_full_word_scalar(ty: &crate::mir::AbiParamType) -> bool {
        matches!(ty, crate::mir::AbiParamType::Scalar(scalar) if scalar.is_full_abi_word())
    }

    fn is_scalar_or_enum(ty: &crate::mir::AbiParamType) -> bool {
        ty.is_scalar_word() && ty.mir_type() != MirType::Function
    }

    fn normalize_abi_word(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        value: ValueId,
    ) -> ValueId {
        if matches!(ty, AbiParamType::Scalar(MirType::Function)) {
            let shift = builder.imm(64);
            builder.shr(shift, value)
        } else {
            value
        }
    }

    fn encode_memory_scalar(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        value: ValueId,
    ) -> ValueId {
        if matches!(ty, AbiParamType::Scalar(MirType::Function)) {
            let shift = builder.imm(64);
            return builder.shl(shift, value);
        }
        value
    }

    fn validate_abi_word(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        validator: AbiWordValidator,
        has_bitwise_shifting: bool,
    ) -> ValueId {
        let valid = validator.condition(builder, value, has_bitwise_shifting);
        builder.revert_if_zero(valid, RevertReason::Empty);
        value
    }

    /// Prepends `if callvalue() != 0 { revert(0, 0) }` to an external entry.
    ///
    /// The new guard block becomes the entry and falls through into the old
    /// body, so the check costs no extra jump. Function bodies have already
    /// been extracted, so internal callers never pay the check.
    fn inject_callvalue_check(&self, func: &mut Function) {
        let old_entry = BlockId::ENTRY;
        let mut builder = self.builder(func);
        let guard = builder.create_block();
        let revert = builder.create_block();
        builder.switch_to_block(guard);
        let value = builder.callvalue();
        builder.branch(value, revert, old_entry);
        builder.switch_to_block(revert);
        builder.revert_with(RevertReason::EtherSentToNonPayable);

        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
    }
}

/// Decodes a memory-backed ABI tuple through the shared ABI-layer decoder.
fn decode_memory_tuple(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    length: ValueId,
    layout: &AbiParamLayout,
    helpers: Option<&FxHashMap<crate::mir::AbiParamType, FunctionId>>,
    has_bitwise_shifting: bool,
) -> Option<Vec<ValueId>> {
    let head_size = layout.checked_head_size()?;
    let mut current = builder.current_block();
    let input_end =
        LowerAbiCx::validate_memory_tuple_input(builder, base, length, head_size, &mut current);

    let mut values = Vec::with_capacity(layout.types.len());
    let static_layout = layout.types.iter().all(|ty| !ty.is_dynamic());
    let mut head_offset = 0_u64;
    for ty in &layout.types {
        let head = builder.add_u64_offset(base, head_offset);
        let value = if static_layout {
            LowerAbiCx::decode_static_memory_argument(
                builder,
                ty,
                head,
                &mut current,
                has_bitwise_shifting,
            )
        } else {
            LowerAbiCx::decode_aggregate_argument(
                builder,
                ty,
                ty.mir_type(),
                head,
                base,
                &mut current,
                DecodeOptions {
                    allow_alias: false,
                    helpers,
                    ..DecodeOptions::new(true, input_end, has_bitwise_shifting).checked()
                },
            )
        };
        values.push(value);
        head_offset = head_offset
            .checked_add(ty.checked_head_size().expect("ABI head size exceeds u64 range"))?;
    }
    Some(values)
}

fn count_dynamic_tuple_types(
    ty: &AbiParamType,
    occurrences: usize,
    counts: &mut FxHashMap<AbiParamType, (usize, usize)>,
) {
    match ty {
        AbiParamType::FixedArray { element, len } => count_dynamic_tuple_types(
            element,
            occurrences.saturating_mul(usize::try_from(*len).unwrap_or(usize::MAX)).min(2),
            counts,
        ),
        AbiParamType::DynamicArray(element) => {
            count_dynamic_tuple_types(element, occurrences, counts)
        }
        AbiParamType::Tuple(fields) => {
            if ty.has_dynamic_child() {
                let first = counts.len();
                let count = counts.entry(ty.clone()).or_insert((0, first));
                count.0 = count.0.saturating_add(occurrences).min(2);
            }
            for field in fields {
                count_dynamic_tuple_types(field, occurrences, counts);
            }
        }
        AbiParamType::Scalar(_) | AbiParamType::Enum { .. } | AbiParamType::Bytes => {}
    }
}

fn abi_param_type_depth(ty: &AbiParamType) -> usize {
    match ty {
        AbiParamType::FixedArray { element, .. } | AbiParamType::DynamicArray(element) => {
            abi_param_type_depth(element) + 1
        }
        AbiParamType::Tuple(fields) => {
            fields.iter().map(abi_param_type_depth).max().unwrap_or_default() + 1
        }
        AbiParamType::Scalar(_) | AbiParamType::Enum { .. } | AbiParamType::Bytes => 0,
    }
}

/// Adds a helper for a repeated dynamic tuple in a memory ABI decode.
fn synthesize_memory_decode_helper(
    revert_strings: RevertStrings,
    module: &mut Module,
    ty: &AbiParamType,
    helpers: &FxHashMap<AbiParamType, FunctionId>,
    has_bitwise_shifting: bool,
) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::decode_memory_type));
    {
        let mut builder = FunctionBuilder::new(&mut function).with_revert_strings(revert_strings);
        let head = builder.add_param(MirType::uint256());
        let tuple_base = builder.add_param(MirType::uint256());
        let input_end = builder.add_param(MirType::uint256());
        let mut current = builder.current_block();
        let value = LowerAbiCx::decode_aggregate_argument(
            &mut builder,
            ty,
            ty.mir_type(),
            head,
            tuple_base,
            &mut current,
            DecodeOptions {
                allow_alias: false,
                helpers: (!helpers.is_empty()).then_some(helpers),
                ..DecodeOptions::new(true, input_end, has_bitwise_shifting).checked()
            },
        );
        builder.add_return(ty.mir_type());
        builder.ret([value]);
    }
    module.add_function(function)
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

fn find_canonical_return_calls(
    module: &Module,
    targets: &[FunctionId],
    cleanup_helpers: &FxHashMap<AbiParamType, FunctionId>,
) -> FxHashSet<(FunctionId, AbiParamType)> {
    let mut candidates = FxHashSet::default();
    for &id in targets {
        let func = module.function(id);
        let Some(layout) = &func.abi_return_params else { continue };
        for block in &func.blocks {
            let Some(Terminator::Return { values }) = &block.terminator else { continue };
            for (&value, ty) in values.iter().zip(&layout.types) {
                if cleanup_helpers.contains_key(ty)
                    && let Value::Inst(inst) = func.value(value)
                    && let InstKind::ICall { function, returns: 1, .. } = func.inst(*inst).kind
                    && func.value_ty(value) == Some(ty.mir_type())
                {
                    candidates.insert((function, ty.clone()));
                }
            }
        }
    }

    candidates.retain(|(id, ty)| {
        is_canonical_return_function(
            &mut CanonicalCallProof { module: Some(module), ..Default::default() },
            *id,
            ty,
        )
    });
    candidates
}

fn canonical_input_for_arg<'a>(
    func: &Function,
    value: ValueId,
    input_params: Option<&'a AbiParamLayout>,
) -> Option<&'a AbiParamType> {
    let crate::mir::Value::Arg(index) = func.value(value) else { return None };
    let mut physical = 0;
    for ty in input_params?.types.iter() {
        if ty.is_scalar_word() && physical == index.index() {
            return Some(ty);
        }
        physical += (ty.checked_head_size().expect("ABI head size exceeds u64 range")
            / EvmMemoryLayout::WORD_SIZE) as usize;
    }
    None
}

fn canonical_input_covers_return(input: &AbiParamType, output: &AbiParamType) -> bool {
    match (input, output) {
        (
            AbiParamType::Scalar(input) | AbiParamType::Enum { ty: input, .. },
            AbiParamType::Scalar(output),
        ) => canonical_scalar_covers_return(*input, *output),
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
        _ => input == output,
    }
}

fn reuses_validated_input(
    func: &Function,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    output: &AbiParamType,
) -> bool {
    canonical_input_for_arg(func, value, input_params)
        .is_some_and(|input| canonical_input_covers_return(input, output))
}

fn is_canonical_return_value(
    func: &Function,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
) -> bool {
    if !ty.needs_return_cleanup()
        || (source == ReturnValueSource::Scalar
            && reuses_validated_input(func, value, input_params, ty))
    {
        return true;
    }

    let mut visiting = FxHashSet::default();
    let mut calls = CanonicalCallProof::default();
    is_canonical_return_value_inner(
        func,
        ty,
        value,
        input_params,
        source,
        &mut visiting,
        &mut calls,
    )
}

fn is_canonical_return_value_inner(
    func: &Function,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
    visiting: &mut FxHashSet<ValueId>,
    calls: &mut CanonicalCallProof<'_>,
) -> bool {
    if !ty.needs_return_cleanup()
        || (source == ReturnValueSource::Scalar
            && reuses_validated_input(func, value, input_params, ty))
    {
        return true;
    }
    if calls.module.is_some()
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::ICall { function, returns: 1, .. } = func.inst(*inst).kind
        && func.value_ty(value) == Some(ty.mir_type())
        && (ty.is_scalar_word() || is_unmodified_call_result(func, value, *inst))
    {
        return is_canonical_return_function(calls, function, ty);
    }
    if !visiting.insert(value) {
        return false;
    }

    let result = match ty {
        AbiParamType::Scalar(ty) => is_canonical_return_scalar(func, *ty, value, source),
        AbiParamType::Enum { .. } => false,
        AbiParamType::Bytes => true,
        AbiParamType::Tuple(fields) => {
            is_canonical_return_object(func, fields, value, input_params, visiting, calls)
        }
        AbiParamType::FixedArray { element, len } => {
            is_canonical_return_array(func, element, *len, value, input_params, visiting, calls)
        }
        AbiParamType::DynamicArray(element) => {
            is_canonical_return_dynamic_array(func, element, value, input_params, visiting, calls)
        }
    };
    visiting.remove(&value);
    result
}

fn is_canonical_return_function(
    calls: &mut CanonicalCallProof<'_>,
    id: FunctionId,
    ty: &AbiParamType,
) -> bool {
    let key = (id, ty.clone());
    if !calls.visiting.insert(key.clone()) {
        return false;
    }
    let module = calls.module.expect("recursive canonical proof has a module");
    let func = module.function(id);
    let result = func.returns.as_slice() == [ty.mir_type()]
        && func
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
        && func.blocks.iter().all(|block| {
            let Some(Terminator::Return { values }) = &block.terminator else { return true };
            let mut value_visiting = FxHashSet::default();
            values.len() == 1
                && is_canonical_return_value_inner(
                    func,
                    ty,
                    values[0],
                    None,
                    ReturnValueSource::Memory,
                    &mut value_visiting,
                    calls,
                )
        });
    calls.visiting.remove(&key);
    result
}

fn is_unmodified_call_result(func: &Function, value: ValueId, call: InstId) -> bool {
    for inst in func.instructions() {
        if inst == call {
            continue;
        }
        match func.inst(inst).kind {
            InstKind::MemoryObjectLen(object, _)
            | InstKind::MemoryObjectLoadByte { object, .. }
                if object == value => {}
            InstKind::MemoryObjectLoadField { object, .. }
            | InstKind::MemoryObjectLoadElement { object, .. }
                if object == value
                    && func
                        .inst_result_value(inst)
                        .and_then(|result| func.value_ty(result))
                        .is_some_and(|ty| !matches!(ty, MirType::MemoryObject(_))) => {}
            InstKind::MemoryObjectStoreField { value: stored, .. }
            | InstKind::MemoryObjectStoreElement { value: stored, .. }
                if stored == value => {}
            _ if func.inst(inst).operands().contains(&value) => return false,
            _ => {}
        }
    }
    func.blocks.iter().all(|block| {
        block.terminator.as_ref().is_none_or(|term| {
            !term.operands().contains(&value)
                || matches!(term, Terminator::Return { values } if values.contains(&value))
        })
    })
}

fn is_canonical_return_call(
    func: &Function,
    block: BlockId,
    ty: &AbiParamType,
    value: ValueId,
    canonical_calls: &FxHashSet<(FunctionId, AbiParamType)>,
) -> bool {
    let Value::Inst(inst) = func.value(value) else { return false };
    let InstKind::ICall { function, returns: 1, .. } = func.inst(*inst).kind else {
        return false;
    };
    func.blocks[block].instructions.last() == Some(inst)
        && func.blocks[block].terminator.is_none()
        && canonical_calls.contains(&(function, ty.clone()))
}

fn is_canonical_return_scalar(
    func: &Function,
    ty: MirType,
    value: ValueId,
    source: ReturnValueSource,
) -> bool {
    let Some(expected) = return_cleanup_mask(ty, source) else {
        if let MirType::Int(size) = ty
            && size.bits() < 256
            && let Value::Inst(inst) = func.value(value)
            && let InstKind::SignExtend(byte, input) = func.inst(*inst).kind
        {
            return func.value_u64(byte) == Some(u64::from(size.bits() / 8 - 1))
                && is_canonical_return_scalar(func, ty, input, source);
        }
        return matches!(ty, MirType::Int(size) if size.bits() >= 256);
    };
    if ty == MirType::Function
        && source == ReturnValueSource::Memory
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::Shl(shift, _) = func.inst(*inst).kind
        && func.value_u64(shift) == Some(64)
    {
        return true;
    }
    if let MirType::FixedBytes(size) = ty
        && size.bytes() < 32
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::Shl(shift, source) = func.inst(*inst).kind
        && func.value_u64(shift) == Some((32 - u64::from(size.bytes())) * 8)
        && is_canonical_low_bits(func, source, u64::from(size.bytes()) * 8)
    {
        return true;
    }
    if source == ReturnValueSource::Scalar
        && let Value::Inst(inst) = func.value(value)
        && matches!(func.inst(*inst).kind, InstKind::LoadImmutable(_))
    {
        return true;
    }
    let Value::Inst(inst) = func.value(value) else {
        return func.value_u256(value).is_some_and(|value| value & !expected == U256::ZERO);
    };
    let InstKind::And(lhs, rhs) = func.inst(*inst).kind else { return false };
    let (source, mask) = if let Some(mask) = func.value_u256(rhs) {
        (lhs, mask)
    } else if let Some(mask) = func.value_u256(lhs) {
        (rhs, mask)
    } else {
        return false;
    };
    mask == expected && source != value
}

fn is_canonical_low_bits(func: &Function, value: ValueId, bits: u64) -> bool {
    let mask = U256::MAX >> (256 - usize::try_from(bits).expect("bit width fits usize"));
    if func.value_u256(value).is_some_and(|value| value & !mask == U256::ZERO) {
        return true;
    }
    let Value::Inst(inst) = func.value(value) else { return false };
    let InstKind::And(lhs, rhs) = func.inst(*inst).kind else { return false };
    func.value_u256(lhs).is_some_and(|value| value == mask)
        || func.value_u256(rhs).is_some_and(|value| value == mask)
}

fn return_cleanup_mask(ty: MirType, source: ReturnValueSource) -> Option<U256> {
    let validator = if source == ReturnValueSource::Memory {
        AbiWordValidator::from_mir_type(ty)
    } else {
        AbiWordValidator::from_return_mir_type(ty)
    };
    validator.and_then(AbiWordValidator::canonical_mask)
}

fn is_canonical_return_object(
    func: &Function,
    fields: &[AbiParamType],
    object: ValueId,
    input_params: Option<&AbiParamLayout>,
    visiting: &mut FxHashSet<ValueId>,
    calls: &mut CanonicalCallProof<'_>,
) -> bool {
    let Value::Inst(alloc) = func.value(object) else { return false };
    let InstKind::Alloc { kind: AllocationKind::Object(_), .. } = &func.inst(*alloc).kind else {
        return false;
    };

    let mut stores = FxHashMap::default();
    let mut zeroed = false;
    for inst in func.instructions() {
        if inst == *alloc {
            continue;
        }
        match &func.inst(inst).kind {
            InstKind::MemoryZero(base, _) if *base == object => zeroed = true,
            InstKind::MemoryObjectStoreField { object: base, field, value, .. }
                if *base == object =>
            {
                if stores.insert(*field, *value).is_some() {
                    return false;
                }
            }
            InstKind::MemoryObjectLoadField { object: base, .. } if *base == object => {}
            InstKind::MemoryObjectStoreField { value: stored, .. } if *stored == object => {}
            InstKind::MemoryObjectStoreElement { value: stored, .. } if *stored == object => {}
            _ if func.inst(inst).operands().contains(&object) => return false,
            _ => {}
        }
    }
    if func.blocks.iter().any(|block| {
        block.terminator.as_ref().is_some_and(|terminator| {
            terminator.operands().contains(&object)
                && !matches!(terminator, Terminator::Return { values } if values.contains(&object))
        })
    }) {
        return false;
    }
    fields.iter().enumerate().all(|(index, ty)| {
        let canonical = stores.get(&(index as u64)).is_some_and(|&value| {
            is_canonical_return_value_inner(
                func,
                ty,
                value,
                input_params,
                ReturnValueSource::Memory,
                visiting,
                calls,
            )
        });
        let zero = zeroed && (ty.is_scalar_word() || !ty.needs_return_cleanup());
        canonical || zero
    })
}

fn is_canonical_return_array(
    func: &Function,
    element: &AbiParamType,
    len: u64,
    object: ValueId,
    input_params: Option<&AbiParamLayout>,
    visiting: &mut FxHashSet<ValueId>,
    calls: &mut CanonicalCallProof<'_>,
) -> bool {
    let Value::Inst(alloc) = func.value(object) else { return false };
    if !matches!(func.inst(*alloc).kind, InstKind::Alloc { kind: AllocationKind::Object(_), .. }) {
        return false;
    }
    let mut stores = FxHashMap::default();
    for inst in func.instructions() {
        if let InstKind::MemoryObjectStoreElement { object: base, index, value, .. } =
            func.inst(inst).kind
            && base == object
        {
            let Some(index) = func.value_u64(index) else { return false };
            if stores.insert(index, value).is_some() {
                return false;
            }
        } else if let InstKind::MemoryObjectStoreElement { value: stored, .. } =
            func.inst(inst).kind
            && stored == object
        {
        } else if inst != *alloc && func.inst(inst).operands().contains(&object) {
            return false;
        }
    }
    (0..len).all(|index| {
        stores.get(&index).is_some_and(|&value| {
            is_canonical_return_value_inner(
                func,
                element,
                value,
                input_params,
                ReturnValueSource::Memory,
                visiting,
                calls,
            )
        })
    })
}

/// Proves canonicality for arrays built by a counted loop with no escaping reads.
fn is_canonical_return_dynamic_array(
    func: &Function,
    element: &AbiParamType,
    object: ValueId,
    input_params: Option<&AbiParamLayout>,
    visiting: &mut FxHashSet<ValueId>,
    calls: &mut CanonicalCallProof<'_>,
) -> bool {
    let Value::Inst(alloc) = func.value(object) else { return false };
    let InstKind::Alloc { kind: AllocationKind::Object(layout), .. } = &func.inst(*alloc).kind
    else {
        return false;
    };
    if layout.kind() != MemoryObjectKind::DynamicArray {
        return false;
    }

    let mut length = None;
    let mut store = None;
    let inst_blocks = func.inst_blocks();
    for inst in func.instructions() {
        match &func.inst(inst).kind {
            InstKind::SetMemoryObjectLen(base, value, kind)
                if *base == object && *kind == MemoryObjectKind::DynamicArray =>
            {
                if length.replace(*value).is_some() {
                    return false;
                }
            }
            InstKind::MemoryObjectStoreElement { object: base, index, value, .. }
                if *base == object =>
            {
                if store.replace((inst, *index, *value)).is_some() {
                    return false;
                }
            }
            _ if inst != *alloc && func.inst(inst).operands().contains(&object) => return false,
            _ => {}
        }
    }
    let (Some(length), Some((store_inst, index, value))) = (length, store) else {
        return false;
    };
    if !is_canonical_return_value_inner(
        func,
        element,
        value,
        input_params,
        ReturnValueSource::Memory,
        visiting,
        calls,
    ) {
        return false;
    }

    let Some(&store_block) = inst_blocks.get(&store_inst) else {
        return false;
    };
    let Value::Inst(phi_inst) = func.value(index) else {
        return false;
    };
    let Some(&phi_block) = inst_blocks.get(phi_inst) else {
        return false;
    };
    let InstKind::Phi(incoming) = &func.inst(*phi_inst).kind else {
        return false;
    };
    if incoming.len() != 2 {
        return false;
    }

    let Some(&(preheader, _zero)) =
        incoming.iter().find(|(_, value)| func.value_u64(*value) == Some(0))
    else {
        return false;
    };
    let Some(&(backedge, _)) = incoming.iter().find(|(_, value)| {
        let Value::Inst(inst) = func.value(*value) else { return false };
        matches!(func.inst(*inst).kind, InstKind::Add(lhs, rhs) if
            (lhs == index && func.value_u64(rhs) == Some(1))
                || (rhs == index && func.value_u64(lhs) == Some(1)))
    }) else {
        return false;
    };
    if preheader == backedge
        || !func.blocks[backedge]
            .terminator
            .as_ref()
            .is_some_and(|term| matches!(term, Terminator::Jump(target) if *target == phi_block))
    {
        return false;
    }

    let Some(Terminator::Branch { condition, then_block, .. }) = &func.blocks[phi_block].terminator
    else {
        return false;
    };
    let Value::Inst(condition_inst) = func.value(*condition) else { return false };
    if !matches!(func.inst(*condition_inst).kind, InstKind::Lt(lhs, rhs) if lhs == index && rhs == length)
    {
        return false;
    }

    let mut work = vec![(*then_block, false)];
    let mut seen = FxHashSet::default();
    while let Some((block, stored)) = work.pop() {
        if !seen.insert((block, stored)) {
            continue;
        }
        let stored = stored || block == store_block;
        if block == phi_block {
            if !stored {
                return false;
            }
            continue;
        }
        let Some(terminator) = &func.blocks[block].terminator else {
            return false;
        };
        if matches!(terminator, Terminator::Return { .. } | Terminator::ReturnData { .. }) {
            return false;
        }
        for successor in terminator.successors() {
            work.push((successor, stored));
        }
    }
    true
}

fn canonicalize_return_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
) -> ValueId {
    let nullable_memory = matches!(
        builder.func().value(value),
        Value::Inst(inst)
            if matches!(
                builder.func().inst(*inst).kind,
                InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
            )
    );
    canonicalize_return_value_inner(builder, ty, value, input_params, source, nullable_memory)
}

fn canonicalize_return_value_inner(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
    nullable_memory: bool,
) -> ValueId {
    if !ty.needs_return_cleanup()
        || is_canonical_return_value(builder.func(), ty, value, input_params, source)
    {
        return value;
    }

    if let AbiParamType::Enum { variants, .. } = ty {
        builder.validate_enum_value(*variants, value);
    }
    match ty {
        AbiParamType::Scalar(ty) | AbiParamType::Enum { ty, .. } => {
            let validator = if source == ReturnValueSource::Memory {
                AbiWordValidator::from_mir_type(*ty)
            } else {
                AbiWordValidator::from_return_mir_type(*ty)
            };
            validator.map_or(value, |validator| validator.cleanup(builder, value))
        }
        AbiParamType::Bytes => value,
        AbiParamType::Tuple(fields) => {
            let (output, layout) =
                builder.alloc_word_struct(fields.len() as u64, AllocationSemantics::INTERNAL);
            let non_null = nullable_memory.then(|| memory_object_non_null(builder, value));
            for (index, field_ty) in fields.iter().enumerate() {
                let mut field_value = builder.memory_object_load_field(value, layout, index as u64);
                if let Some(non_null) = non_null {
                    field_value = builder.mul(field_value, non_null);
                }
                let field_value = canonicalize_return_value_inner(
                    builder,
                    field_ty,
                    field_value,
                    input_params,
                    ReturnValueSource::Memory,
                    true,
                );
                builder.memory_object_store_field(output, layout, index as u64, field_value);
            }
            output
        }
        AbiParamType::FixedArray { element, len } => {
            let (output, layout) = builder.alloc_word_array(*len, AllocationSemantics::INTERNAL);
            let length = builder.imm(*len);
            let non_null = nullable_memory.then(|| memory_object_non_null(builder, value));
            builder.counted_loop(length, |builder, index| {
                let mut element_value = builder.memory_object_load_element(value, layout, index);
                if let Some(non_null) = non_null {
                    element_value = builder.mul(element_value, non_null);
                }
                let element_value = canonicalize_return_value_inner(
                    builder,
                    element,
                    element_value,
                    input_params,
                    ReturnValueSource::Memory,
                    true,
                );
                builder.memory_object_store_element(output, layout, index, element_value);
            });
            output
        }
        AbiParamType::DynamicArray(element) => {
            let layout = MemoryObjectLayout::WORD_ARRAY;
            let mut length = builder.memory_object_len(value, layout.kind());
            let non_null = nullable_memory.then(|| memory_object_non_null(builder, value));
            if let Some(non_null) = non_null {
                length = builder.mul(length, non_null);
            }
            let one = builder.imm(1);
            let mut current = builder.current_block();
            let words = LowerAbiCx::checked_add(builder, length, one, &mut current);
            let word = builder.imm(EvmMemoryLayout::WORD_SIZE);
            let size = LowerAbiCx::checked_mul(builder, words, word, &mut current);
            let output = builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
            builder.set_memory_object_len(output, length, layout.kind());

            builder.counted_loop(length, |builder, index| {
                let mut element_value = builder.memory_object_load_element(value, layout, index);
                if let Some(non_null) = non_null {
                    element_value = builder.mul(element_value, non_null);
                }
                let element_value = canonicalize_return_value_inner(
                    builder,
                    element,
                    element_value,
                    input_params,
                    ReturnValueSource::Memory,
                    true,
                );
                builder.memory_object_store_element(output, layout, index, element_value);
            });
            output
        }
    }
}

fn memory_object_non_null(builder: &mut FunctionBuilder<'_>, object: ValueId) -> ValueId {
    let non_null = builder.iszero(object);
    builder.iszero(non_null)
}

fn direct_calldata_copy_source(
    func: &Function,
    return_block: BlockId,
    object: ValueId,
) -> Option<ValueId> {
    if func.value_ty(object) != Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) {
        return None;
    }
    let crate::mir::Value::Inst(defining_inst) = func.value(object) else { return None };
    if !matches!(func.inst(*defining_inst).kind, InstKind::Alloc { .. }) {
        return None;
    }

    let mut source = None;
    let mut length_set = false;
    for (block_id, block) in func.blocks.iter_enumerated() {
        for &inst in &block.instructions {
            let instruction = func.inst(inst);
            if !instruction.operands().contains(&object) {
                continue;
            }
            match instruction.kind {
                InstKind::SetMemoryObjectLen(object_id, _, object_kind)
                    if object_id == object && object_kind == MemoryObjectKind::Bytes =>
                {
                    length_set = true
                }
                InstKind::MemoryObjectCopyFromSlice {
                    object: object_id,
                    kind: object_kind,
                    source: slice,
                } if object_id == object
                    && object_kind == MemoryObjectKind::Bytes
                    && func.value_ty(slice) == Some(MirType::Slice(SliceLocation::Calldata))
                    && source.replace(slice).is_none()
                    && block_id == return_block => {}
                _ => return None,
            }
        }
        if let Some(terminator) = &block.terminator
            && terminator.operands().contains(&object)
            && (block_id != return_block
                || !matches!(terminator, Terminator::Return { values } if values.contains(&object)))
        {
            return None;
        }
    }
    if !length_set {
        return None;
    }
    source
}

fn reuse_direct_calldata_returns(
    func: &mut Function,
    return_block: BlockId,
    values: &[ValueId],
    layout: &AbiLayout,
) -> (FxHashMap<ValueId, ValueId>, FxHashSet<usize>) {
    let mut replacements = FxHashMap::default();
    let mut calldata_indices = FxHashSet::default();
    for (index, &value) in values.iter().enumerate() {
        let Some(abi_type) = layout.types.get(index) else { continue };
        if !matches!(abi_type, AbiType::Bytes(SliceLocation::Memory)) {
            continue;
        }
        let Some(source) = direct_calldata_copy_source(func, return_block, value) else {
            continue;
        };
        replacements.insert(value, source);
        calldata_indices.insert(index);
    }
    if replacements.is_empty() {
        return (replacements, calldata_indices);
    }

    let copy_insts = func
        .instructions()
        .filter(|&inst| {
            matches!(func.inst(inst).kind, InstKind::MemoryObjectCopyFromSlice { object, .. } if replacements.contains_key(&object))
        })
        .collect::<Vec<_>>();
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !copy_insts.contains(inst));
    }
    (replacements, calldata_indices)
}

fn static_bytes_return(func: &Function) -> Option<StaticBytesReturn> {
    if !func.params.is_empty()
        || func.returns.as_slice() != [MirType::MemoryObject(MemoryObjectKind::Bytes)]
        || func.abi_params.as_ref().is_none_or(|layout| !layout.types.is_empty())
        || func
            .abi_returns
            .as_ref()
            .is_none_or(|layout| layout.types.as_ref() != [AbiType::Bytes(SliceLocation::Memory)])
        || func.blocks.len() != 1
    {
        return None;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [alloc, set_len, store_word] = block.instructions.as_slice() else { return None };
    let Some(Terminator::Return { values }) = &block.terminator else { return None };
    let [object] = values.as_slice() else { return None };
    if !matches!(func.value(*object), Value::Inst(inst) if inst == alloc) {
        return None;
    }
    let InstKind::Alloc { size, kind, semantics } = func.inst(*alloc).kind else { return None };
    if kind != AllocationKind::Object(MemoryObjectLayout::Bytes)
        || semantics != AllocationSemantics::INTERNAL
        || func.value_u64(size) != Some(64)
    {
        return None;
    }
    let InstKind::SetMemoryObjectLen(len_object, len, MemoryObjectKind::Bytes) =
        func.inst(*set_len).kind
    else {
        return None;
    };
    let InstKind::MemoryObjectStoreWord { object: word_object, offset, value } =
        func.inst(*store_word).kind
    else {
        return None;
    };
    let len = func.value_u64(len)?;
    if len_object != *object
        || word_object != *object
        || func.value_u64(offset) != Some(0)
        || !(1..=32).contains(&len)
    {
        return None;
    }
    let word = func.value_u256(value)?;
    let trailing_bits = usize::try_from((32 - len) * 8).ok()?;
    Some(StaticBytesReturn { len, word: word >> trailing_bits << trailing_bits })
}

fn encode_static_bytes_return(
    func: &mut Function,
    return_block: BlockId,
    value: StaticBytesReturn,
) {
    let word_size = EvmMemoryLayout::WORD_SIZE;
    let return_size = word_size * 3;
    func.blocks[return_block].instructions.clear();
    func.blocks[return_block].terminator = None;
    func.external_static_return_size = return_size;
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(return_block);
    builder.inherit_terminator_debug_context(return_block);
    // mstore(128, 32)
    // mstore(160, len)
    // mstore(192, word)
    // returndata(128, 96)
    let offset = builder.imm(EvmMemoryLayout::HEAP_START);
    let data_offset = builder.imm(word_size);
    let len_offset = builder.imm(EvmMemoryLayout::HEAP_START + word_size);
    let len = builder.imm(value.len);
    let word_offset = builder.imm(EvmMemoryLayout::HEAP_START + word_size * 2);
    let word = builder.imm(value.word);
    let size = builder.imm(return_size);
    builder.mstore(offset, data_offset);
    builder.mstore(len_offset, len);
    builder.mstore(word_offset, word);
    builder.ret_data(offset, size);
}

fn materialize_calldata_return(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    base: ValueId,
    has_bitwise_shifting: bool,
) -> ValueId {
    let input_end = builder.calldatasize();
    let mut current = builder.current_block();
    let reason = LowerAbiCx::aggregate_short_reason(ty, true);
    LowerAbiCx::guard_input_range(
        builder,
        base,
        ty.data_head_size(),
        input_end,
        &mut current,
        reason,
    );
    let options = DecodeOptions::new(false, input_end, has_bitwise_shifting).checked();
    LowerAbiCx::decode_static_aggregate(builder, ty, base, |builder, ty, head, current| {
        LowerAbiCx::decode_aggregate_argument(
            builder,
            ty,
            ty.mir_type(),
            head,
            base,
            current,
            options,
        )
    })
}
