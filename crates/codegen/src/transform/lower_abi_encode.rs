//! Lower semantic ABI encoding operations to memory and slice operations.

use crate::{
    mir::{
        AbiLayout, AbiType, BlockId, Function, FunctionBuilder, InstKind, MemoryObjectKind,
        MemoryObjectLayout, Module, SliceLocation, Terminator, ValueId,
    },
    pass::MirPass,
};
use solar_data_structures::map::FxHashMap;
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
pub(crate) struct AbiScratch {
    pub(crate) base: Option<ValueId>,
    pub(crate) depth: u64,
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
    func.replace_uses_canonicalized(&replacements);
    let repaired = crate::mir::utils::repair_reachability_phis(func);
    !replacements.is_empty() || repaired
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
        encode_tuple(builder, args, &layout.types, dest, AbiScratch { base: None, depth: 0 });
        let total = builder.imm_u64(total_size);
        return builder.make_slice(buffer, total, SliceLocation::Memory);
    }

    let scratch_words = layout.scratch_words();
    let scratch_base = if scratch_words == 0 {
        None
    } else {
        let size = builder.imm_u64(scratch_words * 32);
        Some(builder.alloc_raw(size, crate::mir::AllocationSemantics::INTERNAL))
    };

    let encoded_size =
        measure_tuple(builder, args, &layout.types, AbiScratch { base: scratch_base, depth: 0 });
    let selector_size_value = builder.imm_u64(selector_size);
    let total = builder.add(encoded_size, selector_size_value);
    let thirty_one = builder.imm_u64(31);
    let rounded = builder.add(total, thirty_one);
    let mask = builder.not(thirty_one);
    let aligned = builder.and(rounded, mask);
    let buffer = builder.alloc_raw(aligned, crate::mir::AllocationSemantics::INTERNAL);
    if let Some(selector) = selector {
        builder.mstore(buffer, selector);
    }
    let dest = offset_ptr(builder, buffer, selector_size);
    encode_tuple(builder, args, &layout.types, dest, AbiScratch { base: scratch_base, depth: 0 });
    builder.make_slice(buffer, total, SliceLocation::Memory)
}

fn measure_tuple(
    builder: &mut FunctionBuilder<'_>,
    values: &[ValueId],
    types: &[AbiType],
    scratch: AbiScratch,
) -> ValueId {
    let head_size = builder.imm_u64(types.iter().map(AbiType::head_size).sum());
    if !types.iter().any(AbiType::is_dynamic) {
        return head_size;
    }

    let mut size = head_size;
    for (&value, ty) in values.iter().zip(types) {
        if ty.is_dynamic() {
            let body = measure_dynamic_body(builder, ty, value, scratch);
            size = builder.add(size, body);
        }
    }
    size
}

fn measure_dynamic_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    scratch: AbiScratch,
) -> ValueId {
    match ty {
        AbiType::Bytes(location) => {
            let len = match location {
                SliceLocation::Memory => builder.memory_object_len(value, MemoryObjectKind::Bytes),
                SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
            };
            padded_bytes_size(builder, len)
        }
        AbiType::DynamicArray { element, location }
            if matches!(element.as_ref(), AbiType::Word)
                || (matches!(element.as_ref(), AbiType::Function)
                    && location != &SliceLocation::Memory) =>
        {
            let len = match location {
                SliceLocation::Memory => {
                    builder.memory_object_len(value, MemoryObjectKind::DynamicArray)
                }
                SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
            };
            let word = builder.imm_u64(32);
            let data_size = builder.mul(len, word);
            builder.add(word, data_size)
        }
        AbiType::DynamicArray { element, location: SliceLocation::Memory } => {
            measure_dynamic_array(builder, element, value, scratch)
        }
        AbiType::DynamicArray {
            location: SliceLocation::Calldata | SliceLocation::Returndata,
            ..
        } => unreachable!("non-word calldata arrays are materialized before ABI encoding"),
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
            measure_tuple(builder, &values, &types, scratch)
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
            measure_tuple(builder, &values, fields, scratch)
        }
        AbiType::Word | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

fn measure_dynamic_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    scratch: AbiScratch,
) -> ValueId {
    let scratch_base = scratch.base.expect("dynamic ABI array sizing requires scratch memory");
    let len = builder.memory_object_len(value, MemoryObjectKind::DynamicArray);
    let word = builder.imm_u64(32);
    let element_head_size = builder.imm_u64(element.head_size());
    let head_bytes = builder.mul(len, element_head_size);
    let initial_size = builder.add(word, head_bytes);
    scratch_store(builder, scratch_base, scratch.depth, 0, len);
    scratch_store(builder, scratch_base, scratch.depth, 1, initial_size);
    let zero = builder.imm_u64(0);
    scratch_store(builder, scratch_base, scratch.depth, 2, zero);

    let cond = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.jump(cond);

    builder.switch_to_block(cond);
    let remaining = scratch_load(builder, scratch_base, scratch.depth, 0);
    let zero = builder.imm_u64(0);
    let has_next = builder.gt(remaining, zero);
    builder.branch(has_next, body, done);

    builder.switch_to_block(body);
    let source_index = scratch_load(builder, scratch_base, scratch.depth, 2);
    let element_value =
        builder.memory_object_load_element(value, MemoryObjectLayout::WORD_ARRAY, source_index);
    if element.is_dynamic() {
        let size = measure_dynamic_body(
            builder,
            element,
            element_value,
            AbiScratch { base: Some(scratch_base), depth: scratch.depth + 1 },
        );
        let total = scratch_load(builder, scratch_base, scratch.depth, 1);
        let next_size = builder.add(total, size);
        scratch_store(builder, scratch_base, scratch.depth, 1, next_size);
    }
    let remaining = scratch_load(builder, scratch_base, scratch.depth, 0);
    let one = builder.imm_u64(1);
    let next_remaining = builder.sub(remaining, one);
    scratch_store(builder, scratch_base, scratch.depth, 0, next_remaining);
    let source_index = scratch_load(builder, scratch_base, scratch.depth, 2);
    let next_index = builder.add(source_index, one);
    scratch_store(builder, scratch_base, scratch.depth, 2, next_index);
    builder.jump(cond);

    builder.switch_to_block(done);
    scratch_load(builder, scratch_base, scratch.depth, 1)
}

