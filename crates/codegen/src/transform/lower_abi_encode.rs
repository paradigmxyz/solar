//! Lower semantic ABI encoding operations to memory and slice operations.

use crate::{
    mir::{
        AbiLayout, AbiType, BlockId, Function, FunctionBuilder, InstKind, MemoryObjectKind,
        MemoryObjectLayout, MirType, Module, SliceLocation, Terminator, Value, ValueId,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

/// Lowers `abi_encode` after the main optimization pipeline.
pub(crate) struct LowerAbiEncode;

impl MirPass for LowerAbiEncode {
    fn name(&self) -> &'static str {
        "lower-abi-encode"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function(func);
        }
        changed
    }
}

#[derive(Clone, Copy)]
struct AbiValueDest {
    head_addr: ValueId,
    tuple_base: ValueId,
    tail: ValueId,
}

fn lower_function(func: &mut Function) -> bool {
    let has_encodes =
        func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::AbiEncode { .. }));
    if !has_encodes {
        return false;
    }

    let mut replacements = FxHashMap::default();
    let blocks: Vec<_> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let original_terminator = func.blocks[block].terminator.take();
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let encode = match &builder.func().inst(inst).kind {
                InstKind::AbiEncode { selector, args, layout } => Some((
                    selector.map(|value| resolve(value, &replacements)),
                    args.iter().map(|&value| resolve(value, &replacements)).collect::<Vec<_>>(),
                    std::sync::Arc::clone(layout),
                )),
                _ => None,
            };
            if let Some((selector, args, layout)) = encode {
                let replacement = lower_encode(&mut builder, &layout, selector, &args);
                remove_literal_objects(builder.func_mut(), &args);
                let result = builder
                    .func()
                    .inst_result_value(inst)
                    .expect("ABI encode must produce a value");
                replacements.insert(result, replacement);
            } else {
                let current = builder.current_block();
                builder.func_mut().blocks[current].instructions.push(inst);
            }
        }
        move_terminator(&mut builder, block, original_terminator);
    }
    fold_slice_projections(func, &mut replacements);
    func.replace_uses_canonicalized(&replacements);
    let repaired = crate::mir::utils::repair_reachability_phis(func);
    !replacements.is_empty() || repaired
}

fn fold_slice_projections(func: &Function, replacements: &mut FxHashMap<ValueId, ValueId>) {
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        let replacement = match &func.inst(inst_id).kind {
            InstKind::SlicePtr(slice) => {
                let slice = resolve(*slice, replacements);
                let Value::Inst(make_slice) = func.value(slice) else { continue };
                let InstKind::MakeSlice { ptr, .. } = &func.inst(*make_slice).kind else {
                    continue;
                };
                Some(*ptr)
            }
            InstKind::SliceLen(slice) => {
                let slice = resolve(*slice, replacements);
                let Value::Inst(make_slice) = func.value(slice) else { continue };
                let InstKind::MakeSlice { len, .. } = &func.inst(*make_slice).kind else {
                    continue;
                };
                Some(*len)
            }
            _ => None,
        };
        if let Some(replacement) = replacement {
            replacements.insert(result, resolve(replacement, replacements));
        }
    }
}

pub(crate) fn resolve(mut value: ValueId, replacements: &FxHashMap<ValueId, ValueId>) -> ValueId {
    while let Some(&replacement) = replacements.get(&value) {
        value = replacement;
    }
    value
}

pub(crate) fn move_terminator(
    builder: &mut FunctionBuilder<'_>,
    original_block: BlockId,
    terminator: Option<Terminator>,
) {
    let final_block = builder.current_block();
    let Some(terminator) = terminator else { return };
    if final_block != original_block {
        for successor in terminator.successors() {
            let instructions = builder.func().blocks[successor].instructions.clone();
            for inst in instructions {
                if let InstKind::Phi(incoming) = &mut builder.func_mut().inst_mut(inst).kind {
                    for (predecessor, _) in incoming {
                        if *predecessor == original_block {
                            *predecessor = final_block;
                        }
                    }
                }
            }
        }
    }
    builder.func_mut().blocks[final_block].terminator = Some(terminator);
}

