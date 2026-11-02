//! ABI lowering: materialize calldata decoding and returndata encoding as MIR.
//!
//! Semantic MIR initially gives external functions typed arguments and return values.
//! This conversion pass makes their ABI handling explicit before dispatch lowering.
//! The module stays semantic until the final conversion checks the lowered representation.
//!
//! For each external entry `f(x0: T0, .., xn: Tn)`, it:
//!
//! 1. copies the original into a fresh internal function `f.body` with its parameter list preserved
//!    when there are internal callers; in gas mode, a function with three or more dynamic
//!    parameters also keeps a copy of the body inline in its wrapper, and
//! 2. strips `f`'s MIR parameter list, keeping its selector and its `Value::Arg` entries. Scalar
//!    arguments remain lazy ABI head words (up to `MAX_LAZY_ARG_WORDS`), calldata aggregates become
//!    `slice<calldata>` values over their validated encoding until `lower-slices` projects their
//!    pointer and length, and memory aggregates decode into fresh memory objects. Value-carrying
//!    returns are ABI-encoded according to the function's return layout and terminate with
//!    `returndata`.
//!
//! The wrapper keeps argument materialization lazy so values used after a
//! branch can still be rematerialized instead of spilled. Dynamic return
//! encoding becomes a semantic `abi_encode_scratch` operation here and lowers later;
//! static returns use the fixed low-memory return buffer directly. A returned memory
//! aggregate is encoded as it is when its return layout gives every narrow word a
//! validator: the encoder then cleans each word it reads from memory, so no cleaned copy
//! precedes it. Internal
//! call sites that targeted a wrapped function are retargeted to its extracted
//! raw-return body, so internal calls to public functions keep their convention.
//!
//! Calldata aggregates that repeat across parameters, or dynamic tuples that repeat anywhere
//! inside them, decode through shared helpers; tuple helpers take the base of their enclosing
//! tuple so enclosing decoders can call them for nested values.
//! Scalar arrays whose canonicality requires zero bits fold their words with OR and validate
//! once after the loop. Their checked source range supplies a non-wrapping end for a word
//! cursor. An empty array skips the cursor setup and validation; signed and enum validators
//! still check each element separately.
//!
//! Each `abi_decode` reads a bytes object or a memory slice in place. A slice's pointer is the
//! base and its length is checked against the head size, so a constant-length slice, such as an
//! external call's static return buffer, needs no length word. Repeated decodes of a layout from
//! the same kind of input share a helper; one that static slices share takes only the base, and
//! each caller checks its own length, which then usually folds away. A calldata slice decodes in
//! place as well, with the calldata decoders the entries use: offsets resolve against the slice's
//! base and every read stays below its end, as solc decodes it, so no copy of the encoding into
//! memory precedes the decode.
//! A decoded `bytes` object clears its padding bytes after the copy, so they are zero whatever
//! the input and free memory held; the clear stays inside the object.
//!
//! A module the pass cannot lower fails a preflight scan before any rewriting, and the pass
//! reports an error: a return layout that does not match the returned values, constructor
//! parameters that do not match their layout, a bytes fallback that returns anything but one
//! memory bytes object, or an `abi_decode` whose layout head size overflows.
//! Argument-free functions that only return a short literal use direct fixed-buffer stores;
//! both typed word stores and their data-pointer/store form qualify. A single `abi_encode`
//! of up to two constant words uses the same path while the encoding is still opaque. Other
//! instructions, allocation policies, and literals longer than two words keep the general encoder.
//!
//! The `fallback(bytes calldata) returns (bytes memory)` form is a separate
//! raw-data boundary: it gets an argument-free dispatch wrapper and an
//! internal body that terminates with unencoded returndata.
//!
//! A constructor with ABI parameters decodes them inline at the head of its body from its
//! constructor arguments, which are in memory: offsets resolve against their base and every read
//! stays below their end, as in `abi_decode` of a memory object, but without shared helpers.
//!
//! Callvalue checks follow `lower-dispatch`. When every external entry rejects Ether and the
//! module is not a library, `lower-dispatch` checks `callvalue` once in the selector switch. Both
//! passes read the same predicate, `DispatchCallvalue`. Otherwise each rejecting
//! wrapper starts with its own check, a guard block that becomes the entry and falls through into
//! the body, so a selector case pays no extra jump. A rejecting constructor always carries its
//! own check, since no dispatch switch precedes it. Library wrappers never check, because a
//! `DELEGATECALL` sees the caller's value; `lower-dispatch` guards their non-view entries against
//! direct calls instead.
//!
//! A `revert_returndata` terminator, which forwards a failed external call's revert data,
//! becomes `returndatacopy` of the whole returndata followed by a revert of its size. Before
//! Byzantium, which lacks `returndatasize`, it becomes an empty revert.
//!
//! The module-wide `call_cleanup` runs once at the end, after every wrapper exists. It treats
//! the argument words a wrapper validated, recorded while its prologue was built, as already
//! cleaned to the bits their validator checks, and infers the other arguments and returns from
//! call sites and bodies as usual.
//!
//! A returned value skips its cleanup when it is proved canonical: an unmodified validated input
//! word, a scalar whose bits the body proves, or a memory aggregate whose words are proved
//! recursively. An aggregate also needs a function that writes memory only into its own
//! allocations, at constant offsets: `memory_writes_stay_private` rejects writes through unknown
//! addresses, writes at a runtime index, which need a separate extent proof, and calls with
//! hidden effects. In gas mode the proof also follows a call that is the block's last instruction
//! into callees that return canonical values. An enum is canonical only as a validated input
//! word, a call cycle fails the proof, and a failed proof only keeps the cleanup.
//!
//! Together with [`super::lower_dispatch::LowerDispatch`], which routes a selector switch
//! to these argument-free wrappers, this materializes the ABI boundary before
//! EVM codegen. Both passes must complete before the backend runs.

use crate::mir::{
    AbiEncodeMode, AbiLayout, AbiParamLayout, AbiParamLayoutRef, AbiParamLocation, AbiParamType,
    AbiType, AbiWordValidator, AllocationSemantics, ArgIdx, BlockId, CalldataTail, Callee,
    EffectKind, FrameMode, FrameSlotKind, Function, FunctionBuilder, FunctionId, InstId, InstKind,
    MangledSymbol, MemoryObjectLayout, MirPhase, MirType, Module, PtrLocation, RevertReason,
    Terminator, Value, ValueId, ValueLayout,
    analysis::{AliasAnalysis, HotnessAnalysis, LocationSize, MemoryBase},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
    transform::{cfg_simplify::remove_unreachable_blocks, utils::expand_block},
    typing::memory_object,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode, RevertStrings};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Ident, Span, Symbol, sym};
use std::sync::Arc;

/// Head words that scalar parameters address as lazy arguments.
///
/// Lazy arguments index a dense per-word table, so a scalar behind a huge static array loads
/// its word explicitly instead of growing the table with the array.
const MAX_LAZY_ARG_WORDS: u64 = 1 << 16;

/// Materializes explicit ABI entries and decoding.
pub(crate) struct LowerAbi;

impl MirPass for LowerAbi {
    fn name(&self) -> &'static str {
        "lower-abi"
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
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        let gas = module.runs_objective(OptimizationMode::is_gas);
        LowerAbiCx { revert_strings: gcx.sess.opts.revert_strings, ..Default::default() }
            .run(module, gcx.sess.opts.evm_version, gas)
            .unwrap_or_else(|| {
                analyses.fail(gcx.dcx().err("`lower-abi` cannot lower this ABI shape").emit());
                false
            })
    }
}

fn mark_abi_wrappers(module: &mut Module) -> bool {
    let mut changed = false;
    for func in &mut module.functions {
        if func.is_external_entry() && !func.attributes.is_abi_wrapper {
            // ret -> stop at the external ABI boundary
            for block in &mut func.blocks {
                if !func.attributes.is_constructor
                    && matches!(&block.terminator, Some(Terminator::Return { values }) if values.is_empty())
                {
                    block.terminator = Some(Terminator::Stop);
                }
            }
            func.attributes.is_abi_wrapper = true;
            changed = true;
        }
    }
    changed
}

#[derive(Debug, Default)]
struct LowerAbiCx {
    aggregate_type_helpers: FxHashMap<AbiParamType, FunctionId>,
    calldata_slice_helper: Option<FunctionId>,
    function_params: IndexVec<FunctionId, Vec<MirType>>,
    validated_abi_arguments: super::call_cleanup::ArgumentBits,
    /// How compiler-generated decoding reverts are encoded.
    revert_strings: RevertStrings,
}

#[derive(Clone, Copy)]
struct DecodeOptions<'a> {
    constructor: bool,
    input_end: ValueId,
    /// Whether `input_end` is at most `u64::MAX` above the input's base: calldata and constructor
    /// arguments are, while `abi.decode` takes its length from a memory word or a slice that
    /// inline assembly can set up to `2**255 - 1`.
    bounded_input: bool,
    head_checked: bool,
    validate_array_elements: bool,
    helpers: Option<&'a FxHashMap<AbiParamType, FunctionId>>,
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
    tail: Option<U256>,
}

impl DecodeOptions<'_> {
    fn new(constructor: bool, input_end: ValueId) -> Self {
        Self {
            constructor,
            input_end,
            bounded_input: false,
            head_checked: false,
            validate_array_elements: true,
            helpers: None,
            offset_reason: RevertReason::InvalidTupleOffset,
        }
    }

    fn checked(self) -> Self {
        Self { head_checked: true, ..self }
    }

    fn bounded(self) -> Self {
        Self { bounded_input: true, ..self }
    }

    /// Decodes a value nested in an aggregate whose offsets report `offset_reason`.
    fn nested(self, offset_reason: RevertReason) -> Self {
        Self { offset_reason, ..self }
    }
}