fn padded_bytes_size(builder: &mut FunctionBuilder<'_>, len: ValueId) -> ValueId {
    let word = builder.imm_u64(32);
    let thirty_one = builder.imm_u64(31);
    let rounded = builder.add(len, thirty_one);
    let mask = builder.not(thirty_one);
    let padded = builder.and(rounded, mask);
    builder.add(word, padded)
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
    scratch: AbiScratch,
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
        tail = encode_value(
            builder,
            ty,
            value,
            AbiValueDest { head_addr, tuple_base: dest, tail },
            scratch,
        );
        head_offset += ty.head_size();
    }
    builder.sub(tail, dest)
}

fn encode_value(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: AbiValueDest,
    scratch: AbiScratch,
) -> ValueId {
    if ty.is_dynamic() {
        let relative = builder.sub(dest.tail, dest.tuple_base);
        builder.mstore(dest.head_addr, relative);
        encode_dynamic_body(builder, ty, value, dest.tail, scratch)
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
    match ty {
        AbiType::Tuple(fields) => {
            let mut field_head = head_addr;
            for (index, field) in fields.iter().enumerate() {
                let slot = builder.memory_object_field_addr(
                    value,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                let field_value = builder.mload(slot);
                encode_static(builder, field, field_value, field_head);
                field_head = offset_ptr(builder, field_head, field.head_size());
            }
        }
        AbiType::FixedArray { element, len } => {
            let mut element_head = head_addr;
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                let slot = builder.memory_object_element_addr(
                    value,
                    crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                    index_value,
                );
                let element_value = builder.mload(slot);
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

fn encode_dynamic_body(
    builder: &mut FunctionBuilder<'_>,
    ty: &AbiType,
    value: ValueId,
    dest: ValueId,
    scratch: AbiScratch,
) -> ValueId {
    match ty {
        AbiType::Bytes(location) => encode_bytes(builder, value, dest, *location),
        AbiType::DynamicArray { element, location }
            if matches!(element.as_ref(), AbiType::Word)
                || (matches!(element.as_ref(), AbiType::Function)
                    && location != &SliceLocation::Memory) =>
        {
            encode_word_array(builder, value, dest, *location)
        }
        AbiType::DynamicArray { element, location: SliceLocation::Memory } => {
            encode_dynamic_array(builder, element, value, dest, scratch)
        }
        AbiType::FixedArray { element, len } => {
            let mut values = Vec::with_capacity(*len as usize);
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                let slot = builder.memory_object_element_addr(
                    value,
                    crate::mir::MemoryObjectLayout::word_fixed_array(*len),
                    index_value,
                );
                values.push(builder.mload(slot));
            }
            let types = vec![element.as_ref().clone(); *len as usize];
            let size = encode_tuple(builder, &values, &types, dest, scratch);
            builder.add(dest, size)
        }
        AbiType::Tuple(fields) => {
            let mut values = Vec::with_capacity(fields.len());
            for index in 0..fields.len() {
                let slot = builder.memory_object_field_addr(
                    value,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                values.push(builder.mload(slot));
            }
            let size = encode_tuple(builder, &values, fields, dest, scratch);
            builder.add(dest, size)
        }
        AbiType::DynamicArray {
            location: SliceLocation::Calldata | SliceLocation::Returndata,
            ..
        } => {
            unreachable!("non-word calldata arrays are materialized before ABI encoding")
        }
        AbiType::Word | AbiType::Function => unreachable!("word ABI values are static"),
    }
}

fn encode_dynamic_array(
    builder: &mut FunctionBuilder<'_>,
    element: &AbiType,
    value: ValueId,
    dest: ValueId,
    scratch: AbiScratch,
) -> ValueId {
    let scratch_base = scratch.base.expect("dynamic ABI array encoding requires scratch memory");
    let len = builder.memory_object_len(value, MemoryObjectKind::DynamicArray);
    builder.mstore(dest, len);

    let word = builder.imm_u64(32);
    let element_area = builder.add(dest, word);
    let element_head_size = builder.imm_u64(element.head_size());
    let head_bytes = builder.mul(len, element_head_size);
    let initial_tail = builder.add(element_area, head_bytes);
    let source_cursor = builder.memory_object_data(value, MemoryObjectKind::DynamicArray);

    let remaining_slot = scratch_slot(builder, scratch_base, scratch.depth, 0);
    let tail_slot = scratch_slot(builder, scratch_base, scratch.depth, 1);
    let head_slot = scratch_slot(builder, scratch_base, scratch.depth, 2);
    let source_slot = scratch_slot(builder, scratch_base, scratch.depth, 3);
    let tuple_base_slot = scratch_slot(builder, scratch_base, scratch.depth, 4);
    builder.mstore(remaining_slot, len);
    builder.mstore(tail_slot, initial_tail);
    builder.mstore(head_slot, element_area);
    builder.mstore(source_slot, source_cursor);
    builder.mstore(tuple_base_slot, element_area);

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
    let element_value = builder.mload(source);
    let element_head = builder.mload(head_slot);
    let current_tail = builder.mload(tail_slot);
    let tuple_base = builder.mload(tuple_base_slot);
    let new_tail = encode_value(
        builder,
        element,
        element_value,
        AbiValueDest { head_addr: element_head, tuple_base, tail: current_tail },
        AbiScratch { base: Some(scratch_base), depth: scratch.depth + 1 },
    );
    builder.mstore(tail_slot, new_tail);

    let remaining = builder.mload(remaining_slot);
    let one = builder.imm_u64(1);
    let next_remaining = builder.sub(remaining, one);
    builder.mstore(remaining_slot, next_remaining);
    let source = builder.mload(source_slot);
    let next_source = builder.add(source, word);
    builder.mstore(source_slot, next_source);
    let element_head = builder.mload(head_slot);
    let next_head = builder.add(element_head, element_head_size);
    builder.mstore(head_slot, next_head);
    builder.jump(cond);

    builder.switch_to_block(done);
    builder.mload(tail_slot)
}

fn encode_word_array(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
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
    copy_slice_data(builder, location, data_dest, data_source, bytes);
    tail
}

fn encode_bytes(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    dest: ValueId,
    location: SliceLocation,
) -> ValueId {
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

    let zero_block = builder.create_block();
    let copy_block = builder.create_block();
    let empty = builder.iszero(padded);
    builder.branch(empty, copy_block, zero_block);

    builder.switch_to_block(zero_block);
    let last_offset = builder.sub(padded, word);
    let last_word = builder.add(data_dest, last_offset);
    let zero = builder.imm_u64(0);
    builder.mstore(last_word, zero);
    builder.jump(copy_block);

    builder.switch_to_block(copy_block);
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

fn scratch_slot(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    depth: u64,
    slot: u64,
) -> ValueId {
    offset_ptr(builder, base, depth * 160 + slot * 32)
}

fn scratch_store(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    depth: u64,
    slot: u64,
    value: ValueId,
) {
    let address = scratch_slot(builder, base, depth, slot);
    builder.mstore(address, value);
}

fn scratch_load(
    builder: &mut FunctionBuilder<'_>,
    base: ValueId,
    depth: u64,
    slot: u64,
) -> ValueId {
    let address = scratch_slot(builder, base, depth, slot);
    builder.mload(address)
}

fn offset_ptr(builder: &mut FunctionBuilder<'_>, base: ValueId, offset: u64) -> ValueId {
    if offset == 0 {
        base
    } else if builder.func().value_u256(base).is_some_and(|base| base.is_zero()) {
        builder.imm_u64(offset)
    } else {
        let offset = builder.imm_u64(offset);
        builder.add(base, offset)
    }
}
