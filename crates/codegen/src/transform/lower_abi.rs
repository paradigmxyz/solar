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
//! Together with [`super::lower_dispatch::LowerDispatch`], which routes a selector switch
//! to these argument-free wrappers, this materializes the ABI boundary before
//! EVM codegen. Both passes must complete before the backend runs.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiParamLayout, BlockId, Function, FunctionBuilder, FunctionId, InstKind, MangledSymbol,
        MirPhase, MirType, Module, Terminator, ValueId,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_interface::{Span, Symbol};

#[derive(Clone, Copy)]
enum AbiWordValidator {
    Mask(U256),
    SignExtend(u64),
    Bool,
    EnumRange(u64),
}

impl AbiWordValidator {
    fn from_mir_type(ty: MirType) -> Option<Self> {
        Some(match ty {
            MirType::UInt(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::Mask(U256::MAX >> (256 - usize::from(bits)))
            }
            MirType::Int(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::SignExtend(u64::from(bits / 8) - 1)
            }
            MirType::Address => Self::Mask(U256::MAX >> 96),
            MirType::FixedBytes(size) => {
                let bytes = size.bytes();
                if bytes >= 32 {
                    return None;
                }
                Self::Mask(U256::MAX << (256 - 8 * usize::from(bytes)))
            }
            MirType::Bool => Self::Bool,
            _ => return None,
        })
    }

    fn condition(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Mask(mask) => {
                let mask = builder.imm_u256(mask);
                let canonical = builder.and(word, mask);
                builder.eq(word, canonical)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm_u64(byte_index);
                let canonical = builder.signextend(byte_index, word);
                builder.eq(word, canonical)
            }
            Self::Bool => {
                let zero = builder.iszero(word);
                let canonical = builder.iszero(zero);
                builder.eq(word, canonical)
            }
            Self::EnumRange(variants) => {
                let variants = builder.imm_u64(variants);
                builder.lt(word, variants)
            }
        }
    }
}

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
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        LowerAbiCx::default().run(module)
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
}

#[derive(Debug, Default)]
struct LowerAbiCx {
    stats: LowerAbiStats,
}