fn lower_encode(
    builder: &mut FunctionBuilder<'_>,
    layout: &AbiLayout,
    selector: Option<ValueId>,
    args: &[ValueId],
) -> ValueId {
    debug_assert_eq!(layout.types.len(), args.len());
    let selector_size = if selector.is_some() { 4 } else { 0 };
    if !layout.types.iter().any(AbiType::is_dynamic) {
        let total_size = selector_size + layout.head_size();
        let aligned_size = total_size.next_multiple_of(32);
        if selector.is_none() {
            // Constructor arguments do not need a raw slice, and keeping their
            // object header lets the memory lowering preserve the established
            // allocation path for creation code.
            let allocation_size = builder.imm_u64(aligned_size.saturating_add(32));
            let object = builder.alloc_object(
                allocation_size,
                MemoryObjectLayout::Bytes,
                crate::mir::AllocationSemantics::INTERNAL,
            );
            let total = builder.imm_u64(total_size);
            builder.set_memory_object_len(object, total, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(object, MemoryObjectKind::Bytes);
            encode_static_tuple(builder, args, &layout.types, data);
            return builder.make_slice(data, total, SliceLocation::Memory);
        }
        let allocation_size = builder.imm_u64(aligned_size);
        let buffer = builder.alloc_raw(allocation_size, crate::mir::AllocationSemantics::INTERNAL);
        if let Some(selector) = selector {
            builder.mstore(buffer, selector);
        }
        let dest = offset_ptr(builder, buffer, selector_size);
        encode_tuple(builder, args, &layout.types, dest);
        let total = builder.imm_u64(total_size);
        return builder.make_slice(buffer, total, SliceLocation::Memory);
    }

    // Source objects already live below the free-memory pointer. Encode into
    // the untouched range, then reserve the exact output in one pass.
    let buffer = builder.fmp();
    if let Some(selector) = selector {
        builder.mstore(buffer, selector);
    }
    let dest = offset_ptr(builder, buffer, selector_size);
    let encoded_size = encode_tuple(builder, args, &layout.types, dest);
    let selector_size = builder.imm_u64(selector_size);
    let total = builder.add(encoded_size, selector_size);
    let thirty_one = builder.imm_u64(31);
    let rounded = builder.add(total, thirty_one);
    let mask = builder.not(thirty_one);
    let aligned = builder.and(rounded, mask);
    let allocated = builder.alloc_raw(aligned, crate::mir::AllocationSemantics::INTERNAL);
    builder.make_slice(allocated, total, SliceLocation::Memory)
}

/// Encodes a statically shaped tuple into an existing physical return buffer.
///
/// Static ABI returns use the fixed return buffer owned by the ABI boundary;
/// source-level `abi.encode` uses the typed object encoder below.
pub(crate) fn encode_static_tuple(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    dest: ValueId,
) -> ValueId {
    let mut head = dest;
    for (&value, ty) in values.iter().zip(types) {
        encode_static_raw(builder, ty, value, head);
        head = offset_ptr(builder, head, ty.head_size());
    }
    builder.sub(head, dest)
}

fn encode_static_raw(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    head: ValueId,
) {
    match ty {
        AbiType::Tuple(fields) => {
            let mut field_head = head;
            for (index, field) in fields.iter().enumerate() {
                let field_value = builder.memory_object_load_field(
                    value,
                    MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                encode_static_raw(builder, field, field_value, field_head);
                field_head = offset_ptr(builder, field_head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            let mut element_head = head;
            for index in 0..*len {
                let index = builder.imm_u64(index);
                let element_value = builder.memory_object_load_element(
                    value,
                    MemoryObjectLayout::word_fixed_array(*len),
                    index,
                );
                encode_static_raw(builder, element, element_value, element_head);
                element_head = offset_ptr(builder, element_head, element.head_size());
            }
        }
        AbiType::Function => {
            let shift = builder.imm_u64(64);
            let value = builder.shl(shift, value);
            builder.mstore(head, value);
        }
        _ => builder.mstore(head, value),
    }
}

pub(crate) fn encode_tuple(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    dest: ValueId,
) -> ValueId {
    let head_size: u64 = types.iter().map(AbiType::head_size).sum();
    if !types.iter().any(AbiType::is_dynamic) {
        let mut head_offset = 0;
        for (&value, ty) in values.iter().zip(types) {
            let head = offset_ptr(builder, dest, head_offset);
            encode_static(builder, ty, value, head);
            head_offset += ty.head_size();
        }
        return builder.imm_u64(head_size);
    }

    let head_size_value = builder.imm_u64(head_size);
    let mut tail = builder.add(dest, head_size_value);
    let mut head_offset = 0;
    for (&value, ty) in values.iter().zip(types) {
        let head_addr = offset_ptr(builder, dest, head_offset);
        tail = encode_value(builder, ty, value, AbiValueDest { head_addr, tuple_base: dest, tail });
        head_offset += ty.head_size();
    }
    builder.sub(tail, dest)
}

fn encode_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: AbiValueDest,
) -> ValueId {
    if ty.is_dynamic() {
        let relative = builder.sub(dest.tail, dest.tuple_base);
        builder.mstore(dest.head_addr, relative);
        encode_dynamic_body(builder, ty, value, dest.tail)
    } else {
        encode_static(builder, ty, value, dest.head_addr);
        dest.tail
    }
}

fn encode_static(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    head_addr: ValueId,
) {
    if let Some(location @ (SliceLocation::Calldata | SliceLocation::Memory)) =
        builder.func().value_slice_location(value)
    {
        let source = builder.slice_ptr(value);
        encode_static_slice(builder, ty, source, head_addr, location);
        return;
    }
    match ty {
        AbiType::Tuple(fields) => {
            let mut field_head = head_addr;
            for (index, field) in fields.iter().enumerate() {
                let field_value = builder.memory_object_load_field(
                    value,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                encode_static(builder, field, field_value, field_head);
                field_head = offset_ptr(builder, field_head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            let mut element_head = head_addr;
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                let element_value = builder.memory_object_load_element(
                    value,
                    crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                    index_value,
                );
                encode_static(builder, element, element_value, element_head);
                element_head = offset_ptr(builder, element_head, element.head_size());
            }
        }
        AbiType::Function => {
            let shift = builder.imm_u64(64);
            let value = builder.shl(shift, value);
            builder.mstore(head_addr, value);
        }
        _ => builder.mstore(head_addr, value),
    }
}

fn encode_static_slice(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    source: ValueId,
    head_addr: ValueId,
    location: SliceLocation,
) {
    match ty {
        AbiType::Tuple(fields) => {
            let mut source_offset = 0;
            let mut head = head_addr;
            for field in fields {
                let source_word = offset_ptr(builder, source, source_offset);
                encode_static_slice_at(builder, field, source_word, head, location);
                source_offset += field.head_size();
                head = offset_ptr(builder, head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            let mut source_offset = 0;
            let mut head = head_addr;
            for _ in 0..*len {
                let source_word = offset_ptr(builder, source, source_offset);
                encode_static_slice_at(builder, element, source_word, head, location);
                source_offset += element.head_size();
                head = offset_ptr(builder, head, element.head_size());
            }
        }
        AbiType::Word | AbiType::Function => {
            encode_static_slice_at(builder, ty, source, head_addr, location);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

fn encode_static_slice_at(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    source: ValueId,
    head_addr: ValueId,
    location: SliceLocation,
) {
    match ty {
        AbiType::Tuple(_) | AbiType::FixedArray { .. } => {
            encode_static_slice(builder, ty, source, head_addr, location);
        }
        AbiType::Function | AbiType::Word => {
            let value = load_slice_word(builder, source, location);
            builder.mstore(head_addr, value);
        }
        AbiType::Bytes(_) | AbiType::DynamicArray { .. } => {
            unreachable!("dynamic ABI values are not static")
        }
    }
}

fn load_slice_word(
    builder: &mut FunctionBuilder<'_>,
    source: ValueId,
    location: SliceLocation,
) -> ValueId {
    match location {
        SliceLocation::Calldata => builder.calldataload(source),
        SliceLocation::Memory => builder.mload(source),
        SliceLocation::Returndata => unreachable!("returndata slices are not static ABI inputs"),
    }
}

fn encode_dynamic_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: ValueId,
) -> ValueId {
    match ty {
        AbiType::Bytes(location) => {
            let location = effective_slice_location(builder, value, *location);
            encode_bytes(builder, value, dest, location)
        }
        AbiType::DynamicArray { element, location }
            if matches!(element.as_ref(), AbiType::Word | AbiType::Function) =>
        {
            let location = effective_slice_location(builder, value, *location);
            encode_word_array(
                builder,
                value,
                dest,
                location,
                matches!(element.as_ref(), AbiType::Function),
            )
        }
        AbiType::DynamicArray { element, location } => {
            let location = effective_slice_location(builder, value, *location);
            match location {
                SliceLocation::Memory => encode_dynamic_array(builder, element, value, dest),
                SliceLocation::Calldata => {
                    if matches!(element.as_ref(), AbiType::Bytes(_)) {
                        encode_calldata_bytes_array(builder, element, value, dest)
                    } else {
                        unreachable!(
                            "non-word calldata arrays are materialized before ABI encoding"
                        )
                    }
                }
                SliceLocation::Returndata => {
                    unreachable!("returndata arrays are not ABI inputs")
                }
            }
        }
        AbiType::FixedArray { element, len } => {
            let mut values = Vec::with_capacity(*len as usize);
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                let element_value = builder.memory_object_load_element(
                    value,
                    crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                    index_value,
                );
                values.push(element_value);
            }
            let types = vec![element.as_ref().clone(); *len as usize];
            let size = encode_tuple(builder, &values, &types, dest);
            builder.add(dest, size)
        }
        AbiType::Tuple(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for index in 0..fields.len() {
                let field_value = builder.memory_object_load_field(
                    value,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                values.push(field_value);
            }
            let size = encode_tuple(builder, &values, fields, dest);
            builder.add(dest, size)
        }
        AbiType::Word | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

fn effective_slice_location(
    builder: &FunctionBuilder<'_>,
    value: ValueId,
    declared: SliceLocation,
) -> SliceLocation {
    if declared == SliceLocation::Memory
        && let Some(MirType::Slice(
            location @ (SliceLocation::Calldata | SliceLocation::Returndata),
        )) = builder.func().value_ty(value)
    {
        location
    } else {
        declared
    }
}

fn zero_padded_tail(builder: &mut FunctionBuilder<'_>, data: ValueId, padded: ValueId) {
    let zero_block = builder.create_block();
    let copy_block = builder.create_block();
    let empty = builder.iszero(padded);
    builder.branch(empty, copy_block, zero_block);
    builder.switch_to_block(zero_block);
    let word = builder.imm_u64(32);
    let last_offset = builder.sub(padded, word);
    let last = builder.add(data, last_offset);
    let zero = builder.imm_u64(0);
    builder.mstore(last, zero);
    builder.jump(copy_block);
    builder.switch_to_block(copy_block);
}

fn encode_dynamic_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
) -> ValueId {
    let len = builder.memory_object_len(value, MemoryObjectKind::DynamicArray);
    builder.mstore(dest, len);

    let word = builder.imm_u64(32);
    let element_area = builder.add(dest, word);
    let element_head_size = builder.imm_u64(element.head_size());
    let head_bytes = builder.mul(len, element_head_size);
    let initial_tail = builder.add(element_area, head_bytes);
    let source_cursor = builder.memory_object_data(value, MemoryObjectKind::DynamicArray);

    let preheader = builder.current_block();
    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = builder.phi(vec![(preheader, len)]);
    let current_tail = builder.phi(vec![(preheader, initial_tail)]);
    let element_head = builder.phi(vec![(preheader, element_area)]);
    let source = builder.phi(vec![(preheader, source_cursor)]);
    let zero = builder.imm_u64(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    let element_value = builder.mload(source);
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base: element_area, tail: current_tail },
    );

    let one = builder.imm_u64(1);
    let next_remaining = builder.sub(remaining, one);
    let next_source = builder.add(source, word);
    let next_head = builder.add(element_head, element_head_size);
    let backedge = builder.current_block();
    builder.jump(cond);
    builder.add_phi_incoming(remaining, backedge, next_remaining);
    builder.add_phi_incoming(current_tail, backedge, new_tail);
    builder.add_phi_incoming(element_head, backedge, next_head);
    builder.add_phi_incoming(source, backedge, next_source);

    builder.switch_to_block(done);
    current_tail
}

fn encode_calldata_bytes_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
) -> ValueId {
    let len = builder.slice_len(value);
    builder.mstore(dest, len);

    let word = builder.imm_u64(32);
    let element_area = builder.add(dest, word);
    let head_bytes = builder.mul(len, word);
    let initial_tail = builder.add(element_area, head_bytes);
    let source_base = builder.slice_ptr(value);

    let preheader = builder.current_block();
    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = builder.phi(vec![(preheader, len)]);
    let current_tail = builder.phi(vec![(preheader, initial_tail)]);
    let element_head = builder.phi(vec![(preheader, element_area)]);
    let source_head = builder.phi(vec![(preheader, source_base)]);
    let zero = builder.imm_u64(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    let offset = builder.calldataload(source_head);
    let calldata_size = builder.calldatasize();
    let available = builder.sub(calldata_size, source_base);
    let invalid_offset = builder.gt(offset, available);
    revert_if_calldata_invalid(builder, invalid_offset);
    let element_base = builder.add(source_base, offset);
    check_calldata_range(builder, element_base, word);
    let length = builder.calldataload(element_base);
    let data = builder.add(element_base, word);
    check_calldata_range(builder, data, length);
    let element_value = builder.make_slice(data, length, SliceLocation::Calldata);
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base: element_area, tail: current_tail },
    );

    let one = builder.imm_u64(1);
    let next_remaining = builder.sub(remaining, one);
    let next_source = builder.add(source_head, word);
    let next_head = builder.add(element_head, word);
    let backedge = builder.current_block();
    builder.jump(cond);
    builder.add_phi_incoming(remaining, backedge, next_remaining);
    builder.add_phi_incoming(current_tail, backedge, new_tail);
    builder.add_phi_incoming(element_head, backedge, next_head);
    builder.add_phi_incoming(source_head, backedge, next_source);

    builder.switch_to_block(done);
    current_tail
}

fn revert_if_calldata_invalid(builder: &mut FunctionBuilder<'_>, condition: ValueId) {
    let revert = builder.create_block();
    let continue_block = builder.create_block();
    builder.branch(condition, revert, continue_block);
    builder.switch_to_block(revert);
    let zero = builder.imm_u64(0);
    builder.revert(zero, zero);
    builder.switch_to_block(continue_block);
}

fn check_calldata_range(builder: &mut FunctionBuilder<'_>, start: ValueId, size: ValueId) {
    let end = builder.add(start, size);
    let overflow = builder.lt(end, start);
    let calldata_size = builder.calldatasize();
    let out_of_bounds = builder.gt(end, calldata_size);
    let invalid = builder.or(overflow, out_of_bounds);
    revert_if_calldata_invalid(builder, invalid);
}

fn encode_word_array(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
    function_elements: bool,
) -> ValueId {
    let len = match location {
        SliceLocation::Memory => builder.memory_object_len(value, MemoryObjectKind::DynamicArray),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    builder.mstore(dest, len);
    let word = builder.imm_u64(32);
    let bytes = builder.mul(len, word);
    let data_dest = builder.add(dest, word);
    let data_source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::DynamicArray),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, bytes);
    if function_elements && location == SliceLocation::Memory {
        let preheader = builder.current_block();
        let cond = builder.create_block();
        let body = builder.create_block();
        let done = builder.create_block();
        builder.jump(cond);

        builder.switch_to_block(cond);
        let zero = builder.imm_u64(0);
        let index = builder.phi(vec![(preheader, zero)]);
        let more = builder.lt(index, len);
        builder.branch(more, body, done);

        builder.switch_to_block(body);
        let offset = builder.mul(index, word);
        let source = builder.add(data_source, offset);
        let destination = builder.add(data_dest, offset);
        let value = builder.mload(source);
        let shift = builder.imm_u64(64);
        let encoded = builder.shl(shift, value);
        builder.mstore(destination, encoded);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let backedge = builder.current_block();
        builder.jump(cond);
        builder.add_phi_incoming(index, backedge, next);

        builder.switch_to_block(done);
        return tail;
    }
    copy_slice_data(builder, location, data_dest, data_source, bytes);
    tail
}

fn encode_bytes(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
) -> ValueId {
    if location == SliceLocation::Memory
        && let Some(bytes) = literal_bytes(builder.func(), value)
    {
        let length = builder.imm_u64(bytes.len() as u64);
        builder.mstore(dest, length);
        let word = builder.imm_u64(32);
        let data = builder.add(dest, word);
        for (index, chunk) in bytes.chunks(32).enumerate() {
            let mut padded = [0_u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let word = builder.imm_u256(U256::from_be_bytes(padded));
            let offset = builder.imm_u64(index as u64 * 32);
            let address = builder.add(data, offset);
            builder.mstore(address, word);
        }
        let size = builder.imm_u64(bytes.len().next_multiple_of(32) as u64);
        return builder.add(data, size);
    }

    let len = match location {
        SliceLocation::Memory => builder.memory_object_len(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    builder.mstore(dest, len);

    let word = builder.imm_u64(32);
    let thirty_one = builder.imm_u64(31);
    let mask = builder.not(thirty_one);
    let rounded = builder.add(len, thirty_one);
    let padded = builder.and(rounded, mask);
    let data_dest = builder.add(dest, word);

    zero_padded_tail(builder, data_dest, padded);
    let data_source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    let tail = builder.add(data_dest, padded);
    copy_slice_data(builder, location, data_dest, data_source, len);
    tail
}

/// Copies `size` bytes of a slice's data from its address space into memory at
/// `dest`. Memory-to-memory uses `mcopy`; calldata and returndata slices copy
/// from their own buffers with `calldatacopy`/`returndatacopy`.
fn copy_slice_data(
    builder: &mut FunctionBuilder<'_>,
    location: SliceLocation,
    dest: ValueId,
    source: ValueId,
    size: ValueId,
) {
    match location {
        SliceLocation::Memory => builder.mcopy(dest, source, size),
        SliceLocation::Calldata => builder.calldatacopy(dest, source, size),
        SliceLocation::Returndata => builder.returndatacopy(dest, source, size),
    }
}

/// Returns the bytes represented by an immutable literal object when all active
/// uses are its literal initialization operations.
fn literal_bytes(func: &Function, object: ValueId) -> Option<Vec<u8>> {
    if func.value_ty(object) != Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) {
        return None;
    }
    let Value::Inst(defining_inst) = func.value(object) else { return None };
    if !matches!(func.inst(*defining_inst).kind, InstKind::Alloc { .. }) {
        return None;
    }

    let mut length = None;
    let mut words = FxHashMap::default();
    for inst in func.instructions() {
        if inst == *defining_inst {
            continue;
        }
        let instruction = func.inst(inst);
        if !instruction.operands().contains(&object) {
            continue;
        }
        match &instruction.kind {
            InstKind::SetMemoryObjectLen(value, len, MemoryObjectKind::Bytes)
                if *value == object =>
            {
                if length.replace(func.value_u64(*len)?).is_some() {
                    return None;
                }
            }
            InstKind::MemoryObjectStoreWord { object: value, offset, value: word }
                if *value == object =>
            {
                let offset = func.value_u64(*offset)?;
                if !offset.is_multiple_of(32)
                    || words.insert(offset, func.value_u256(*word)?).is_some()
                {
                    return None;
                }
            }
            _ => return None,
        }
    }

    if func
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator.operands().contains(&object))
    {
        return None;
    }

    let length = length?;
    let word_count = length.div_ceil(32);
    if words.len() != usize::try_from(word_count).ok()? {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(length).ok()?);
    for index in 0..word_count {
        let offset = index.checked_mul(32)?;
        let word = words.remove(&offset)?.to_be_bytes::<32>();
        let remaining = length.saturating_sub(offset).min(32) as usize;
        bytes.extend_from_slice(&word[..remaining]);
    }
    Some(bytes)
}

/// Removes a literal object's initialization after its encoding has embedded
/// the bytes directly in the output. The object must have no active use other
/// than its allocation and literal stores; otherwise the object remains live.
fn remove_literal_objects(func: &mut Function, values: &[ValueId]) {
    for &object in values {
        if literal_bytes(func, object).is_none() {
            continue;
        }
        let Value::Inst(defining_inst) = func.value(object) else { continue };
        let mut removed = FxHashSet::default();
        removed.insert(*defining_inst);
        let mut valid = true;
        for inst_id in func.instructions() {
            if inst_id == *defining_inst {
                continue;
            }
            let instruction = func.inst(inst_id);
            if !instruction.operands().contains(&object) {
                continue;
            }
            match instruction.kind {
                InstKind::SetMemoryObjectLen(value, _, MemoryObjectKind::Bytes)
                    if value == object =>
                {
                    removed.insert(inst_id);
                }
                InstKind::MemoryObjectStoreWord { object: value, .. } if value == object => {
                    removed.insert(inst_id);
                }
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid
            || func
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
                .any(|terminator| terminator.operands().contains(&object))
        {
            continue;
        }
        for block in &mut func.blocks {
            block.instructions.retain(|inst| !removed.contains(inst));
        }
    }
}

fn offset_ptr(builder: &mut FunctionBuilder<'_>, base: ValueId, offset: u64) -> ValueId {
    if offset != 0 && builder.func().value_u256(base).is_some_and(|base| base.is_zero()) {
        builder.imm_u64(offset)
    } else {
        builder.add_u64_offset(base, offset)
    }
}