impl LowerAbiCx {
    /// Lowers the module's ABI boundary, returning whether it changed, or `None` when the module
    /// has an ABI shape this pass cannot lower.
    fn run(
        &mut self,
        module: &mut Module,
        evm_version: EvmVersion,
        gas_mode: bool,
    ) -> Option<bool> {
        let mut targets = Vec::new();
        let mut constructors = Vec::new();
        let mut bytes_fallback = None;
        let mut decode_functions = DenseBitSet::new_empty(module.functions.len());
        let mut has_revert_returndata = false;
        let mut rejecting_constructor = None;
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) && !func.attributes.is_abi_wrapper {
                targets.push(id);
                if !can_encode_live_returns(module, func) {
                    return None;
                }
            }
            if func.attributes.is_constructor && !func.attributes.is_abi_wrapper {
                if super::utils::rejects_callvalue(func) {
                    rejecting_constructor = Some(id);
                }
                if func.abi_params.is_some() {
                    if Self::can_wrap_constructor_params(func) {
                        constructors.push(id);
                    } else {
                        return None;
                    }
                }
            }
            if func.attributes.is_fallback
                && !func.attributes.is_abi_wrapper
                && is_bytes_fallback(func)
            {
                if !can_lower_bytes_fallback_returns(func) {
                    return None;
                }
                bytes_fallback = Some(id);
            }
            for inst_id in func.instructions() {
                match &func.inst(inst_id).kind {
                    InstKind::AbiDecode { layout, .. } => {
                        layout.checked_head_size()?;
                        decode_functions.insert(id);
                    }
                    &InstKind::ICall { function: Callee::Function(function), .. } => {
                        internally_called.insert(function);
                    }
                    _ => {}
                }
            }
            has_revert_returndata |= func
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(Terminator::RevertReturndata)));
        }

        if targets.is_empty()
            && constructors.is_empty()
            && decode_functions.is_empty()
            && !has_revert_returndata
            && bytes_fallback.is_none()
            && rejecting_constructor.is_none()
        {
            return Some(mark_abi_wrappers(module));
        }
        self.function_params =
            module.functions.iter().map(|func| func.params.iter().copied().collect()).collect();

        self.synthesize_shared_calldata_slice_helpers(module, &targets);
        self.synthesize_shared_aggregate_type_helpers(module, &targets);
        let canonical_return_calls = if gas_mode {
            find_canonical_return_calls(module, &targets)
        } else {
            FxHashSet::default()
        };

        self.lower_decode_instructions(module, &decode_functions);

        if has_revert_returndata {
            self.lower_revert_returndata(module, evm_version);
        }

        // A bytes fallback receives the complete calldata blob and returns raw
        // bytes, rather than an ABI tuple. Keep that boundary explicit: the
        // external fallback becomes an argument-free dispatcher target, while
        // its extracted body terminates with the raw returndata operation.
        if let Some(id) = bytes_fallback {
            Self::wrap_bytes_fallback(module, id);
        }

        let hoist_callvalue = !module.is_library && callvalue.hoists();

        for id in constructors {
            let layout = module.function_mut(id).abi_params.take();
            self.inject_abi_prologue(module.function_mut(id), layout.as_ref(), true);
            Self::clear_abi_inputs(module.function_mut(id));
        }
        if let Some(id) = rejecting_constructor {
            self.inject_callvalue_check(module.function_mut(id));
        }

        let mut body_of_wrapper = FxHashMap::default();
        let no_canonical_return_calls = FxHashSet::default();
        for id in targets {
            let gas_mode = module.objective(id).is_gas();
            // Most external functions are never called internally. Only those
            // that are need a second, parameterized body; cloning every wrapper
            // needlessly grows the MIR consumed by all subsequent lowering and
            // backend passes.
            if let Some(body_id) = self.wrap_function(
                module,
                id,
                internally_called.contains(id),
                gas_mode,
                if gas_mode { &canonical_return_calls } else { &no_canonical_return_calls },
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
                    if let InstKind::ICall { function: Callee::Function(function), .. } =
                        &mut inst.kind
                        && let Some(&body_id) = body_of_wrapper.get(function)
                    {
                        *function = body_id;
                    }
                });
            }
        }

        mark_abi_wrappers(module);
        let argument_bits =
            |func: &Function, id, index, facts: &super::call_cleanup::ArgumentBits| {
                self.validated_abi_arguments
                    .get(&(id, index))
                    .copied()
                    .unwrap_or_else(|| super::call_cleanup::argument_bits(func, id, index, facts))
            };
        let facts = super::call_cleanup::infer_arguments_with(module, argument_bits);
        // The wrappers above changed which functions are cold.
        HotnessAnalysis::new().run_functions(module);
        let returns = super::call_cleanup::infer_returns(module);
        for (id, func) in module.functions.iter_mut_enumerated() {
            super::call_cleanup::cleanup(
                func,
                |func, index| argument_bits(func, id, index, &facts),
                &returns,
            );
        }
        Some(true)
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
            && layout.types.iter().zip(&func.params).all(|(abi_ty, &param_ty)| {
                abi_ty.mir_type() == param_ty
                    || (abi_ty.is_scalar_word() && param_ty == MirType::I256)
            })
    }

    /// Rewrites value-carrying returns into a semantic ABI encode followed by
    /// `returndata (extract_value encoded, 0), (extract_value encoded, 1)`.
    fn encode_live_returns(
        &self,
        func: &mut Function,
        return_params: Option<&AbiParamLayout>,
        input_params: Option<&AbiParamLayout>,
        canonical_calls: &FxHashSet<(FunctionId, AbiParamType)>,
        static_bytes_return: Option<StaticBytesReturn>,
        decoded_params: &[Option<ValueId>],
    ) {
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
        // Several return blocks share one layout, so only a single one is rewritten.
        let mut calldata_returns = FxHashMap::default();
        let mut canonical_returns = FxHashSet::default();
        if let [return_block] = return_blocks.as_slice() {
            let values = match &func.blocks[*return_block].terminator {
                Some(Terminator::Return { values }) => values.to_vec(),
                _ => unreachable!("return block collected above"),
            };
            let (replacements, calldata_indices) =
                reuse_direct_calldata_returns(func, *return_block, &values, &layout);
            calldata_returns = replacements;
            if !calldata_indices.is_empty() {
                let layout = Arc::make_mut(&mut layout);
                for index in calldata_indices {
                    *layout.types.get_mut(index).expect("ABI return shape exists") =
                        AbiType::Bytes(PtrLocation::Calldata);
                }
            }
            // An array already holding ABI-validated words needs neither a cleaned copy
            // nor the encoder's per-element copy: the array this wrapper decoded when element
            // cleanup proved that nothing widens it, or one a call returned with that proof.
            // Strip the element cleanup from the layout so the encoder copies the payload once.
            for (index, &value) in values.iter().enumerate() {
                let Some(ty) = return_params.and_then(|params| params.types.get(index)) else {
                    continue;
                };
                if is_canonical_decoded_array(func, ty, value, decoded_params)
                    || returns_proved_call(func, *return_block, ty, value, canonical_calls)
                    || is_canonical_return_value(
                        func,
                        ty,
                        value,
                        input_params,
                        ReturnValueSource::Scalar,
                    )
                {
                    canonical_returns.insert(index);
                    if let Some(ty) = Arc::make_mut(&mut layout).types.get_mut(index) {
                        strip_array_element_cleanup(ty);
                    }
                }
            }
        }
        let in_scratch = returns_in_scratch(&layout);
        if !layout.types.iter().any(AbiType::is_dynamic) && !in_scratch {
            // Static return data outside scratch memory occupies the low-memory ABI
            // buffer. Keep the backend spill area above it so a cross-block value cannot be
            // overwritten while the return tuple is encoded.
            func.external_static_return_size = layout.head_size();
        }
        let return_types = func.return_components().to_vec();
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
                        .or_else(|| {
                            return_types
                                .get(index)
                                .copied()
                                .map(|ty| AbiParamType::Scalar(ty.value_layout()))
                        })
                    else {
                        return value;
                    };
                    if return_types.get(index) == Some(&MirType::Slice(PtrLocation::Calldata))
                        && match &ty {
                            AbiParamType::Tuple(_) | AbiParamType::FixedArray { .. } => true,
                            // Returns copy arrays of composite elements to memory and
                            // canonicalize them there, with the ABI decoder's revert messages.
                            AbiParamType::DynamicArray(element) => {
                                !element.is_scalar_word()
                                    && !matches!(element.as_ref(), AbiParamType::Bytes)
                            }
                            _ => false,
                        }
                    {
                        return materialize_calldata_return(&mut builder, &ty, value);
                    }
                    // The encoder cleans each word it reads from a memory aggregate, so a
                    // cleaned copy would only clean every word twice.
                    if canonical_returns.contains(&index)
                        || (!ty.is_scalar_word()
                            && builder.func().value_ty(value) == Some(MirType::MEMPTR)
                            && layout
                                .types
                                .get(index)
                                .is_some_and(|abi| encoder_cleans_return(abi, &ty)))
                    {
                        return value;
                    }
                    canonicalize_return_value(
                        &mut builder,
                        &ty,
                        value,
                        input_params,
                        ReturnValueSource::Scalar,
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            if layout.types.iter().any(AbiType::is_dynamic) {
                // The call ends with the encoding, so nothing can observe the
                // memory it would reserve: stage it at the free-memory pointer.
                // encoded = abi_encode_scratch(values)
                let encoded = builder.abi_encode_scratch(layout.clone(), None, values);
                let offset = builder.slice_ptr(encoded);
                let size = builder.slice_len(encoded);
                builder.ret_data(offset, size);
            } else {
                // A result that fits below the heap start never reaches the spill area.
                let start = if in_scratch {
                    EvmMemoryLayout::SCRATCH_SLOT
                } else {
                    EvmMemoryLayout::HEAP_START
                };
                let offset = builder.imm(start);
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

    /// Lowers every `abi_decode` in `decode_functions`, whose layouts have valid head sizes.
    fn lower_decode_instructions(
        &self,
        module: &mut Module,
        decode_functions: &DenseBitSet<FunctionId>,
    ) {
        let mut decode_counts = FxHashMap::default();
        let mut memory_type_counts = FxHashMap::default();
        for func_id in decode_functions.iter() {
            let func = module.function(func_id);
            for inst_id in func.instructions() {
                let InstKind::AbiDecode { data, layout } = &func.inst(inst_id).kind else {
                    continue;
                };
                // A calldata slice decodes in place, without the shared memory decoders.
                if func.value_slice_location(*data) == Some(PtrLocation::Calldata) {
                    continue;
                }
                let data_ty = memory_object(func, *data);
                let count = decode_counts.entry((layout.clone(), data_ty)).or_insert(0);
                if *count == 0 {
                    for ty in &layout.types {
                        count_dynamic_tuple_types(ty, 1, &mut memory_type_counts);
                    }
                }
                *count += 1;
            }
        }

        let mut memory_type_helpers = FxHashMap::default();
        for ty in repeated_types(memory_type_counts) {
            let helper = synthesize_memory_decode_helper(
                self.revert_strings,
                module,
                &ty,
                &memory_type_helpers,
            );
            memory_type_helpers.insert(ty, helper);
        }

        let mut decode_helpers = FxHashMap::default();
        for ((layout, data_ty), count) in decode_counts {
            if count >= 2 && layout.types.len() == 1 && !layout.types[0].is_dynamic() {
                let helper = self.synthesize_decode_helper(
                    module,
                    layout.clone(),
                    data_ty,
                    sym::decode_static,
                    &memory_type_helpers,
                );
                decode_helpers.insert((layout, data_ty), helper);
            } else if count >= 2 && layout.types.iter().any(AbiParamType::is_dynamic) {
                let helper = self.synthesize_decode_helper(
                    module,
                    layout.clone(),
                    data_ty,
                    sym::decode_aggregate,
                    &memory_type_helpers,
                );
                decode_helpers.insert((layout, data_ty), helper);
            }
        }

        let mut replacements = FxHashMap::default();
        for func_id in decode_functions.iter() {
            let mut builder = self.builder(module.function_mut(func_id));
            let blocks = builder.func().blocks.indices();
            for block in blocks {
                expand_block(&mut builder, block, |builder, inst| {
                    let InstKind::AbiDecode { data, layout } = &builder.func().inst(inst).kind
                    else {
                        return false;
                    };
                    let layout = layout.clone();
                    let data = crate::mir::utils::resolve_replacement(*data, &replacements);
                    let result = builder.func().inst_result_value(inst);
                    let calldata =
                        builder.func().value_slice_location(data) == Some(PtrLocation::Calldata);
                    let values = if calldata {
                        let (base, length) = decode_input(builder, data, true);
                        decode_tuple(builder, base, length, &layout, true, None)
                    } else {
                        let key = (layout, memory_object(builder.func(), data));
                        let (layout, data_ty) = &key;
                        if let Some(&helper) = decode_helpers.get(&key) {
                            let arg = if passes_slice_base(*data_ty, layout) {
                                // if slt (extract_value data, 1), head_size { revert }
                                // arg = extract_value data, 0
                                let (base, length) = decode_input(builder, data, false);
                                let head_size =
                                    layout.checked_head_size().expect("checked ABI layout");
                                let head_size = builder.imm(head_size);
                                let short = builder.slt(length, head_size);
                                builder.revert_if(short, RevertReason::TupleDataTooShort);
                                base
                            } else {
                                data
                            };
                            // result = icall decode_helper(arg)
                            let result = result.expect("decode helpers have outputs");
                            let result_ty =
                                builder.func().value_ty(result).expect("typed ABI decode");
                            let value = builder.icall(helper, vec![arg], result_ty);
                            replacements.insert(result, value);
                            return true;
                        }
                        let (base, length) = decode_input(builder, data, false);
                        decode_tuple(
                            builder,
                            base,
                            length,
                            layout,
                            false,
                            Some(&memory_type_helpers),
                        )
                    };
                    if let Some(result) = result {
                        // result = insert_value(undef, field0), ...
                        let value = match builder.func().value_ty(result) {
                            Some(MirType::Struct(id)) => builder.make_struct(id, values),
                            _ => values[0],
                        };
                        replacements.insert(result, value);
                    }
                    true
                });
            }
            builder.func_mut().replace_uses_canonicalized(&replacements);
            replacements.clear();
        }
    }

    fn synthesize_shared_aggregate_type_helpers(
        &mut self,
        module: &mut Module,
        targets: &[FunctionId],
    ) {
        // Parameters share a helper per type, and so do the dynamic tuples repeated
        // anywhere inside them, which the helpers of enclosing types then call.
        // Debug revert strings give nested sites their own offset reasons, which only inline
        // decoding reports, so only parameters count there.
        let nested = !self.revert_strings.is_debug();
        let mut counts = FxHashMap::<AbiParamType, (usize, usize)>::default();
        for &id in targets {
            let func = module.function(id);
            let Some(layout) = func.abi_params.as_ref() else { continue };
            for (ty, &arg_type) in layout.types.iter().zip(&func.params) {
                if !ty.is_scalar_word() && arg_type == MirType::MEMPTR {
                    // `count_dynamic_tuple_types` counts a dynamic tuple itself.
                    if !nested || !decodes_nested(ty) {
                        count_type(&mut counts, ty, 1);
                    }
                    if nested {
                        count_dynamic_tuple_types(ty, 1, &mut counts);
                    }
                }
            }
        }
        for ty in repeated_types(counts) {
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
            // fn @decode_calldata_type(head, tuple_base) -> T { ret decode T at head }
            // fn @decode_calldata_type(head) -> T { ret decode T at head, tuple_base = 4 }
            let head = builder.add_param(MirType::I256);
            let tuple_base =
                if decodes_nested(&ty) { builder.add_param(MirType::I256) } else { builder.imm(4) };
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let value = Self::decode_aggregate_argument(
                &mut builder,
                &ty,
                ty.mir_type(),
                head,
                tuple_base,
                &mut current,
                DecodeOptions {
                    helpers: Some(&self.aggregate_type_helpers),
                    ..DecodeOptions::new(false, input_end).checked().bounded()
                },
            );
            builder.set_return_type(ty.mir_type());
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
                func.abi_params.as_ref().map_or(0, |layout| {
                    layout
                        .types
                        .iter()
                        .zip(&func.params)
                        .filter(|(ty, arg_type)| {
                            matches!(ty, AbiParamType::Bytes)
                                && matches!(arg_type, MirType::Slice(PtrLocation::Calldata))
                        })
                        .count()
                })
            })
            .sum::<usize>();
        if count >= 2 {
            self.calldata_slice_helper = Some(self.synthesize_calldata_slice_helper(module));
        }
    }

    fn synthesize_calldata_slice_helper(&self, module: &mut Module) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(sym::decode_calldata_slice));
        {
            let mut builder = self.builder(&mut function);
            let head = builder.add_param(MirType::I256);
            let tuple_base = builder.imm(4);
            let input_end = builder.calldatasize();
            let mut current = builder.current_block();
            let (base, _, _) = Self::decode_calldata_bytes_slice_values(
                &mut builder,
                head,
                tuple_base,
                input_end,
                true,
                &mut current,
                true,
                RevertReason::InvalidTupleOffset,
            );
            builder.set_return_type(MirType::I256);
            builder.ret([base]);
        }
        module.add_function(function)
    }

    fn synthesize_decode_helper(
        &self,
        module: &mut Module,
        layout: AbiParamLayoutRef,
        data_ty: MirType,
        name: Symbol,
        memory_type_helpers: &FxHashMap<AbiParamType, FunctionId>,
    ) -> FunctionId {
        let mut function = Function::new(Ident::with_dummy_span(name));
        {
            let mut builder = self.builder(&mut function);
            let (base, length) = if passes_slice_base(data_ty, &layout) {
                // The callers check the slice length, so the input is the head alone.
                let base = builder.add_param(MirType::MEMPTR);
                let head_size = layout.checked_head_size().expect("checked ABI layout");
                (base, builder.imm(head_size))
            } else {
                let data = builder.add_param(data_ty);
                decode_input(&mut builder, data, false)
            };
            let fields = layout.types.iter().map(AbiParamType::mir_type).collect();
            builder.set_return_type(module.intern_return_type(fields).expect("decode has outputs"));
            let values = decode_tuple(
                &mut builder,
                base,
                length,
                layout.as_ref(),
                false,
                Some(memory_type_helpers),
            );
            // ret insert_value(undef, field0), ...
            builder.ret(values);
        }
        module.add_function(function)
    }

    fn validate_memory_tuple_input(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        length: ValueId,
        head_size: u64,
        current: &mut BlockId,
    ) -> ValueId {
        // input_end = base + length
        // if slt(length, head_size) { revert }
        builder.switch_to_block(*current);
        let input_end = builder.add(base, length);
        let head_size = builder.imm(head_size);
        let short = builder.slt(length, head_size);
        *current = builder.revert_if(short, RevertReason::TupleDataTooShort);
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
        // An internally called function keeps one body, which its wrapper
        // calls. In gas mode, a function with three or more dynamic
        // parameters also keeps a copy inside its wrapper, which saves moving
        // each decoded slice through the call protocol at the price of the
        // duplicate body.
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

        let logical_values =
            self.inject_abi_prologue(module.function_mut(wrapper_id), abi_params.as_ref(), false);
        self.validated_abi_arguments.extend(
            abi_params
                .as_ref()
                .into_iter()
                .flat_map(|layout| layout.types.iter().zip(&logical_values))
                .filter_map(|(ty, value)| {
                    let Value::Arg(index) = module.function(wrapper_id).value((*value)?) else {
                        return None;
                    };
                    let bits = match ty.word_validator()? {
                        AbiWordValidator::Unsigned(bits) => u32::from(bits),
                        AbiWordValidator::Bool => 1,
                        _ => return None,
                    };
                    Some(((wrapper_id, *index), bits))
                }),
        );
        // A wrapper that keeps its body in place returns values computed from these
        // decoded parameters; one that calls the body returns the call's results instead.
        let decoded_params = if call_body { Vec::new() } else { logical_values.clone() };
        if call_body && logical_values.iter().all(Option::is_some) {
            Self::replace_body_with_call(
                module.function_mut(wrapper_id),
                body_id.expect("body clone for a calling wrapper"),
                original_entry_inst.expect("calling wrapper has an entry instruction"),
                original.return_components(),
                logical_values.into_iter().map(Option::unwrap).collect(),
            );
        }
        if let [MirType::Struct(id)] = original.return_components() {
            let fields = module.struct_types[*id].fields.clone();
            let wrapper = module.function_mut(wrapper_id);
            // wrapper -> struct [return_abi = fields]
            wrapper.set_return_abi(fields.clone());
            let blocks = wrapper.blocks.indices();
            for block in blocks {
                let Some(Terminator::Return { values }) = &wrapper.blocks[block].terminator else {
                    continue;
                };
                let [aggregate] = values.as_slice() else { continue };
                let aggregate = *aggregate;
                let mut builder = FunctionBuilder::new(wrapper);
                builder.switch_to_block(block);
                // field0 = extract_value result, 0
                // ...
                // ret field0, ...
                let values = fields
                    .iter()
                    .enumerate()
                    .map(|(index, &field)| {
                        let mut current = aggregate;
                        let mut remaining = builder.func().num_insts();
                        while remaining != 0
                            && let Value::Inst(inst) = builder.func().value(current)
                            && let InstKind::InsertValue {
                                ty,
                                aggregate: previous,
                                index: inserted_index,
                                value,
                            } = builder.func().inst(*inst).kind
                            && ty == MirType::Struct(*id)
                        {
                            if inserted_index == index as u32 {
                                return value;
                            }
                            remaining -= 1;
                            current = previous;
                        }
                        builder.extract_value(*id, aggregate, index as u32, field)
                    })
                    .collect();
                builder.set_terminator(Terminator::Return { values });
            }
        }
        let return_params = module.function_mut(wrapper_id).abi_return_params.take();
        self.encode_live_returns(
            module.function_mut(wrapper_id),
            return_params.as_ref(),
            abi_params.as_ref(),
            canonical_return_calls,
            static_bytes_return,
            &decoded_params,
        );

        // External wrappers take no MIR arguments; constructor parameters
        // retain their physical ABI head words for deployment codegen.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.params.clear();
        wrapper.set_return_type(MirType::Void);
        Self::clear_abi_metadata(wrapper);
        body_id
    }

    fn can_call_body(func: &Function, abi_params: Option<&AbiParamLayout>) -> bool {
        let Some(layout) = abi_params else { return false };
        layout.types.len() == func.params.len()
            && layout.types.iter().zip(&func.params).all(|(abi_ty, &param_ty)| {
                (abi_ty.is_scalar_word()
                    && !matches!(abi_ty, AbiParamType::Scalar(ValueLayout::Function))
                    && param_ty != MirType::MEMPTR)
                    || (!abi_ty.is_scalar_word()
                        && matches!(param_ty, MirType::MEMPTR | MirType::Slice(_)))
            })
            && !func.return_components()
                .iter()
                .any(|&ty| matches!(ty, MirType::Slice(PtrLocation::Returndata)))
            && (func.return_components().len() <= 1
                || func.return_components().iter().all(|&ty| !matches!(ty, MirType::Slice(_))))
            && func.blocks.iter().all(|block| {
                !matches!(&block.terminator, Some(Terminator::Return { values }) if values.len() != func.return_components().len())
            })
    }

    fn internal_body(original: &Function) -> Function {
        let mut body = original.clone();
        body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
        body.name_span = Span::DUMMY;
        body.selector = None;
        body.attributes.is_abi_wrapper = false;
        Self::clear_abi_metadata(&mut body);
        body.attributes.visibility = solar_sema::hir::Visibility::Internal;
        Self::discharge_abi_validations(&mut body);
        body
    }

    fn discharge_abi_validations(func: &mut Function) {
        // validate_abi value -> validated entry decoding or a typed internal argument
        for block in func.blocks.indices() {
            let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
            instructions.retain(|&id| !matches!(func.inst(id).kind, InstKind::ValidateAbi(_)));
            func.blocks[block].instructions = instructions;
        }
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
        original_entry_inst: InstId,
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
        // body(args) -> icall body_id, args; return results
        func.blocks[block].instructions.clear();
        crate::mir::utils::replace_terminator(func, block, Terminator::Invalid);
        func.blocks[block].terminator = None;
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        if let Some((&first, rest)) = return_types.split_first() {
            let mut values = Vec::with_capacity(return_types.len());
            values.push(builder.icall(body_id, args, first));
            if !rest.is_empty() {
                let base = builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word);
                for (index, &ty) in (1_u64..).zip(rest) {
                    let value = match ty {
                        MirType::MEMPTR => {
                            builder.memory_load_field_as(base, index, MirType::MEMPTR)
                        }
                        _ => {
                            let position =
                                builder.add_u64_offset(base, index * EvmMemoryLayout::WORD_SIZE);
                            builder.load(position, PtrLocation::Memory)
                        }
                    };
                    values.push(value);
                }
            }
            builder.ret(values);
        } else {
            builder.icall_void(body_id, args);
            builder.ret([]);
        }
        // Drop the detached body blocks before lowering their now-orphaned slice projections.
        let _ = remove_unreachable_blocks(builder.func_mut());
    }

    /// Rewrites `fallback(bytes calldata) returns (bytes memory)` into an
    /// argument-free wrapper and an internal body that returns the raw bytes.
    ///
    /// Fallback input is the complete calldata, including any selector-like
    /// prefix. Its return value is not ABI encoded: the body therefore uses a
    /// `return_data` terminator, and the wrapper's resultless call is reshaped
    /// into a tail call after slice lowering.
    fn wrap_bytes_fallback(module: &mut Module, fallback_id: FunctionId) {
        let original = module.function(fallback_id).clone();
        let mut body = Self::internal_body(&original);
        body.attributes.is_fallback = false;
        body.attributes.is_receive = false;
        Self::lower_bytes_fallback_returns(&mut body);
        body.set_return_type(MirType::Void);
        let body_id = module.add_function(body);

        let mut wrapper = Function::new(Ident::with_dummy_span(original.name.symbol));
        wrapper.name = original.name;
        wrapper.name_span = original.name_span;
        wrapper.attributes = original.attributes;
        {
            let mut builder = FunctionBuilder::new(&mut wrapper);
            let zero = builder.imm(0);
            let length = builder.calldatasize();
            let input = builder.make_slice(zero, length, PtrLocation::Calldata);
            builder.icall_void(body_id, vec![input]);
            builder.invalid();
        }
        *module.function_mut(fallback_id) = wrapper;
    }

    /// Rewrites every value-carrying fallback return, which `can_lower_bytes_fallback_returns`
    /// admitted, into raw returndata.
    fn lower_bytes_fallback_returns(func: &mut Function) {
        let blocks = func.blocks.indices();
        for block in blocks {
            let Some(Terminator::Return { values }) = &func.blocks[block].terminator else {
                continue;
            };
            let value = values[0];
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block);
            builder.inherit_terminator_debug_context(block);
            let offset = builder.memory_data(value);
            let size = builder.memory_len(value);
            builder.ret_data(offset, size);
        }
    }

    /// Materializes deferred ABI arguments and their validation checks.
    fn inject_abi_prologue(
        &self,
        func: &mut Function,
        abi_params: Option<&AbiParamLayout>,
        constructor: bool,
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
        if let Some(layout) = abi_params {
            let mut head_offset = 0_u64;
            let mut logical_physical = Vec::with_capacity(layout.types.len());
            for ty in &layout.types {
                let word = head_offset / 32;
                logical_physical.push(
                    (ty.is_scalar_word() && word < MAX_LAZY_ARG_WORDS)
                        .then(|| ArgIdx::new(word as usize)),
                );
                head_offset += abi_head_size(ty);
            }
            // The table also keeps the original arguments, whose stale `Value::Arg`s
            // stay typed.
            let num_params =
                ((head_offset / 32).min(MAX_LAZY_ARG_WORDS) as usize).max(arg_types.len());
            // ABI words can contain sign bits above the native integer width.
            let mut params = (0..num_params).map(|_| MirType::I256).collect::<IndexVec<_, _>>();
            for (logical, physical) in logical_physical.iter().enumerate() {
                if let Some(physical) = *physical
                    && let Some(&ty) = arg_types.get(logical)
                    && layout.types[logical].mir_type() == ty
                    && matches!(
                        layout.types[logical],
                        AbiParamType::Scalar(
                            ValueLayout::Bool | ValueLayout::UInt(_) | ValueLayout::Address
                        )
                    )
                {
                    params[physical] = ty;
                }
            }
            func.set_params(params);
            logical_values = logical_physical
                .into_iter()
                .map(|index| index.map(|index| func.alloc_arg(index)))
                .collect::<Vec<_>>();
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
                    .and_then(AbiParamType::word_validator)
                    .or_else(|| AbiWordValidator::from_layout(ty.value_layout()));
                head_offset += abi_type.map_or(32, abi_head_size);
                // A scalar without a lazy argument takes the word it is validated against.
                let eager = abi_type.is_some_and(AbiParamType::is_scalar_word)
                    && logical_values.get(index).is_some_and(Option::is_none);
                if validator.is_none() && !eager {
                    continue;
                }
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
                if eager {
                    logical_values[index] = Some(word);
                }
                let Some(validator) = validator else { continue };
                let valid = validator.condition_once(&mut builder, word);
                let next = builder.create_block();
                builder.branch(valid, next, validator_revert);
                current = next;
            }

            if let Some(layout) = abi_params {
                let mut head_offset = 0;
                for (index, ty) in layout.types.iter().enumerate() {
                    let arg_index = ArgIdx::new(index);
                    let uses = arg_uses.get(arg_index).map_or(&[][..], Vec::as_slice);
                    let head_size = abi_head_size(ty);
                    if ty.is_scalar_word()
                        || !arg_types
                            .get(index)
                            .is_some_and(|ty| matches!(*ty, MirType::MEMPTR | MirType::Slice(_)))
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
                    let validate_array_elements = constructor
                        || (location == AbiParamLocation::Memory
                            && Self::requires_calldata_element_validation(ty))
                        || !matches!(arg_type, MirType::Slice(PtrLocation::Calldata))
                        || self.needs_full_calldata_array_validation(builder.func(), uses, ty);
                    let helpers = &self.aggregate_type_helpers;
                    let decode_options = DecodeOptions {
                        validate_array_elements,
                        helpers: (!constructor).then_some(helpers),
                        ..DecodeOptions::new(constructor, input_end).bounded()
                    };
                    if uses.is_empty() && location != AbiParamLocation::Memory {
                        if ty.is_dynamic() {
                            // validate_dynamic_aggregate(head, tuple_base, input_end)
                            Self::validate_dynamic_aggregate_argument(
                                &mut builder,
                                ty,
                                head,
                                tuple_base,
                                input_end,
                                constructor,
                                &mut current,
                            );
                        }
                    } else {
                        let value = if !constructor
                            && arg_type == MirType::MEMPTR
                            && let Some(&helper) = self.aggregate_type_helpers.get(ty)
                        {
                            // value = icall @decode_calldata_type, head[, tuple_base]
                            let args = calldata_type_helper_args(ty, head, tuple_base);
                            builder.icall(helper, args, arg_type)
                        } else if !constructor
                            && matches!(arg_type, MirType::Slice(PtrLocation::Calldata))
                            && matches!(ty, AbiParamType::Bytes)
                            && let Some(helper) = self.calldata_slice_helper
                        {
                            let base = builder.icall(helper, vec![head], MirType::I256);
                            let len = builder.load(base, PtrLocation::Calldata);
                            let data = builder.add_u64_offset(base, 32);
                            builder.make_slice(data, len, PtrLocation::Calldata)
                        } else {
                            Self::decode_aggregate_argument(
                                &mut builder,
                                ty,
                                arg_type,
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
                        for &use_value in uses {
                            replacements.insert(use_value, value);
                        }
                    }
                    head_offset += head_size;
                }
            }

            builder.switch_to_block(current);
            for (logical, value) in logical_values.iter_mut().enumerate() {
                if let Some(raw) = *value
                    && let Some(abi_ty) = abi_params.and_then(|layout| layout.types.get(logical))
                    && abi_ty.is_scalar_word()
                {
                    let mut replacement = raw;
                    if let Some(&ty) = arg_types.get(logical)
                        && matches!(ty, MirType::Int(bits) if bits.get() < 256)
                    {
                        // value = cast raw to the declared scalar type
                        replacement = builder.cast(raw, ty);
                        *value = Some(replacement);
                    }
                    if matches!(abi_ty, AbiParamType::Scalar(ValueLayout::Function)) {
                        // value = shr value, 64
                        let shift = builder.imm(64);
                        replacement = builder.shr(replacement, shift);
                    }
                    for &use_value in arg_uses.get(ArgIdx::new(logical)).into_iter().flatten() {
                        replacements.insert(use_value, replacement);
                    }
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
        Self::discharge_abi_validations(func);
        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
        logical_values
    }

    /// Validates the immediate ABI shape of a dynamic aggregate without
    /// materializing its memory representation. Only entry points call it, so the input is
    /// bounded.
    fn validate_dynamic_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        constructor: bool,
        current: &mut BlockId,
    ) {
        builder.switch_to_block(*current);
        let offset = Self::load_input_word(builder, head, constructor);
        let base = Self::guard_input_offset(
            builder,
            tuple_base,
            offset,
            input_end,
            true,
            ty,
            current,
            RevertReason::InvalidTupleOffset,
            !constructor,
        );

        match ty {
            AbiParamType::DynamicArray(element) => {
                Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    abi_head_size(element),
                );
            }
            AbiParamType::FixedArray { .. } | AbiParamType::Tuple(..) => {
                let reason = Self::aggregate_short_reason(ty, !constructor);
                Self::guard_input_range(
                    builder,
                    base,
                    ty.data_head_size(),
                    input_end,
                    current,
                    reason,
                );
            }
            AbiParamType::Bytes => {
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                Self::guard_bytes_data(builder, data, len, input_end, current, false);
            }
            AbiParamType::Scalar(_) | AbiParamType::Enum { .. } => {
                unreachable!("scalar ABI value is not a dynamic aggregate")
            }
        }
    }

    fn decode_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        arg_type: MirType,
        head: ValueId,
        tuple_base: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) -> ValueId {
        let DecodeOptions {
            constructor,
            input_end,
            bounded_input,
            head_checked,
            validate_array_elements,
            helpers,
            offset_reason,
        } = options;
        builder.switch_to_block(*current);
        let is_dynamic = ty.is_dynamic();
        let location = if constructor { PtrLocation::Memory } else { PtrLocation::Calldata };
        let to_calldata = !constructor && matches!(arg_type, MirType::Slice(PtrLocation::Calldata));
        // The shared helper checks the value's head offset with the tuple reason, so a struct
        // member decodes inline when reasons are encoded. A calldata helper decodes a fresh,
        // fully validated memory struct.
        if (constructor
            || (decodes_nested(ty) && validate_array_elements && arg_type == ty.mir_type()))
            && head_checked
            && (!builder.encodes_revert_reasons()
                || offset_reason == RevertReason::InvalidTupleOffset)
            && let Some(&helper) = helpers.and_then(|helpers| helpers.get(ty))
        {
            // A calldata helper reads its input end from `calldatasize` itself.
            let args = if constructor {
                vec![head, tuple_base, input_end]
            } else {
                calldata_type_helper_args(ty, head, tuple_base)
            };
            // word_arguments = ptrtoint pointer_arguments to i256
            // decoded = icall helper, word_arguments
            let args = args.into_iter().map(|value| builder.cast(value, MirType::I256)).collect();
            return builder.icall(helper, args, ty.mir_type());
        }
        if !constructor
            && matches!(ty, AbiParamType::Bytes)
            && matches!(arg_type, MirType::Slice(PtrLocation::Calldata))
        {
            let (_, data, len) = Self::decode_calldata_bytes_slice_values(
                builder,
                head,
                tuple_base,
                input_end,
                bounded_input,
                current,
                head_checked,
                options.offset_reason,
            );
            return builder.make_slice(data, len, PtrLocation::Calldata);
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
                bounded_input,
                ty,
                current,
                options.offset_reason,
                to_calldata,
            )
        } else {
            head
        };
        if !is_dynamic
            && arg_type == MirType::MEMPTR
            && !constructor
            && matches!(ty, AbiParamType::FixedArray { .. } | AbiParamType::Tuple(..))
        {
            if !head_checked {
                Self::guard_input_range(
                    builder,
                    base,
                    abi_head_size(ty),
                    input_end,
                    current,
                    Self::aggregate_short_reason(ty, to_calldata),
                );
            }
            return Self::decode_static_argument(builder, ty, base, current, PtrLocation::Calldata);
        }
        match ty {
            AbiParamType::Scalar(_) | AbiParamType::Enum { .. } => {
                Self::decode_source_scalar(builder, ty, base, current, options)
            }
            AbiParamType::FixedArray { element, len } => {
                let head_size = ty.data_head_size();
                if !head_checked || is_dynamic {
                    let reason = Self::aggregate_short_reason(ty, to_calldata);
                    Self::guard_input_range(builder, base, head_size, input_end, current, reason);
                }
                if matches!(arg_type, MirType::Slice(PtrLocation::Calldata)) {
                    let length = builder.imm(*len);
                    if validate_array_elements && Self::is_scalar_array(ty) {
                        Self::validate_scalar_array(
                            builder, base, element, length, current, options,
                        );
                    }
                    return builder.make_slice(base, length, PtrLocation::Calldata);
                }
                let ptr =
                    Self::decode_aggregate(builder, ty, base, |builder, element, head, current| {
                        if element.is_scalar_word() {
                            Self::decode_source_scalar(
                                builder,
                                element,
                                head,
                                current,
                                options.checked(),
                            )
                        } else {
                            Self::decode_aggregate_argument(
                                builder,
                                element,
                                element.mir_type(),
                                head,
                                base,
                                current,
                                options.checked().nested(RevertReason::InvalidCalldataArrayOffset),
                            )
                        }
                    });
                *current = builder.current_block();
                ptr
            }
            AbiParamType::DynamicArray(element) if matches!(arg_type, MirType::Slice(_)) => {
                let (len, data) = Self::load_input_dynamic_array(
                    builder,
                    base,
                    input_end,
                    constructor,
                    current,
                    abi_head_size(element),
                );
                if !constructor && validate_array_elements && Self::is_scalar_or_enum(element) {
                    Self::validate_scalar_array(builder, data, element, len, current, options);
                }
                builder.make_slice(data, len, location)
            }
            AbiParamType::DynamicArray(element) if Self::is_full_word_scalar(element) => {
                let word = builder.imm(32);
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                let (ptr, _) = builder
                    .alloc_dynamic_word_array(len, AllocationSemantics::SOLIDITY_UNINITIALIZED);
                *current = builder.current_block();
                let bytes = builder.mul(len, word);
                Self::guard_input_dynamic_array(builder, len, data, input_end, current, 32);
                let source = builder.make_slice(data, bytes, location);
                builder.memory_copy_from_slice(ptr, source);
                ptr
            }
            AbiParamType::DynamicArray(element) if arg_type == MirType::MEMPTR => {
                let len = Self::load_input_word(builder, base, constructor);
                let data_base = builder.add_u64_offset(base, 32);
                let (ptr, _) = builder
                    .alloc_dynamic_word_array(len, AllocationSemantics::SOLIDITY_UNINITIALIZED);
                *current = builder.current_block();
                let word = builder.imm(32);
                let bytes = builder.mul(len, word);

                let copy_validated =
                    !constructor && validate_array_elements && Self::is_scalar_or_enum(element);
                Self::guard_input_dynamic_array(
                    builder,
                    len,
                    data_base,
                    input_end,
                    current,
                    abi_head_size(element),
                );
                if copy_validated {
                    Self::validate_scalar_array(builder, data_base, element, len, current, options);
                    let source = builder.make_slice(data_base, bytes, PtrLocation::Calldata);
                    builder.memory_copy_from_slice(ptr, source);
                    return ptr;
                }

                Self::decode_array_elements(
                    builder, element, ptr, len, data_base, current, options,
                );
                ptr
            }
            AbiParamType::Bytes if matches!(arg_type, MirType::Slice(_)) => {
                let len = Self::load_input_word(builder, base, constructor);
                let data = builder.add_u64_offset(base, 32);
                Self::guard_bytes_data(builder, data, len, input_end, current, false);
                builder.make_slice(data, len, location)
            }
            AbiParamType::Bytes => {
                let len = Self::load_input_word(builder, base, constructor);
                let word = builder.imm(32);
                let data = builder.add(base, word);
                builder.switch_to_block(*current);
                let ptr =
                    builder.alloc_bytes_object(len, AllocationSemantics::SOLIDITY_UNINITIALIZED);
                *current = builder.current_block();
                Self::guard_bytes_data(builder, data, len, input_end, current, true);
                let source = builder.make_slice(data, len, location);
                builder.memory_copy_from_slice(ptr, source);
                // payload = ptradd ptr, 32
                // tail = ptradd payload, len
                // padding = and (sub 0, len), 31
                // memory_zero tail, padding
                // After the copy, which may read memory the new object overlaps. The clear stays
                // inside the object: it spans the padding bytes only.
                let payload = builder.memory_data(ptr);
                let tail = builder.ptradd(payload, len);
                let zero = builder.imm(0);
                let negative_len = builder.sub(zero, len);
                let mask = builder.imm(EvmMemoryLayout::WORD_SIZE - 1);
                let padding = builder.and(negative_len, mask);
                builder.memory_zero(tail, padding);
                ptr
            }
            AbiParamType::Tuple(fields) => {
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
                if matches!(arg_type, MirType::Slice(PtrLocation::Calldata)) {
                    let length = builder.imm(abi_head_size(ty));
                    return builder.make_slice(base, length, PtrLocation::Calldata);
                }
                let carries_base = !constructor && ty.has_dynamic_child();
                let storage_fields = fields.len() + usize::from(carries_base);
                let ptr =
                    builder.alloc_word_struct(storage_fields as u64, AllocationSemantics::INTERNAL);
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
                    builder.memory_store_field(ptr, index as u64, value);
                    offset += abi_head_size(field);
                }
                if carries_base {
                    builder.memory_store_field(ptr, fields.len() as u64, base);
                }
                ptr
            }
            _ => builder.undef(arg_type),
        }
    }

    /// Validates the `len` scalar words at `data`, whose range the caller has checked.
    ///
    /// Folds the words of a zero-bits validator with OR and checks once after the loop, which
    /// reverts the same way; other validators check each element.
    fn validate_scalar_array(
        builder: &mut FunctionBuilder<'_>,
        data: ValueId,
        element: &AbiParamType,
        len: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) {
        let word = builder.imm(32);
        builder.switch_to_block(*current);
        if let Some(validator) = element.word_validator().and_then(AbiWordValidator::zero_bits) {
            // jumpi (ne len, 0), start, done
            // start: bytes = mul len, 32
            // end = add data, bytes
            // jump body
            // body: cursor = phi [start: data], [body: next_cursor]
            // folded = phi [start: 0], [body: next]
            // next = or folded, (load cursor)
            // next_cursor = add cursor, 32
            // jumpi (lt next_cursor, end), body, validate
            // validate: revert_if_zero (valid next)
            // jump done
            let zero = builder.imm(0);
            let start = builder.create_block();
            let body = builder.create_block();
            let validate = builder.create_block();
            let done = builder.create_block();
            let has_words = builder.ne_zero(len);
            builder.branch(has_words, start, done);
            builder.switch_to_block(start);
            let bytes = builder.mul(len, word);
            let end = builder.add(data, bytes);
            builder.jump(body);
            builder.switch_to_block(body);
            let cursor = builder.phi(vec![(start, data)]);
            let folded = builder.phi(vec![(start, zero)]);
            let value = Self::load_input_word(builder, cursor, options.constructor);
            let next = builder.or(folded, value);
            let next_cursor = builder.add(cursor, word);
            let more = builder.lt(next_cursor, end);
            builder.branch(more, body, validate);
            builder.add_phi_incoming(cursor, body, next_cursor);
            builder.add_phi_incoming(folded, body, next);
            builder.switch_to_block(validate);
            Self::validate_abi_word(builder, next, validator);
            builder.jump(done);
            builder.switch_to_block(done);
            *current = builder.current_block();
            return;
        }
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

    /// Decodes `len` ABI elements whose heads start at `data_base` into the
    /// memory array `ptr`.
    fn decode_array_elements(
        builder: &mut FunctionBuilder<'_>,
        element: &AbiParamType,
        ptr: ValueId,
        len: ValueId,
        data_base: ValueId,
        current: &mut BlockId,
        options: DecodeOptions<'_>,
    ) {
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
        builder.memory_store_element(ptr, MemoryObjectLayout::WORD_ARRAY, destination_index, value);
        let one = builder.imm(1);
        let next_remaining = builder.sub(remaining, one);
        let element_head_size = builder.imm(abi_head_size(element));
        let next_source = builder.add(source, element_head_size);
        let next_destination_index = builder.add(destination_index, one);
        builder.switch_to_block(element_current);
        builder.jump(cond);
        builder.add_phi_incoming(remaining, element_current, next_remaining);
        builder.add_phi_incoming(source, element_current, next_source);
        builder.add_phi_incoming(destination_index, element_current, next_destination_index);

        builder.switch_to_block(done);
        *current = done;
    }

    fn decode_aggregate(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        mut decode: impl FnMut(
            &mut FunctionBuilder<'_>,
            &AbiParamType,
            ValueId,
            &mut BlockId,
        ) -> ValueId,
    ) -> ValueId {
        match ty {
            AbiParamType::FixedArray { element, len } => {
                let (object, layout) =
                    builder.alloc_word_array(*len, AllocationSemantics::INTERNAL);
                let length = builder.imm(*len);
                Self::decode_word_array_elements(
                    builder, element, object, layout, length, head, decode,
                );
                object
            }
            AbiParamType::Tuple(fields) => {
                let object =
                    builder.alloc_word_struct(fields.len() as u64, AllocationSemantics::INTERNAL);
                let mut offset = 0;
                let mut current = builder.current_block();
                for (index, field) in fields.iter().enumerate() {
                    let field_head = builder.add_u64_offset(head, offset);
                    let value = decode(builder, field, field_head, &mut current);
                    builder.switch_to_block(current);
                    let value = Self::encode_memory_scalar(builder, field, value);
                    builder.memory_store_field(object, index as u64, value);
                    offset += abi_head_size(field);
                }
                object
            }
            AbiParamType::Bytes | AbiParamType::DynamicArray(_) => {
                unreachable!("dynamic ABI value in static aggregate")
            }
            AbiParamType::Scalar(_) | AbiParamType::Enum { .. } => {
                unreachable!("scalar ABI word handled above")
            }
        }
    }

    /// Decodes `length` elements with heads at `head + i * stride` into `object`.
    fn decode_word_array_elements(
        builder: &mut FunctionBuilder<'_>,
        element: &AbiParamType,
        object: ValueId,
        layout: MemoryObjectLayout,
        length: ValueId,
        head: ValueId,
        mut decode: impl FnMut(
            &mut FunctionBuilder<'_>,
            &AbiParamType,
            ValueId,
            &mut BlockId,
        ) -> ValueId,
    ) {
        let stride = builder.imm(abi_head_size(element));
        // for i in 0..length { object[i] = decode(head + i * stride) }
        builder.counted_loop(length, |builder, index| {
            let offset = builder.mul(index, stride);
            let field = builder.add(head, offset);
            let mut current = builder.current_block();
            let value = decode(builder, element, field, &mut current);
            builder.switch_to_block(current);
            let value = Self::encode_memory_scalar(builder, element, value);
            builder.memory_store_element(object, layout, index, value);
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_calldata_bytes_slice_values(
        builder: &mut FunctionBuilder<'_>,
        head: ValueId,
        tuple_base: ValueId,
        input_end: ValueId,
        bounded_input: bool,
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
        let offset = builder.load(head, PtrLocation::Calldata);
        let base = builder.add(tuple_base, offset);
        let word = builder.imm(32);
        if builder.encodes_revert_reasons() {
            // Report solc's checks separately: an unencodable offset belongs to the enclosing
            // tuple, struct, or array, then the head range, length, and stride checks.
            let target_end = builder.add(base, word);
            let max_offset = builder.imm(u64::MAX);
            let offset_overflow = builder.gt(offset, max_offset);
            let head_out_of_range = builder.gt(target_end, input_end);
            *current = builder.revert_if(offset_overflow, offset_reason);
            *current =
                builder.revert_if(head_out_of_range, RevertReason::InvalidCalldataArrayOffset);
            let len = builder.load(base, PtrLocation::Calldata);
            let data = builder.add(base, word);
            Self::guard_bytes_data(builder, data, len, input_end, current, false);
            return (base, data, len);
        }
        // if offset > input_end - tuple_base - 32 { revert(Error("invalid calldata array offset"))
        // } for a bounded input, as in `fused_offset_invalid`.
        let head_invalid =
            Self::fused_offset_invalid(builder, tuple_base, offset, input_end, bounded_input);
        let len = builder.load(base, PtrLocation::Calldata);
        let data = builder.add(base, word);
        let remaining = builder.sub(input_end, data);
        let tail_invalid = builder.gt(len, remaining);
        let invalid = builder.or(head_invalid, tail_invalid);
        *current = builder.revert_if(invalid, RevertReason::InvalidCalldataArrayOffset);
        (base, data, len)
    }

    /// Decodes a static value at `head` in `location`. The word checks of a scalar or a fixed word
    /// array share one revert; the fields of an aggregate each revert on their own.
    fn decode_static_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        current: &mut BlockId,
        location: PtrLocation,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let mut valid = builder.imm(1);
        let value = Self::decode_static_value(builder, ty, head, &mut valid, location);
        let invalid = builder.eq_zero(valid);
        *current = builder.revert_if(invalid, RevertReason::Empty);
        value
    }

    fn decode_static_value(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        head: ValueId,
        valid: &mut ValueId,
        location: PtrLocation,
    ) -> ValueId {
        if let Some(validator) = ty.word_validator() {
            let value = builder.load(head, location);
            let condition = validator.condition(builder, value);
            *valid = builder.and(*valid, condition);
            return Self::normalize_abi_word(builder, ty, value);
        }
        if matches!(ty, AbiParamType::Scalar(_)) {
            return builder.load(head, location);
        }
        if let AbiParamType::FixedArray { element, len } = ty
            && Self::is_scalar_or_enum(element)
            && !matches!(element.as_ref(), AbiParamType::Scalar(ValueLayout::Function))
        {
            let (object, _) = builder.alloc_word_array(*len, AllocationSemantics::INTERNAL);
            if element.word_validator().is_some() {
                let length = builder.imm(*len);
                let stride = builder.imm(EvmMemoryLayout::WORD_SIZE);
                builder.counted_loop(length, |builder, index| {
                    let offset = builder.mul(index, stride);
                    let element_head = builder.add(head, offset);
                    let mut current = builder.current_block();
                    Self::decode_static_argument(
                        builder,
                        element,
                        element_head,
                        &mut current,
                        location,
                    );
                    builder.switch_to_block(current);
                });
            }
            let byte_length = builder.imm(
                len.checked_mul(EvmMemoryLayout::WORD_SIZE).expect("checked static ABI array size"),
            );
            let source = builder.make_slice(head, byte_length, location);
            builder.memory_copy_to_object(object, source);
            return object;
        }
        Self::decode_aggregate(builder, ty, head, |builder, ty, head, current| {
            Self::decode_static_argument(builder, ty, head, current, location)
        })
    }

    fn load_input_word(
        builder: &mut FunctionBuilder<'_>,
        position: ValueId,
        constructor: bool,
    ) -> ValueId {
        builder
            .load(position, if constructor { PtrLocation::Memory } else { PtrLocation::Calldata })
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
            builder.shr(remaining, shift)
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
        ty: &AbiParamType,
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
            Self::validate_abi_word(builder, value, validator);
            *current = builder.current_block();
        }
        Self::normalize_abi_word(builder, ty, value)
    }

    /// Forms the absolute address of a dynamically encoded value from its head `offset` and
    /// checks that its first word lies inside the input.
    ///
    /// In debug mode the check is split: an unencodable offset reports `offset_reason`, and the
    /// target range check reports the message of the value's own decoder. `to_calldata` selects the
    /// message for a struct that stays in calldata.
    #[allow(clippy::too_many_arguments)]
    fn guard_input_offset(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        input_end: ValueId,
        bounded_input: bool,
        ty: &AbiParamType,
        current: &mut BlockId,
        offset_reason: RevertReason,
        to_calldata: bool,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let target = builder.add(base, offset);
        let is_array_like = matches!(ty, AbiParamType::DynamicArray(_) | AbiParamType::Bytes);
        if builder.encodes_revert_reasons() {
            // Split so each comparison reports its own message; without revert strings, arrays
            // and bytes fuse them into one comparison below:
            // if offset > 0xffffffffffffffff { revert(Error(offset_reason)) }
            // if offset > input_end - base { revert(Error("struct ... too short" | "stride")) }
            //   or, for arrays and bytes,
            // if target + 32 > input_end { revert(Error("invalid calldata array offset")) }
            let max_offset = builder.imm(u64::MAX);
            let overflow = builder.gt(offset, max_offset);
            *current = builder.revert_if(overflow, offset_reason);
            // solc decodes a fixed array with dynamic elements to memory through its array
            // decoder, which checks the first word before the element head sizes.
            let checks_first_word =
                is_array_like || (!to_calldata && matches!(ty, AbiParamType::FixedArray { .. }));
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

        // Every dynamic ABI value starts with a word-sized head, so one offset comparison proves
        // the head word fits and the value-specific decoder needs no second range guard.
        let invalid = Self::fused_offset_invalid(builder, base, offset, input_end, bounded_input);
        *current = builder.revert_if(invalid, RevertReason::InvalidCalldataArrayOffset);
        target
    }

    /// Tests that a dynamic value's head offset is unencodable or puts its head word outside the
    /// input, without revert strings.
    ///
    /// A bounded input is checked against `input_end - base - 32`, which cannot wrap given the
    /// checked head word and also rejects offsets above `u64::MAX` and offsets that wrap
    /// `base + offset + 32`. An unbounded one keeps the `u64::MAX` test, since `abi.decode` of a
    /// long input would otherwise accept offsets solc rejects.
    fn fused_offset_invalid(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: ValueId,
        input_end: ValueId,
        bounded_input: bool,
    ) -> ValueId {
        if bounded_input {
            // offset > input_end - base - 32
            let remaining = builder.sub(input_end, base);
            let word = builder.imm(32);
            let max_offset = builder.sub(remaining, word);
            return builder.gt(offset, max_offset);
        }
        // offset > u64::MAX || base + offset + 32 > input_end
        let target = builder.add(base, offset);
        let target_end = builder.add_u64_offset(target, 32);
        let max_offset = builder.imm(u64::MAX);
        let overflow = builder.gt(offset, max_offset);
        let out_of_range = builder.gt(target_end, input_end);
        builder.or(overflow, out_of_range)
    }

    /// The debug message for a static aggregate whose head does not fit the input. Like solc,
    /// structs that stay in calldata report "struct calldata too short" and structs decoded to
    /// memory report "struct data too short"; fixed arrays reuse the array stride check.
    fn aggregate_short_reason(ty: &AbiParamType, to_calldata: bool) -> RevertReason {
        match ty {
            AbiParamType::Tuple(..) if to_calldata => RevertReason::StructCalldataTooShort,
            AbiParamType::Tuple(..) => RevertReason::StructDataTooShort,
            AbiParamType::FixedArray { .. } => RevertReason::InvalidCalldataArrayStride,
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

    fn is_scalar_array(ty: &AbiParamType) -> bool {
        matches!(
            ty,
            AbiParamType::DynamicArray(element)
                | AbiParamType::FixedArray { element, .. }
                if Self::is_scalar_or_enum(element)
        )
    }

    /// Returns whether a calldata scalar array must be validated in full.
    ///
    /// Calldata indexing validates the selected word at the point of access.
    /// A full pass is only needed when the array itself crosses another ABI or
    /// memory boundary, such as encoding it or passing it to an external call.
    /// An internal callee that takes it as a calldata slice validates the
    /// elements it reads, copies or encodes, so passing it there needs none.
    fn needs_full_calldata_array_validation(
        &self,
        func: &Function,
        uses: &[ValueId],
        ty: &AbiParamType,
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
                | InstKind::Copy(..)
                | InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. } => return true,
                InstKind::ICall { function: Callee::Function(function), args, .. } => {
                    let callee_params = self.function_params.get(*function);
                    let preserves_calldata = args.iter().enumerate().all(|(index, value)| {
                        !tainted.contains(*value)
                            || callee_params.and_then(|params| params.get(index))
                                == Some(&MirType::Slice(PtrLocation::Calldata))
                    });
                    // A callee that takes the array as a calldata slice validates each element
                    // where it reads, copies or encodes it, as solc does, so the call alone
                    // needs no validation here, whether or not it returns a value.
                    if preserves_calldata {
                        continue;
                    }
                    true
                }
                InstKind::ExtractValue { ty: MirType::Slice(_), .. } => false,
                InstKind::Load(
                    _,
                    PtrLocation::Memory | PtrLocation::Calldata | PtrLocation::Returndata,
                ) => result.is_some_and(|value| {
                    func.value_ty(value)
                        .is_some_and(|ty| matches!(ty, MirType::MEMPTR | MirType::Slice(_)))
                }),
                _ => result.is_some(),
            };
            if action {
                if let Some(result) = result {
                    tainted.insert(result);
                } else {
                    return true;
                }
            } else if result.is_none() {
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

    fn can_encode_calldata_slice(ty: &AbiParamType) -> bool {
        matches!(ty, AbiParamType::Bytes) || Self::is_scalar_array(ty)
    }

    fn requires_calldata_element_validation(ty: &AbiParamType) -> bool {
        matches!(
            ty,
            AbiParamType::DynamicArray(element)
                | AbiParamType::FixedArray { element, .. }
                if Self::is_scalar_or_enum(element) && !Self::is_full_word_scalar(element)
        )
    }

    fn is_full_word_scalar(ty: &AbiParamType) -> bool {
        matches!(ty, AbiParamType::Scalar(scalar) if scalar.is_full_abi_word())
    }

    fn is_scalar_or_enum(ty: &AbiParamType) -> bool {
        ty.is_scalar_word() && !matches!(ty, AbiParamType::Scalar(ValueLayout::Function))
    }

    fn normalize_abi_word(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        value: ValueId,
    ) -> ValueId {
        if matches!(ty, AbiParamType::Scalar(ValueLayout::Function)) {
            let shift = builder.imm(64);
            builder.shr(value, shift)
        } else {
            value
        }
    }

    fn encode_memory_scalar(
        builder: &mut FunctionBuilder<'_>,
        ty: &AbiParamType,
        value: ValueId,
    ) -> ValueId {
        if matches!(ty, AbiParamType::Scalar(ValueLayout::Function)) {
            let shift = builder.imm(64);
            return builder.shl(value, shift);
        }
        value
    }

    fn validate_abi_word(
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        validator: AbiWordValidator,
    ) {
        let valid = validator.condition(builder, value);
        builder.revert_if_zero(valid, RevertReason::Empty);
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

/// Decodes an ABI tuple of `length` bytes at `base`, as an external entry decodes its own
/// calldata when `calldata` holds: offsets resolve against the base, every read stays below the
/// end, and dynamic values are copied into memory. Reading a calldata slice directly spares the
/// copy of the whole encoding into memory that decoding a memory object needs. Returns the fields
/// cast to their declared types.
fn decode_tuple(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    length: ValueId,
    layout: &AbiParamLayout,
    calldata: bool,
    helpers: Option<&FxHashMap<AbiParamType, FunctionId>>,
) -> Vec<ValueId> {
    debug_assert!(!calldata || helpers.is_none(), "calldata decoders take no memory helpers");
    let head_size = layout.checked_head_size().expect("checked ABI layout");
    let mut current = builder.current_block();
    let input_end =
        LowerAbiCx::validate_memory_tuple_input(builder, base, length, head_size, &mut current);

    let mut values = Vec::with_capacity(layout.types.len());
    let static_layout = layout.types.iter().all(|ty| !ty.is_dynamic());
    let mut head_offset = 0_u64;
    for ty in &layout.types {
        builder.switch_to_block(current);
        let head = builder.add_u64_offset(base, head_offset);
        let value = if static_layout {
            let location = if calldata { PtrLocation::Calldata } else { PtrLocation::Memory };
            LowerAbiCx::decode_static_argument(builder, ty, head, &mut current, location)
        } else {
            LowerAbiCx::decode_aggregate_argument(
                builder,
                ty,
                ty.mir_type(),
                head,
                base,
                &mut current,
                DecodeOptions { helpers, ..DecodeOptions::new(!calldata, input_end).checked() },
            )
        };
        values.push(value);
        head_offset += abi_head_size(ty);
    }
    builder.switch_to_block(current);
    // field = cast decoded word to the declared field type
    for (value, ty) in values.iter_mut().zip(&layout.types) {
        *value = builder.cast(*value, ty.mir_type());
    }
    values
}

/// Returns whether a shared decoder of a memory slice takes only the slice's base, which holds for
/// static layouts: they read the head alone, so the caller checks the length, which is usually the
/// constant head size, and the check folds away.
fn passes_slice_base(data_ty: MirType, layout: &AbiParamLayout) -> bool {
    matches!(data_ty, MirType::Slice(_)) && !layout.types.iter().any(AbiParamType::is_dynamic)
}

/// Returns the base and length of an ABI decode's input: a calldata slice, a bytes object, or a
/// memory slice.
fn decode_input(
    builder: &mut FunctionBuilder<'_>,
    data: ValueId,
    calldata: bool,
) -> (ValueId, ValueId) {
    if calldata {
        // base = ptrtoint (extract_value data, 0)
        // length = extract_value data, 1
        let ptr = builder.slice_ptr(data);
        return (builder.cast(ptr, MirType::I256), builder.slice_len(data));
    }
    // base = inttoptr ((extract_value data, 0) | ptrtoint data + 32)
    // length = (extract_value data, 1) | load data
    let ptr = builder.memory_data(data);
    let base = builder.cast(ptr, MirType::MEMPTR);
    (base, builder.memory_len(data))
}

/// Returns the head size of a layout the preflight scan accepted.
fn abi_head_size(ty: &AbiParamType) -> u64 {
    ty.checked_head_size().expect("ABI head size exceeds u64 range")
}

/// Returns whether shared calldata decoders also decode `ty` nested in other values, which makes
/// its decoder take the base of the enclosing tuple.
fn decodes_nested(ty: &AbiParamType) -> bool {
    matches!(ty, AbiParamType::Tuple(_)) && ty.is_dynamic()
}

/// Returns the arguments of the shared calldata decoder of `ty`.
fn calldata_type_helper_args(
    ty: &AbiParamType,
    head: ValueId,
    tuple_base: ValueId,
) -> Vec<ValueId> {
    if decodes_nested(ty) { vec![head, tuple_base] } else { vec![head] }
}

/// Adds `occurrences` to the use count of `ty`, saturating at two.
fn count_type(
    counts: &mut FxHashMap<AbiParamType, (usize, usize)>,
    ty: &AbiParamType,
    occurrences: usize,
) {
    let first = counts.len();
    let count = match counts.get_mut(ty) {
        Some(count) => count,
        None => counts.entry(ty.clone()).or_insert((0, first)),
    };
    count.0 = count.0.saturating_add(occurrences).min(2);
}

/// Returns the types counted at least twice, inner types before the types that contain them.
fn repeated_types(counts: FxHashMap<AbiParamType, (usize, usize)>) -> Vec<AbiParamType> {
    let mut types = counts
        .into_iter()
        .filter_map(|(ty, (count, first))| (count >= 2).then_some((first, ty)))
        .collect::<Vec<_>>();
    types.sort_by_key(|(first, ty)| (abi_param_type_depth(ty), *first));
    types.into_iter().map(|(_, ty)| ty).collect()
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
                count_type(counts, ty, occurrences);
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
) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::decode_memory_type));
    {
        let mut builder = FunctionBuilder::new(&mut function).with_revert_strings(revert_strings);
        let head = builder.add_param(MirType::I256);
        let tuple_base = builder.add_param(MirType::I256);
        let input_end = builder.add_param(MirType::I256);
        let mut current = builder.current_block();
        let value = LowerAbiCx::decode_aggregate_argument(
            &mut builder,
            ty,
            ty.mir_type(),
            head,
            tuple_base,
            &mut current,
            DecodeOptions {
                helpers: Some(helpers),
                ..DecodeOptions::new(true, input_end).checked()
            },
        );
        builder.set_return_type(ty.mir_type());
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
        && matches!(func.params[ArgIdx::new(0)], MirType::Slice(PtrLocation::Calldata))
        && matches!(func.return_components(), [MirType::MEMPTR])
}

/// Whether every value-carrying fallback return can use raw bytes returndata.
fn can_lower_bytes_fallback_returns(func: &Function) -> bool {
    func.blocks.iter().all(|block| {
        let Some(Terminator::Return { values }) = &block.terminator else { return true };
        values.len() == 1 && func.value_ty(values[0]) == Some(MirType::MEMPTR)
    })
}

/// Whether every value-carrying return has a matching semantic ABI layout.
fn can_encode_live_returns(module: &Module, func: &Function) -> bool {
    func.blocks.iter().all(|block| {
        let Some(Terminator::Return { values }) = &block.terminator else {
            return true;
        };
        let count = match values.as_slice() {
            [value] => match func.value_ty(*value) {
                Some(MirType::Struct(id)) => module.struct_types[id].fields.len(),
                _ => 1,
            },
            _ => values.len(),
        };
        values.is_empty()
            || func.abi_returns.as_ref().is_some_and(|layout| layout.types.len() == count)
    })
}

fn find_canonical_return_calls(
    module: &Module,
    targets: &[FunctionId],
) -> FxHashSet<(FunctionId, AbiParamType)> {
    let mut candidates = FxHashSet::default();
    for &id in targets {
        let func = module.function(id);
        let Some(layout) = &func.abi_return_params else { continue };
        for block in &func.blocks {
            let Some(Terminator::Return { values }) = &block.terminator else { continue };
            for (&value, ty) in values.iter().zip(&layout.types) {
                // Lowering types a returned array as the raw pointer it is, so a proved
                // element width stands in for the exact type match.
                if !ty.is_scalar_word()
                    && ty.needs_return_cleanup()
                    && let Value::Inst(inst) = func.value(value)
                    && let InstKind::ICall { function: Callee::Function(function), .. } =
                        func.inst(*inst).kind
                    && module.function(function).return_components().len() == 1
                    && (func.value_ty(value) == Some(ty.mir_type())
                        || module.function(function).attributes.array_return_element_bits.is_some())
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
    let Value::Arg(index) = func.value(value) else { return None };
    let mut physical = 0;
    for ty in input_params?.types.iter() {
        if ty.is_scalar_word() && physical == index.index() {
            return Some(ty);
        }
        physical += (abi_head_size(ty) / EvmMemoryLayout::WORD_SIZE) as usize;
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

fn canonical_scalar_covers_return(input: ValueLayout, output: ValueLayout) -> bool {
    match (input, output) {
        (ValueLayout::UInt(input), ValueLayout::UInt(output)) => input.bits() <= output.bits(),
        (ValueLayout::UInt(input), ValueLayout::Address) => input.bits() <= 160,
        (ValueLayout::Address, ValueLayout::UInt(output)) => output.bits() >= 160,
        (ValueLayout::Int(input), ValueLayout::Int(output)) => input.bits() <= output.bits(),
        (ValueLayout::FixedBytes(input), ValueLayout::FixedBytes(output)) => {
            input.bytes() <= output.bytes()
        }
        (
            ValueLayout::Bool,
            ValueLayout::Bool | ValueLayout::UInt(_) | ValueLayout::Int(_) | ValueLayout::Address,
        ) => true,
        _ => input == output,
    }
}

fn is_canonical_return_value(
    func: &Function,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
) -> bool {
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
    let value = if ty.is_scalar_word() { value } else { returned_object_root(func, value) };
    if !ty.needs_return_cleanup()
        || (source == ReturnValueSource::Scalar
            && canonical_input_for_arg(func, value, input_params)
                .is_some_and(|input| canonical_input_covers_return(input, ty)))
    {
        return true;
    }
    // Element cleanup's proof covers every write into the array this function returns.
    if let AbiParamType::DynamicArray(element) | AbiParamType::FixedArray { element, .. } = ty
        && returns_proved_array(func, element, value)
    {
        return true;
    }
    if !ty.is_scalar_word()
        && !memory_writes_stay_private(func, calls.module, &mut FxHashSet::default())
    {
        return false;
    }
    if calls.module.is_some()
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::ICall { function: Callee::Function(function), .. } = func.inst(*inst).kind
        && calls
            .module
            .is_some_and(|module| module.function(function).return_components().len() == 1)
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
        // Runtime-index stores need an allocation extent proof before cleanup can be omitted.
        AbiParamType::DynamicArray(element) => {
            is_canonical_return_array_param(func, element, value, calls)
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
    // Element cleanup proved the words of the array this function returns.
    if let AbiParamType::DynamicArray(element) | AbiParamType::FixedArray { element, .. } = ty
        && let Some(bits) = abi_element_bits(element)
        && func.attributes.array_return_element_bits.is_some_and(|proved| proved <= bits)
    {
        calls.visiting.remove(&key);
        return true;
    }
    let mut value_visiting = FxHashSet::default();
    let result = func.return_components() == [ty.mir_type()]
        && func
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
        && func.blocks.iter().all(|block| {
            let Some(Terminator::Return { values }) = &block.terminator else { return true };
            value_visiting.clear();
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

/// Follows only conversions that preserve every pointer bit.
fn pointer_alias_root(func: &Function, mut value: ValueId) -> ValueId {
    // A malformed cyclic definition must not make the proof loop forever.
    for _ in 0..func.num_values() {
        let Value::Inst(inst) = func.value(value) else { break };
        let Some(input) = lossless_pointer_cast(func, *inst) else { break };
        value = input;
    }
    value
}

/// Follows only conversions that keep the address, so the proof checks the words the
/// encoding reads; unlike [`pointer_alias_root`] it stops at a nonzero offset.
fn returned_object_root(func: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..func.num_values() {
        let Value::Inst(inst) = func.value(value) else { break };
        if let InstKind::PtrAdd(_, offset) = func.inst(*inst).kind
            && func.value_u64(offset) != Some(0)
        {
            break;
        }
        let Some(input) = lossless_pointer_cast(func, *inst) else { break };
        value = input;
    }
    value
}

/// The alias root of the object a word store writes, with the word it writes: a word-aligned
/// constant offset into a fixed array or struct.
fn fixed_word_store(func: &Function, inst: InstId) -> Option<(ValueId, u64, ValueId)> {
    let InstKind::Store(address, value, PtrLocation::Memory) = func.inst(inst).kind else {
        return None;
    };
    let (object, offset) = func.object_address(address)?;
    let slot = EvmMemoryLayout::word_slot(offset)?;
    Some((pointer_alias_root(func, object), slot, value))
}

/// The array a word element load reads: a word at a runtime index, or at a word-aligned constant
/// offset past the first word, which is a dynamic array's length.
fn array_word_object(func: &Function, address: ValueId) -> Option<ValueId> {
    let (object, offset, index) = func.word_access(address)?;
    (index.is_some() || offset != 0).then_some(object)
}

/// Returns the calldata slice a copy of `len` bytes from `data` reads whole:
/// `calldatacopy dest, (extract_value slice, 0), (extract_value slice, 1)`.
fn calldata_slice_copy(func: &Function, data: ValueId, len: ValueId) -> Option<ValueId> {
    let Value::Inst(ptr) = func.value(data) else { return None };
    let Some((slice, MirType::SLICE_PTR_FIELD)) = func.inst(*ptr).kind.slice_projection() else {
        return None;
    };
    let Value::Inst(length) = func.value(len) else { return None };
    (func.inst(*length).kind.slice_projection() == Some((slice, MirType::SLICE_LEN_FIELD))
        && func.value_ty(slice) == Some(MirType::Slice(PtrLocation::Calldata)))
    .then_some(slice)
}

fn lossless_pointer_cast(func: &Function, inst: InstId) -> Option<ValueId> {
    let result = func.inst_result_value(inst).and_then(|value| func.value_ty(value))?;
    match func.inst(inst).kind {
        InstKind::PtrToInt(value, 256)
            if result == MirType::I256 && func.value_ty(value).is_some_and(MirType::is_pointer) =>
        {
            Some(value)
        }
        InstKind::IntToPtr(value)
            if result.is_pointer() && func.value_ty(value) == Some(MirType::I256) =>
        {
            Some(value)
        }
        // A slice reads through the object or pointer it was made from.
        InstKind::MakeSlice { ptr: value, location: PtrLocation::Memory, .. } => Some(value),
        // A constant offset stays inside the object.
        InstKind::PtrAdd(value, offset) if func.value_u64(offset).is_some() => Some(value),
        _ => None,
    }
}

/// Rejects writes through unknown aliases, including effects hidden in callees.
fn memory_writes_stay_private(
    func: &Function,
    module: Option<&Module>,
    visiting: &mut FxHashSet<FunctionId>,
) -> bool {
    let aliases = AliasAnalysis::new(func);
    func.instructions().all(|inst| {
        let kind = &func.inst(inst).kind;
        match *kind {
            InstKind::Alloc { .. } => true,
            _ if kind.effect_kind() == EffectKind::MemoryWrite => {
                let location = match *kind {
                    InstKind::MemoryZero(object, size) => aliases.memory_location(
                        func,
                        inst,
                        object,
                        aliases.location_size(func, size),
                    ),
                    InstKind::Store(address, _, PtrLocation::Memory) => aliases
                        .memory_location(func, inst, address, LocationSize::Const(32))
                        .filter(|location| location.size.as_const().is_some()),
                    _ => None,
                };
                // Runtime indices need a separate extent proof; a payload range is insufficient.
                location.is_some_and(|location| {
                    matches!(
                        location.address.base,
                        MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_)
                    )
                })
            }
            InstKind::ICall { function: Callee::Function(id), .. } => {
                let Some(module) = module else { return false };
                if !visiting.insert(id) {
                    return false;
                }
                let private =
                    memory_writes_stay_private(module.function(id), Some(module), visiting);
                visiting.remove(&id);
                private
            }
            _ => !matches!(
                kind.effect_kind(),
                EffectKind::MemoryWrite
                    | EffectKind::ICall
                    | EffectKind::ExternalCall
                    | EffectKind::Create
            ),
        }
    })
}

/// A stored pointer cannot be reloaded or exposed through its containing object.
fn container_keeps_pointer_private(func: &Function, object: ValueId) -> bool {
    let mut pending = Vec::new();
    pending.push(pointer_alias_root(func, object));
    let mut seen = FxHashSet::default();
    while let Some(object) = pending.pop() {
        if !seen.insert(object) {
            continue;
        }
        let Value::Inst(alloc) = func.value(object) else { return false };
        if !matches!(func.inst(*alloc).kind, InstKind::Alloc { .. }) {
            return false;
        }
        for inst in func.instructions() {
            let kind = &func.inst(inst).kind;
            if !kind.operands().iter().any(|&value| pointer_alias_root(func, value) == object) {
                continue;
            }
            if lossless_pointer_cast(func, inst).is_some() {
                continue;
            }
            match *kind {
                InstKind::Store(base, value, PtrLocation::Memory) => {
                    if pointer_alias_root(func, value) == object {
                        pending.push(pointer_alias_root(func, base));
                    } else if pointer_alias_root(func, base) != object {
                        return false;
                    }
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: base,
                    index: MirType::SLICE_LEN_FIELD,
                } if pointer_alias_root(func, base) == object => {}
                _ => return false,
            }
        }
        if func.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|term| {
                !matches!(term, Terminator::Return { .. })
                    && term
                        .operands()
                        .iter()
                        .any(|&value| pointer_alias_root(func, value) == object)
            })
        }) {
            return false;
        }
    }
    true
}

fn is_unmodified_call_result(func: &Function, value: ValueId, call: InstId) -> bool {
    let value = pointer_alias_root(func, value);
    for inst in func.instructions() {
        if inst == call || lossless_pointer_cast(func, inst).is_some() {
            continue;
        }
        match func.inst(inst).kind {
            InstKind::ExtractValue {
                ty: MirType::Slice(_),
                aggregate: object,
                index: MirType::SLICE_LEN_FIELD,
            }
            | InstKind::Load(object, PtrLocation::Memory)
                if pointer_alias_root(func, object) == value => {}
            InstKind::Store(object, stored, PtrLocation::Memory)
                if pointer_alias_root(func, stored) == value
                    && pointer_alias_root(func, object) != value
                    && container_keeps_pointer_private(func, object) => {}
            _ if func
                .inst(inst)
                .operands()
                .iter()
                .any(|&operand| pointer_alias_root(func, operand) == value) =>
            {
                return false;
            }
            _ => {}
        }
    }
    pointer_only_returned(func, value)
}

fn pointer_only_returned(func: &Function, value: ValueId) -> bool {
    func.blocks.iter().all(|block| {
        block.terminator.as_ref().is_none_or(|term| {
            matches!(term, Terminator::Return { .. })
                || !term
                    .operands()
                    .iter()
                    .any(|&operand| pointer_alias_root(func, operand) == value)
        })
    })
}

/// Whether `value` is the result of this block's final call, proved canonical for `ty`.
fn returns_proved_call(
    func: &Function,
    block: BlockId,
    ty: &AbiParamType,
    value: ValueId,
    canonical_calls: &FxHashSet<(FunctionId, AbiParamType)>,
) -> bool {
    let Value::Inst(inst) = func.value(value) else { return false };
    let InstKind::ICall { function: Callee::Function(function), .. } = func.inst(*inst).kind else {
        return false;
    };
    func.blocks[block].instructions.last() == Some(inst)
        && canonical_calls.contains(&(function, ty.clone()))
}

fn is_canonical_return_scalar(
    func: &Function,
    ty: ValueLayout,
    value: ValueId,
    source: ReturnValueSource,
) -> bool {
    let Some(expected) = return_cleanup_mask(ty, source) else {
        if let ValueLayout::Int(size) = ty
            && size.bits() < 256
            && let Value::Inst(inst) = func.value(value)
            && let InstKind::SignExtend(byte, input) = func.inst(*inst).kind
        {
            return func.value_u64(byte) == Some(u64::from(size.bits() / 8 - 1))
                && is_canonical_return_scalar(func, ty, input, source);
        }
        return matches!(ty, ValueLayout::Int(size) if size.bits() >= 256);
    };
    let bits = super::egraph::max_bits_with_args(func, value, 8, &|_| 256);
    if (U256::MAX >> (256 - bits)) & !expected == U256::ZERO {
        return true;
    }
    if ty == ValueLayout::Function
        && source == ReturnValueSource::Memory
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::Shl(shift, _) = func.inst(*inst).kind
        && func.value_u64(shift) == Some(64)
    {
        return true;
    }
    // Shifting by the padding width clears every bit outside the fixed-bytes
    // result, even when the input has dirty upper bits.
    if let ValueLayout::FixedBytes(size) = ty
        && size.bytes() < 32
        && let Value::Inst(inst) = func.value(value)
        && let InstKind::Shl(shift, _) = func.inst(*inst).kind
        && func.value_u64(shift) == Some((32 - u64::from(size.bytes())) * 8)
    {
        return true;
    }
    if source == ReturnValueSource::Scalar
        && let Value::Inst(inst) = func.value(value)
        && matches!(func.inst(*inst).kind, InstKind::LoadImmutable(_))
        && (!matches!(ty, ValueLayout::Bool | ValueLayout::Address)
            || func.value_ty(value) == Some(ty.mir_type()))
    {
        return true;
    }
    // An element of an array parameter whose words element cleanup proved
    // narrower than the mask is canonical without one.
    if let Value::Inst(inst) = func.value(value)
        && let InstKind::Load(address, PtrLocation::Memory) = func.inst(*inst).kind
        && let Some(object) = array_word_object(func, address)
        && let Value::Arg(index) = func.value(object)
        && func
            .attributes
            .array_element_bits
            .get(index)
            .is_some_and(|&bits| bits <= expected.bit_len() as u32)
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

fn return_cleanup_mask(ty: ValueLayout, source: ReturnValueSource) -> Option<U256> {
    let validator = if source == ReturnValueSource::Memory {
        AbiWordValidator::from_layout(ty)
    } else {
        AbiWordValidator::from_return_layout(ty)
    };
    validator.and_then(AbiWordValidator::canonical_mask)
}

/// Instructions after the allocation in its block execute whenever the object is created.
fn allocation_initializers(func: &Function, alloc: InstId) -> Option<&[InstId]> {
    func.blocks.iter().find_map(|block| {
        block
            .instructions
            .iter()
            .position(|&inst| inst == alloc)
            .map(|index| &block.instructions[index + 1..])
    })
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
    if !matches!(func.inst(*alloc).kind, InstKind::Alloc { .. }) {
        return false;
    }

    let Some(initializers) = allocation_initializers(func, *alloc) else { return false };
    let Some(size) = u64::try_from(fields.len()).ok().and_then(|len| len.checked_mul(32)) else {
        return false;
    };
    let mut stores = FxHashMap::default();
    let mut zeroed = false;
    for inst in func.instructions() {
        if inst == *alloc || lossless_pointer_cast(func, inst).is_some() {
            continue;
        }
        match &func.inst(inst).kind {
            InstKind::MemoryZero(base, length) if pointer_alias_root(func, *base) == object => {
                if !initializers.contains(&inst)
                    || func.value_u64(*length).is_none_or(|length| length < size)
                {
                    return false;
                }
                zeroed = true;
            }
            _ if let Some((root, field, value)) = fixed_word_store(func, inst)
                && root == object =>
            {
                if !initializers.contains(&inst) || stores.insert(field, value).is_some() {
                    return false;
                }
            }
            InstKind::Store(base, stored, PtrLocation::Memory)
                if pointer_alias_root(func, *stored) == object
                    && container_keeps_pointer_private(func, *base) => {}
            _ if func
                .inst(inst)
                .operands()
                .iter()
                .any(|&value| pointer_alias_root(func, value) == object) =>
            {
                return false;
            }
            _ => {}
        }
    }
    if !pointer_only_returned(func, object) {
        return false;
    }
    fields.iter().enumerate().all(|(index, ty)| {
        if let Some(&value) = stores.get(&(index as u64)) {
            is_canonical_return_value_inner(
                func,
                ty,
                value,
                input_params,
                ReturnValueSource::Memory,
                visiting,
                calls,
            )
        } else {
            zeroed && (ty.is_scalar_word() || !ty.needs_return_cleanup())
        }
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
    if is_canonical_return_array_param(func, element, object, calls) {
        return true;
    }
    let Value::Inst(alloc) = func.value(object) else { return false };
    if func.alloc_size(object).and_then(EvmMemoryLayout::word_slot) != Some(len) {
        return false;
    }
    if !pointer_only_returned(func, object) {
        return false;
    }
    let Some(initializers) = allocation_initializers(func, *alloc) else { return false };
    let mut stores = FxHashMap::default();
    for inst in func.instructions() {
        if let Some((root, index, value)) = fixed_word_store(func, inst)
            && root == object
        {
            if !initializers.contains(&inst) || stores.insert(index, value).is_some() {
                return false;
            }
        } else if let InstKind::Store(base, stored, PtrLocation::Memory) = func.inst(inst).kind
            && pointer_alias_root(func, stored) == object
            && container_keeps_pointer_private(func, base)
        {
        } else if inst != *alloc
            && lossless_pointer_cast(func, inst).is_none()
            && func
                .inst(inst)
                .operands()
                .iter()
                .any(|&value| pointer_alias_root(func, value) == object)
        {
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

/// Whether `object` is the array this wrapper decoded for one of its own parameters and
/// element cleanup proved every word in it fits the ABI element type. Decoding validates
/// each element, and the proof covers everything the function and its callees store, so
/// the array still holds validated words where it is returned.
fn is_canonical_decoded_array(
    func: &Function,
    ty: &AbiParamType,
    value: ValueId,
    decoded_params: &[Option<ValueId>],
) -> bool {
    let (AbiParamType::DynamicArray(element) | AbiParamType::FixedArray { element, .. }) = ty
    else {
        return false;
    };
    let Some(bits) = abi_element_bits(element) else { return false };
    decoded_params.iter().position(|decoded| *decoded == Some(value)).is_some_and(|index| {
        func.attributes
            .array_element_bits
            .get(&ArgIdx::new(index))
            .is_some_and(|&proved| proved <= bits)
    })
}

/// Drops the per-element cleanup from a memory word array's encoding, leaving one payload
/// copy.
fn strip_array_element_cleanup(ty: &mut AbiType) {
    if let AbiType::DynamicArray { element, location: PtrLocation::Memory }
    | AbiType::FixedArray { element, .. } = ty
        && let AbiType::Word(validator @ Some(_)) = &mut **element
    {
        *validator = None;
    }
}

/// Whether `object` is an array parameter whose words element cleanup proved
/// narrow enough for the ABI element type. The proof covers everything this
/// function and its callees store into the array, so an array it returns
/// unchanged needs no per-element cleanup.
fn is_canonical_return_array_param(
    func: &Function,
    element: &AbiParamType,
    object: ValueId,
    calls: &CanonicalCallProof<'_>,
) -> bool {
    let Some(bits) = abi_element_bits(element) else { return false };
    if returns_proved_array(func, element, object) {
        return true;
    }
    // An array a call returned and nothing has written since.
    if let Value::Inst(inst) = func.value(object)
        && let InstKind::ICall { function: Callee::Function(function), .. } = func.inst(*inst).kind
        && calls
            .module
            .is_some_and(|module| module.function(function).return_components().len() == 1)
        && is_unmodified_call_result(func, object, *inst)
        && calls.module.is_some_and(|module| {
            module
                .function(function)
                .attributes
                .array_return_element_bits
                .is_some_and(|proved| proved <= bits)
        })
    {
        return true;
    }
    let Value::Arg(index) = *func.value(object) else { return false };
    func.attributes.array_element_bits.get(&index).is_some_and(|&proved| proved <= bits)
}

/// Whether `object` is the array this function returns, whether it built it or received it,
/// and element cleanup proved its words narrow enough for the ABI element type.
fn returns_proved_array(func: &Function, element: &AbiParamType, object: ValueId) -> bool {
    abi_element_bits(element).is_some_and(|bits| {
        func.attributes.array_return_element_bits.is_some_and(|proved| proved <= bits)
    }) && func.blocks.iter().any(|block| {
        matches!(&block.terminator, Some(Terminator::Return { values })
            if values.first() == Some(&object))
    })
}

/// The width the ABI validates for a scalar array element, when it validates one.
fn abi_element_bits(element: &AbiParamType) -> Option<u32> {
    let AbiParamType::Scalar(ty) = element else { return None };
    let Some(AbiWordValidator::Unsigned(bits)) = AbiWordValidator::from_layout(*ty) else {
        return None;
    };
    Some(u32::from(bits))
}

fn canonicalize_return_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    value: ValueId,
    input_params: Option<&AbiParamLayout>,
    source: ReturnValueSource,
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
                AbiWordValidator::from_layout(*ty)
            } else {
                AbiWordValidator::from_return_layout(*ty)
            };
            validator.map_or(value, |validator| validator.cleanup(builder, value))
        }
        AbiParamType::Bytes => value,
        AbiParamType::Tuple(fields) => {
            let output =
                builder.alloc_word_struct(fields.len() as u64, AllocationSemantics::INTERNAL);
            for (index, field_ty) in fields.iter().enumerate() {
                let field_value = builder.memory_load_field(value, index as u64);
                let field_value = canonicalize_return_value(
                    builder,
                    field_ty,
                    field_value,
                    input_params,
                    ReturnValueSource::Memory,
                );
                builder.memory_store_field(output, index as u64, field_value);
            }
            output
        }
        AbiParamType::FixedArray { element, len } => {
            let (output, layout) = builder.alloc_word_array(*len, AllocationSemantics::INTERNAL);
            let length = builder.imm(*len);
            builder.counted_loop(length, |builder, index| {
                let element_value = builder.memory_load_element(value, layout, index);
                let element_value = canonicalize_return_value(
                    builder,
                    element,
                    element_value,
                    input_params,
                    ReturnValueSource::Memory,
                );
                builder.memory_store_element(output, layout, index, element_value);
            });
            output
        }
        AbiParamType::DynamicArray(element) => {
            let length = builder.memory_len(value);
            let (output, layout) =
                builder.alloc_dynamic_word_array(length, AllocationSemantics::INTERNAL);

            builder.counted_loop(length, |builder, index| {
                let element_value = builder.memory_load_element(value, layout, index);
                let element_value = canonicalize_return_value(
                    builder,
                    element,
                    element_value,
                    input_params,
                    ReturnValueSource::Memory,
                );
                builder.memory_store_element(output, layout, index, element_value);
            });
            output
        }
    }
}

/// Whether encoding a memory value of shape `ty` with `abi` cleans every word that
/// [`AbiParamType::needs_return_cleanup`] requires. Source layouts always carry the
/// validators; a hand-written layout may not, and then needs a cleaned copy.
fn encoder_cleans_return(abi: &AbiType, ty: &AbiParamType) -> bool {
    if !ty.needs_return_cleanup() {
        return true;
    }
    match (abi, ty) {
        (AbiType::Word(Some(validator)), _) => ty.word_validator() == Some(*validator),
        (AbiType::Function, AbiParamType::Scalar(ValueLayout::Function)) => true,
        (AbiType::Tuple(fields), AbiParamType::Tuple(tys)) => {
            fields.len() == tys.len()
                && fields.iter().zip(tys).all(|(abi, ty)| encoder_cleans_return(abi, ty))
        }
        (
            AbiType::FixedArray { element, len, .. },
            AbiParamType::FixedArray { element: ty, len: ty_len },
        ) => len == ty_len && encoder_cleans_return(element, ty),
        (
            AbiType::DynamicArray { element, location: PtrLocation::Memory },
            AbiParamType::DynamicArray(ty),
        ) => encoder_cleans_return(element, ty),
        _ => false,
    }
}

fn direct_calldata_copy_source(
    func: &Function,
    return_block: BlockId,
    object: ValueId,
) -> Option<ValueId> {
    if func.value_ty(object) != Some(MirType::MEMPTR) {
        return None;
    }
    let Value::Inst(defining_inst) = func.value(object) else { return None };
    if !matches!(func.inst(*defining_inst).kind, InstKind::Alloc { .. }) {
        return None;
    }

    let mut source = None;
    let mut lengths = Vec::new();
    for (block_id, block) in func.blocks.iter_enumerated() {
        for &inst in &block.instructions {
            if !func
                .inst(inst)
                .operands()
                .iter()
                .any(|&value| func.object_address(value).is_some_and(|(base, _)| base == object))
                || func.is_object_derivation(inst)
            {
                continue;
            }
            if let Some((base, length)) = func.memory_length_store(inst)
                && base == object
            {
                lengths.push(length);
                continue;
            }
            match func.inst(inst).kind {
                InstKind::Copy(destination, data, len, PtrLocation::Calldata)
                    if func.object_address(destination)
                        == Some((object, EvmMemoryLayout::DYNAMIC_HEADER_SIZE))
                        && let Some(slice) = calldata_slice_copy(func, data, len)
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
    // The object's only length is the copied slice's, so it still holds exactly those bytes.
    let source = source?;
    let [length] = lengths[..] else { return None };
    let Value::Inst(length) = func.value(length) else { return None };
    (func.inst(*length).kind.slice_projection() == Some((source, MirType::SLICE_LEN_FIELD)))
        .then_some(source)
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
        if !matches!(abi_type, AbiType::Bytes(PtrLocation::Memory)) {
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
            matches!(func.inst(inst).kind, InstKind::Copy(destination, ..)
                if func.object_address(destination).is_some_and(|(object, _)| replacements.contains_key(&object)))
        })
        .collect::<Vec<_>>();
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !copy_insts.contains(inst));
    }
    (replacements, calldata_indices)
}

fn static_bytes_return(func: &Function) -> Option<StaticBytesReturn> {
    if !func.params.is_empty()
        || func.return_components() != [MirType::MEMPTR]
        || func.abi_params.as_ref().is_none_or(|layout| !layout.types.is_empty())
        || func
            .abi_returns
            .as_ref()
            .is_none_or(|layout| layout.types.as_ref() != [AbiType::Bytes(PtrLocation::Memory)])
        || func.blocks.len() != 1
    {
        return None;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let Some(Terminator::Return { values }) = &block.terminator else { return None };
    let [object] = values.as_slice() else { return None };
    if let [encode] = block.instructions.as_slice()
        && let InstKind::AbiEncode { mode: AbiEncodeMode::Bytes, selector: None, args, layout } =
            &func.inst(*encode).kind
        && func.inst_result_value(*encode) == Some(*object)
        && args.len() == layout.types.len()
        && layout.types.iter().all(|ty| matches!(ty, AbiType::Word(None)))
    {
        let (word, tail) = match args.as_ref() {
            [] => (U256::ZERO, None),
            [word] => (func.value_u256(*word)?, None),
            [word, tail] => (func.value_u256(*word)?, Some(func.value_u256(*tail)?)),
            _ => return None,
        };
        return Some(StaticBytesReturn { len: args.len() as u64 * 32, word, tail });
    }
    let [alloc, initialization @ ..] = block.instructions.as_slice() else { return None };
    if !matches!(func.value(*object), Value::Inst(inst) if inst == alloc) {
        return None;
    }
    let InstKind::Alloc { size, semantics } = func.inst(*alloc).kind else { return None };
    if semantics != AllocationSemantics::INTERNAL {
        return None;
    }
    // Payload words by offset, after the length store.
    let mut len = None;
    let mut words = [None; 2];
    for &inst in initialization {
        if func.is_object_derivation(inst) {
            continue;
        }
        let InstKind::Store(address, stored, PtrLocation::Memory) = func.inst(inst).kind else {
            return None;
        };
        let (base, offset) = func.object_address(address)?;
        if base != *object {
            return None;
        }
        let slot = match offset {
            0 => &mut len,
            32 => &mut words[0],
            64 => &mut words[1],
            _ => return None,
        };
        if slot.replace(stored).is_some() {
            return None;
        }
    }
    let (len, [Some(value), tail]) = (len?, words) else { return None };
    let len = func.value_u64(len)?;
    if !(1..=64).contains(&len)
        || (len > 32) != tail.is_some()
        || func.value_u64(size) != Some(32 + len.next_multiple_of(32))
    {
        return None;
    }
    let mut word = func.value_u256(value)?;
    let mut tail = if let Some(value) = tail { Some(func.value_u256(value)?) } else { None };
    let trailing_bits = usize::try_from((len.next_multiple_of(32) - len) * 8).ok()?;
    let last = tail.as_mut().unwrap_or(&mut word);
    *last = *last >> trailing_bits << trailing_bits;
    Some(StaticBytesReturn { len, word, tail })
}

fn encode_static_bytes_return(
    func: &mut Function,
    return_block: BlockId,
    value: StaticBytesReturn,
) {
    let word_size = EvmMemoryLayout::WORD_SIZE;
    let return_size = word_size * 2 + value.len.next_multiple_of(word_size);
    func.blocks[return_block].instructions.clear();
    func.blocks[return_block].terminator = None;
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(return_block);
    builder.inherit_terminator_debug_context(return_block);
    // mstore(0, 32)
    // mstore(32, len)
    // if length != 0 { mstore(64, word) }
    // if tail { mstore(96, tail) }
    // returndata(0, 64 + rounded_length)
    let offset = builder.imm(EvmMemoryLayout::SCRATCH_SLOT);
    let data_offset = builder.imm(word_size);
    let len_offset = builder.imm(EvmMemoryLayout::SCRATCH_SLOT + word_size);
    let len = builder.imm(value.len);
    let size = builder.imm(return_size);
    builder.store(offset, data_offset);
    builder.store(len_offset, len);
    if value.len != 0 {
        // mstore(64, word)
        let word_offset = builder.imm(EvmMemoryLayout::SCRATCH_SLOT + word_size * 2);
        let word = builder.imm(value.word);
        builder.store(word_offset, word);
    }
    if let Some(tail) = value.tail {
        // mstore(96, tail)
        let tail_offset = builder.imm(EvmMemoryLayout::SCRATCH_SLOT + word_size * 3);
        let tail = builder.imm(tail);
        builder.store(tail_offset, tail);
    }
    builder.ret_data(offset, size);
}

/// Returns whether a static return tuple is encoded in the words below the heap start,
/// overwriting the free memory pointer and zero slot. `return` ends execution, so nothing reads
/// them again. Only stack scalars qualify: encoding a memory aggregate loads its fields while
/// storing earlier ones, and assembly can point such an aggregate into scratch.
fn returns_in_scratch(layout: &AbiLayout) -> bool {
    layout.head_size() <= EvmMemoryLayout::HEAP_START
        && layout.types.iter().all(|ty| matches!(ty, AbiType::Word(_) | AbiType::Function))
}

fn materialize_calldata_return(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    base: ValueId,
) -> ValueId {
    if matches!(ty, AbiParamType::DynamicArray(_)) {
        // len = extract_value returned_slice, 1
        // data = extract_value returned_slice, 0
        // object = copy_calldata_value(data, len)
        let len = builder.slice_len(base);
        let data = builder.slice_ptr(base);
        return copy_calldata_value(builder, ty, data, Some(len));
    }
    // base = extract_value returned_slice, 0
    // check_range(base, head_size)
    // object = copy_calldata_value(base)
    let input_end = builder.calldatasize();
    let base = builder.slice_ptr(base);
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
    builder.switch_to_block(current);
    copy_calldata_value(builder, ty, base, None)
}

/// Copies a calldata value to memory with the checks solc applies when it re-encodes calldata
/// rather than those of the ABI decoder: nested offsets go through
/// [`FunctionBuilder::check_calldata_tail_offset`], which accepts negative offsets that wrap, an
/// inner length above `2**64 - 1` reverts before anything is allocated, and narrow words are
/// validated. `position` is the value's head, or the data address of a dynamically sized value
/// whose `length` the caller resolved.
fn copy_calldata_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    position: ValueId,
    length: Option<ValueId>,
) -> ValueId {
    match ty {
        AbiParamType::Bytes => {
            // object = bytes(length)
            // copy(position, object.data, length)
            let length = length.expect("bytes have a length");
            let object =
                builder.alloc_bytes_object(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
            let source = builder.make_slice(position, length, PtrLocation::Calldata);
            builder.memory_copy_from_slice(object, source);
            object
        }
        AbiParamType::DynamicArray(element) => {
            // object = word_array(length)
            let length = length.expect("dynamic arrays have a length");
            let (object, layout) = builder
                .alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
            if LowerAbiCx::is_full_word_scalar(element) {
                // copy(position, object.data, length * 32)
                let word = builder.imm(EvmMemoryLayout::WORD_SIZE);
                let size = builder.mul(length, word);
                let source = builder.make_slice(position, size, PtrLocation::Calldata);
                builder.memory_copy_from_slice(object, source);
                return object;
            }
            // for i in 0..length { object[i] = copy_calldata_member(position + i * head_size) }
            LowerAbiCx::decode_word_array_elements(
                builder,
                element,
                object,
                layout,
                length,
                position,
                |builder, element, head, current| {
                    let value = copy_calldata_member(builder, element, position, head);
                    *current = builder.current_block();
                    value
                },
            );
            object
        }
        AbiParamType::FixedArray { .. } | AbiParamType::Tuple(_) if ty.is_dynamic() => {
            // for each member { object.member = copy_calldata_member(position + member_offset) }
            LowerAbiCx::decode_aggregate(builder, ty, position, |builder, member, head, current| {
                let value = copy_calldata_member(builder, member, position, head);
                *current = builder.current_block();
                value
            })
        }
        _ => {
            // value = decode_static(position)
            let mut current = builder.current_block();
            let value = LowerAbiCx::decode_static_argument(
                builder,
                ty,
                position,
                &mut current,
                PtrLocation::Calldata,
            );
            builder.switch_to_block(current);
            value
        }
    }
}

/// Copies the member of an aggregate starting at `base` whose head is at `head`.
///
/// A dynamically encoded member is resolved with the ABI encoder's `calldata_access` checks.
fn copy_calldata_member(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiParamType,
    base: ValueId,
    head: ValueId,
) -> ValueId {
    if !ty.is_dynamic() {
        return copy_calldata_value(builder, ty, head, None);
    }
    // offset = calldataload(head)
    // check_calldata_tail_offset(base, offset)
    // position = base + offset
    let tail = CalldataTail::Decode;
    let offset = builder.load(head, PtrLocation::Calldata);
    // A fresh `calldatasize` per member keeps fewer values live across copy loops.
    let end = builder.calldatasize();
    builder.check_calldata_tail_offset(tail, base, offset, end, ty.data_head_size());
    let position = builder.add(base, offset);
    let Some(stride) = ty.calldata_stride() else {
        return copy_calldata_value(builder, ty, position, None);
    };
    // length = calldataload(position)
    // data = calldata_tail_data(position + 32, length, stride)
    let length = builder.load(position, PtrLocation::Calldata);
    let data = builder.calldata_tail_data(tail, position, 32, length, end, stride);
    copy_calldata_value(builder, ty, data, Some(length))
}