impl LowerAbiCx {
    fn run(&mut self, module: &mut Module) -> bool {
        self.stats = LowerAbiStats::default();

        // Idempotent: only `built`/`optimized` modules have an implicit ABI
        // boundary to materialize.
        if module.phase >= MirPhase::Abi {
            return false;
        }

        let mut targets = Vec::new();
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) {
                targets.push(id);
                self.stats.skipped_returns += usize::from(!can_encode_live_returns(func));
            }
            for inst_id in func.instructions() {
                if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
                    internally_called.insert(function);
                }
            }
        }

        // All-or-nothing: `abi` means *every* bodied external function is a
        // wrapper. If any return lacks the semantic layout required to encode
        // it, leave the module untouched instead of advancing to a phase the
        // content does not satisfy.
        if self.stats.skipped_returns != 0 {
            return false;
        }
        if targets.is_empty() {
            let has_selectorless_entry = module.functions.iter().any(|func| {
                func.attributes.is_constructor
                    || func.attributes.is_receive
                    || func.attributes.is_fallback
            });
            if !has_selectorless_entry {
                return false;
            }
            module.advance_phase(MirPhase::Abi);
            return true;
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
        let lazy_args = module.function(wrapper_id).abi_args_lazy;
        let abi_params = module.function(wrapper_id).abi_params.clone();
        // The copy must precede wrapper mutation and callvalue injection so
        // internal callers keep the original function semantics.
        let body_id = needs_body.then(|| {
            let mut body = module.function(wrapper_id).clone();
            body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
            body.name_span = Span::DUMMY;
            body.selector = None;
            body.abi_returns = None;
            body.abi_params = None;
            body.abi_args_lazy = false;
            body.attributes.visibility = solar_sema::hir::Visibility::Internal;
            module.add_function(body)
        });

        if lazy_args || abi_params.is_some() {
            Self::inject_abi_prologue(
                module.function_mut(wrapper_id),
                abi_params.as_ref(),
                lazy_args,
            );
        }
        self.stats.encoded_returns += encode_live_returns(module.function_mut(wrapper_id));

        // The wrapper takes no MIR arguments; its `Arg` values now read the
        // calldata head words directly.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.params.clear();
        wrapper.returns.clear();
        wrapper.abi_returns = None;
        wrapper.abi_params = None;
        wrapper.abi_args_lazy = false;
        body_id
    }

    /// Materializes deferred ABI arguments and their validation checks.
    fn inject_abi_prologue(
        func: &mut Function,
        abi_params: Option<&crate::mir::AbiParamLayout>,
        lazy_args: bool,
    ) {
        let arg_types: Vec<_> = func.params.iter().copied().collect();
        if arg_types.is_empty() && abi_params.is_none() {
            return;
        }

        let old_entry = BlockId::ENTRY;
        let arg_uses = func.arg_uses();
        let mut replacements = FxHashMap::default();
        if let Some(layout) = abi_params {
            let mut head_offset = 0_u64;
            let mut logical_physical = Vec::with_capacity(layout.types.len());
            for ty in &layout.types {
                logical_physical.push(
                    (!Self::is_supported_aggregate(ty))
                        .then(|| crate::mir::ArgIdx::new((head_offset / 32) as usize)),
                );
                head_offset += ty.head_size();
            }
            let mut params = IndexVec::with_capacity((head_offset / 32) as usize);
            for _ in 0..head_offset / 32 {
                params.push(MirType::uint256());
            }
            func.set_params(params);
            let values = logical_physical
                .into_iter()
                .map(|index| index.map(|index| func.alloc_arg(index)))
                .collect::<Vec<_>>();
            for (logical, value) in values.iter().enumerate() {
                if let Some(value) = value {
                    for &use_value in
                        arg_uses.get(crate::mir::ArgIdx::new(logical)).into_iter().flatten()
                    {
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
            let calldata_size = builder.calldatasize();
            let head_size =
                abi_params.map_or((arg_types.len() as u64) * 32, AbiParamLayout::head_size);
            let required = builder.imm_u64(4 + head_size);
            let short = builder.lt(calldata_size, required);
            let next = builder.create_block();
            builder.branch(short, revert, next);
            current = next;

            if lazy_args {
                let mut head_offset = 0;
                for (index, &ty) in arg_types.iter().enumerate() {
                    let validator = abi_params
                        .and_then(|layout| layout.types.get(index))
                        .and_then(|layout_ty| match layout_ty {
                            crate::mir::AbiParamType::Enum { variants, .. } => {
                                Some(AbiWordValidator::EnumRange(*variants))
                            }
                            crate::mir::AbiParamType::Scalar(_) => {
                                AbiWordValidator::from_mir_type(ty)
                            }
                            _ => None,
                        })
                        .or_else(|| AbiWordValidator::from_mir_type(ty));
                    head_offset += abi_params
                        .and_then(|layout| layout.types.get(index))
                        .map_or(32, crate::mir::AbiParamType::head_size);
                    let Some(validator) = validator else { continue };
                    builder.switch_to_block(current);
                    // `Value::Arg` values carry the canonicality invariant that this
                    // guard establishes. Read the raw calldata word here so an
                    // optimizer cannot fold the check away before it runs.
                    let offset = builder.imm_u64(4 + head_offset - 32);
                    let word = builder.calldataload(offset);
                    let valid = validator.condition(&mut builder, word);
                    let next = builder.create_block();
                    builder.branch(valid, next, revert);
                    current = next;
                }
            }

            if let Some(layout) = abi_params {
                let mut head_offset = 0;
                for (index, ty) in layout.types.iter().enumerate() {
                    let arg_index = crate::mir::ArgIdx::new(index);
                    let uses = arg_uses.get(arg_index).map_or(&[][..], Vec::as_slice);
                    if uses.is_empty() {
                        head_offset += ty.head_size();
                        continue;
                    }
                    if !Self::is_supported_aggregate(ty) {
                        head_offset += ty.head_size();
                        continue;
                    }
                    let Some(arg_type) = arg_types.get(index).copied() else {
                        head_offset += ty.head_size();
                        continue;
                    };
                    if !matches!(arg_type, MirType::MemoryObject(_) | MirType::Slice(_)) {
                        head_offset += ty.head_size();
                        continue;
                    }
                    let head = builder.imm_u64(4 + head_offset);
                    let tuple_base = builder.imm_u64(4);
                    let value = Self::decode_aggregate_argument(
                        &mut builder,
                        ty,
                        arg_type,
                        head,
                        tuple_base,
                        &mut current,
                    );
                    for &use_value in uses {
                        replacements.insert(use_value, value);
                    }
                    head_offset += ty.head_size();
                }
            }

            builder.switch_to_block(current);
            builder.jump(old_entry);

            builder.switch_to_block(revert);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);

            guard
        };
        func.replace_uses_canonicalized(&replacements);
        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
    }

    fn decode_aggregate_argument(
        builder: &mut FunctionBuilder<'_>,
        ty: &crate::mir::AbiParamType,
        arg_type: MirType,
        head: ValueId,
        tuple_base: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        let base = if ty.is_dynamic() {
            Self::guard_calldata_range(builder, head, 32, current);
            let offset = builder.calldataload(head);
            Self::guard_calldata_offset(builder, tuple_base, offset, current)
        } else {
            head
        };
        match ty {
            crate::mir::AbiParamType::Scalar(scalar) => {
                Self::decode_scalar(builder, *scalar, base, current)
            }
            crate::mir::AbiParamType::Enum { variants, .. } => {
                Self::decode_enum(builder, *variants, base, current)
            }
            crate::mir::AbiParamType::FixedArray { element, len }
                if Self::is_supported_tuple_field(element) =>
            {
                let head_size = len.saturating_mul(element.head_size());
                Self::guard_calldata_range(builder, base, head_size, current);
                let size = builder.imm_u64(len.saturating_mul(32));
                let ptr = builder.alloc_object(
                    size,
                    crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                let mut offset = 0;
                for index in 0..*len {
                    let offset_value = builder.imm_u64(offset);
                    let word_pos = builder.add(base, offset_value);
                    let value = match element.as_ref() {
                        crate::mir::AbiParamType::Scalar(scalar) => {
                            Self::decode_scalar(builder, *scalar, word_pos, current)
                        }
                        crate::mir::AbiParamType::Enum { variants, .. } => {
                            Self::decode_enum(builder, *variants, word_pos, current)
                        }
                        element => Self::decode_aggregate_argument(
                            builder,
                            element,
                            element.mir_type(),
                            word_pos,
                            base,
                            current,
                        ),
                    };
                    let elem_index = builder.imm_u64(index);
                    let slot = builder.memory_object_element_addr(
                        ptr,
                        crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                        elem_index,
                    );
                    builder.mstore(slot, value);
                    offset += element.head_size();
                }
                ptr
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if matches!(element.as_ref(), crate::mir::AbiParamType::Scalar(_))
                    && matches!(arg_type, MirType::Slice(_)) =>
            {
                Self::guard_calldata_range(builder, base, 32, current);
                let len = builder.calldataload(base);
                let word = builder.imm_u64(32);
                let bytes = Self::checked_mul(builder, len, word, current);
                let data = builder.add(base, word);
                Self::guard_calldata_range_value(builder, data, bytes, current);
                builder.make_slice(data, len, crate::mir::SliceLocation::Calldata)
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if Self::is_full_word_scalar(element) =>
            {
                Self::guard_calldata_range(builder, base, 32, current);
                let len = builder.calldataload(base);
                let word = builder.imm_u64(32);
                let bytes = Self::checked_mul(builder, len, word, current);
                let total = Self::checked_add(builder, bytes, word, current);
                Self::guard_calldata_range_value(builder, base, total, current);
                let ptr = builder.alloc_object(
                    total,
                    crate::mir::MemoryObjectLayout::WORD_ARRAY,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, crate::mir::MemoryObjectKind::DynamicArray);
                let dst =
                    builder.memory_object_data(ptr, crate::mir::MemoryObjectKind::DynamicArray);
                let src = builder.add(base, word);
                builder.calldatacopy(dst, src, bytes);
                ptr
            }
            crate::mir::AbiParamType::DynamicArray(element)
                if matches!(arg_type, MirType::MemoryObject(_)) =>
            {
                Self::guard_calldata_range(builder, base, 32, current);
                let len = builder.calldataload(base);
                let word = builder.imm_u64(32);
                let element_head_size = builder.imm_u64(element.head_size());
                let head_bytes = Self::checked_mul(builder, len, element_head_size, current);
                let head = builder.add(base, word);
                Self::guard_calldata_range_value(builder, head, head_bytes, current);
                let bytes = Self::checked_mul(builder, len, word, current);
                let total = Self::checked_add(builder, bytes, word, current);
                let ptr = builder.alloc_object(
                    total,
                    crate::mir::MemoryObjectLayout::WORD_ARRAY,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, crate::mir::MemoryObjectKind::DynamicArray);
                let data_base = builder.add(base, word);
                let dest =
                    builder.memory_object_data(ptr, crate::mir::MemoryObjectKind::DynamicArray);

                // Dynamic ABI arrays use a head of one word per element. The
                // element value may itself be dynamic, so nested objects are
                // decoded recursively and stored as pointers in this array.
                // Recursive decoding can introduce arbitrary CFG edges; keep
                // loop state in a semantic object so those edges do not make
                // values live across an unbounded stack path.
                let state_layout = crate::mir::MemoryObjectLayout::structure(3);
                let state_size = builder.imm_u64(3 * 32);
                let state = builder.alloc_object(
                    state_size,
                    state_layout,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                let remaining_slot = builder.memory_object_field_addr(state, state_layout, 0);
                let source_slot = builder.memory_object_field_addr(state, state_layout, 1);
                let destination_slot = builder.memory_object_field_addr(state, state_layout, 2);
                builder.mstore(remaining_slot, len);
                builder.mstore(source_slot, data_base);
                builder.mstore(destination_slot, dest);

                let cond = builder.create_block();
                let body = builder.create_block();
                let done = builder.create_block();
                builder.jump(cond);

                builder.switch_to_block(cond);
                let remaining = builder.mload(remaining_slot);
                let zero = builder.imm_u64(0);
                let has_next = builder.gt(remaining, zero);
                builder.branch(has_next, body, done);

                builder.switch_to_block(body);
                let source = builder.mload(source_slot);
                let destination = builder.mload(destination_slot);
                let mut element_current = builder.current_block();
                let value = Self::decode_aggregate_argument(
                    builder,
                    element,
                    element.mir_type(),
                    source,
                    data_base,
                    &mut element_current,
                );
                builder.mstore(destination, value);
                let one = builder.imm_u64(1);
                let next_remaining = builder.sub(remaining, one);
                let element_head_size = builder.imm_u64(element.head_size());
                let next_source = builder.add(source, element_head_size);
                let next_destination = builder.add(destination, word);
                builder.mstore(remaining_slot, next_remaining);
                builder.mstore(source_slot, next_source);
                builder.mstore(destination_slot, next_destination);
                builder.jump(cond);

                builder.switch_to_block(done);
                *current = done;
                ptr
            }
            crate::mir::AbiParamType::Bytes if matches!(arg_type, MirType::Slice(_)) => {
                Self::guard_calldata_range(builder, base, 32, current);
                let len = builder.calldataload(base);
                let word = builder.imm_u64(32);
                let data = builder.add(base, word);
                let padding = builder.imm_u64(31);
                let rounded = Self::checked_add(builder, len, padding, current);
                let mask = builder.not(padding);
                let padded = builder.and(rounded, mask);
                Self::guard_calldata_range_value(builder, data, padded, current);
                builder.make_slice(data, len, crate::mir::SliceLocation::Calldata)
            }
            crate::mir::AbiParamType::Bytes => {
                Self::guard_calldata_range(builder, base, 32, current);
                let len = builder.calldataload(base);
                let word = builder.imm_u64(32);
                let thirty_one = builder.imm_u64(31);
                let rounded = Self::checked_add(builder, len, thirty_one, current);
                let mask = builder.not(thirty_one);
                let data_size = builder.and(rounded, mask);
                let data = builder.add(base, word);
                Self::guard_calldata_range_value(builder, data, data_size, current);
                let total = Self::checked_add(builder, data_size, word, current);
                let ptr = builder.alloc_object(
                    total,
                    crate::mir::MemoryObjectLayout::Bytes,
                    crate::mir::AllocationSemantics::INTERNAL,
                );
                builder.set_memory_object_len(ptr, len, crate::mir::MemoryObjectKind::Bytes);
                let dst = builder.memory_object_data(ptr, crate::mir::MemoryObjectKind::Bytes);
                let src = builder.add(base, word);
                builder.calldatacopy(dst, src, len);
                ptr
            }
            crate::mir::AbiParamType::Tuple(fields) if Self::is_supported_aggregate(ty) => {
                // Calldata structs with dynamic fields keep their source base
                // in one trailing word so slice expressions can recover the
                // original calldata location after the fields are copied.
                Self::guard_calldata_range(builder, base, ty.head_size(), current);
                let carries_base = fields.iter().any(crate::mir::AbiParamType::is_dynamic);
                let storage_fields = fields.len() + usize::from(carries_base);
                let size = builder.imm_u64((storage_fields as u64).saturating_mul(32));
                let layout = crate::mir::MemoryObjectLayout::structure(storage_fields as u64);
                let ptr =
                    builder.alloc_object(size, layout, crate::mir::AllocationSemantics::INTERNAL);
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
                        current,
                    );
                    let slot = builder.memory_object_field_addr(ptr, layout, index as u64);
                    builder.mstore(slot, value);
                    offset += field.head_size();
                }
                if carries_base {
                    let slot = builder.memory_object_field_addr(ptr, layout, fields.len() as u64);
                    builder.mstore(slot, base);
                }
                ptr
            }
            _ => builder.undef(arg_type),
        }
    }

    fn decode_scalar(
        builder: &mut FunctionBuilder<'_>,
        scalar: MirType,
        position: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        Self::guard_calldata_range(builder, position, 32, current);
        let value = builder.calldataload(position);
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
        value
    }

    fn decode_enum(
        builder: &mut FunctionBuilder<'_>,
        variants: u64,
        position: ValueId,
        current: &mut BlockId,
    ) -> ValueId {
        builder.switch_to_block(*current);
        Self::guard_calldata_range(builder, position, 32, current);
        let value = builder.calldataload(position);
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
        let end = builder.add(start, size);
        let overflow = builder.lt(end, start);
        let calldata_size = builder.calldatasize();
        let out_of_range = builder.gt(end, calldata_size);
        let invalid = builder.or(overflow, out_of_range);
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
        let overflow = builder.lt(target, base);
        let calldata_size = builder.calldatasize();
        let out_of_range = builder.gt(target, calldata_size);
        let minimum_offset = builder.imm_u64(32);
        let too_small = builder.lt(offset, minimum_offset);
        let invalid_range = builder.or(overflow, out_of_range);
        let invalid = builder.or(invalid_range, too_small);
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
        let quotient = builder.div(result, rhs);
        let valid = builder.eq(quotient, lhs);
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

    fn is_supported_aggregate(ty: &crate::mir::AbiParamType) -> bool {
        matches!(
            ty,
            crate::mir::AbiParamType::FixedArray { element, .. }
                if Self::is_supported_tuple_field(element)
        ) || matches!(
            ty,
            crate::mir::AbiParamType::DynamicArray(element)
                if Self::is_supported_tuple_field(element)
        ) || matches!(ty, crate::mir::AbiParamType::Bytes)
            || matches!(
                ty,
                crate::mir::AbiParamType::Tuple(fields)
                    if fields.iter().all(Self::is_supported_tuple_field)
            )
    }

    fn is_supported_tuple_field(ty: &crate::mir::AbiParamType) -> bool {
        matches!(
            ty,
            crate::mir::AbiParamType::Scalar(_)
                | crate::mir::AbiParamType::Enum { .. }
                | crate::mir::AbiParamType::Bytes
        ) || matches!(
            ty,
            crate::mir::AbiParamType::FixedArray { element, .. }
                if Self::is_supported_tuple_field(element)
        ) || matches!(
            ty,
            crate::mir::AbiParamType::DynamicArray(element)
                if Self::is_supported_tuple_field(element)
        ) || matches!(ty, crate::mir::AbiParamType::Tuple(fields) if fields.iter().all(Self::is_supported_tuple_field))
    }

    fn is_full_word_scalar(ty: &crate::mir::AbiParamType) -> bool {
        matches!(
            ty,
            crate::mir::AbiParamType::Scalar(scalar)
                if *scalar == MirType::uint256()
                    || *scalar == MirType::int256()
                    || *scalar == MirType::bytes32()
        )
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
        crate::mir::utils::remap_block_order(func, &order);
    }
}

/// An external entry with a body and a selector — the shape a wrapper is built
/// for. Receive/fallback entries have no selector and need no ABI wrapper.
fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor
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

/// Rewrites value-carrying returns into a semantic ABI encode followed by
/// `returndata(slice_ptr(encoded), slice_len(encoded))`.
fn encode_live_returns(func: &mut Function) -> usize {
    let Some(layout) = func.abi_returns.clone() else { return 0 };
    let block_ids: Vec<_> = func.blocks.indices().collect();
    let mut encoded_returns = 0;
    for block_id in block_ids {
        let Some(Terminator::Return { values }) = &func.blocks[block_id].terminator else {
            continue;
        };
        if values.is_empty() {
            continue;
        }
        let values = values.clone().into_vec().into_boxed_slice();
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block_id);
        if layout.types.iter().any(crate::mir::AbiType::is_dynamic) {
            let encoded = builder.abi_encode(layout.clone(), None, values);
            let offset = builder.slice_ptr(encoded);
            let size = builder.slice_len(encoded);
            builder.ret_data(offset, size);
        } else {
            let offset = builder.imm_u64(EvmMemoryLayout::HEAP_START);
            let size = super::lower_abi_encode::encode_tuple(
                &mut builder,
                &values,
                &layout.types,
                offset,
                super::lower_abi_encode::AbiScratch { base: None, depth: 0 },
            );
            builder.ret_data(offset, size);
        }
        encoded_returns += 1;
    }
    encoded_returns
}
