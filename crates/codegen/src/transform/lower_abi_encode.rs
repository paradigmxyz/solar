//! Lower semantic ABI encoding operations to memory and slice operations.

use crate::{
    mir::{
        AbiLayout, AbiType, BlockId, Function, FunctionBuilder, InstKind, MemoryObjectKind, Module,
        SliceLocation, Terminator, Value, ValueId,
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

    let inst_results = func.inst_results();
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
                replacements.insert(inst_results[&inst], replacement);
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

fn resolve(mut value: ValueId, replacements: &FxHashMap<ValueId, ValueId>) -> ValueId {
    while let Some(&replacement) = replacements.get(&value) {
        value = replacement;
    }
    value
}

fn move_terminator(
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
        let allocation_size = builder.imm_u64(aligned_size);
        let buf = builder.alloc(allocation_size, crate::mir::AllocationSemantics::INTERNAL);
        if let Some(selector) = selector {
            builder.mstore(buf, selector);
        }
        let dest = offset_ptr(builder, buf, selector_size);
        encode_tuple(builder, args, &layout.types, dest, AbiScratch { base: None, depth: 0 });
        let total_size = builder.imm_u64(total_size);
        return builder.make_slice(buf, total_size, SliceLocation::Memory);
    }

    let scratch_words = layout.scratch_words();
    let scratch_base = if scratch_words == 0 {
        None
    } else {
        let size = builder.imm_u64(scratch_words * 32);
        Some(builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL))
    };

    let buf = builder.fmp();
    if let Some(selector) = selector {
        builder.mstore(buf, selector);
    }
    let dest = offset_ptr(builder, buf, selector_size);
    let size = encode_tuple(
        builder,
        args,
        &layout.types,
        dest,
        AbiScratch { base: scratch_base, depth: 0 },
    );
    let selector_size = builder.imm_u64(selector_size);
    let total = builder.add(size, selector_size);
    let thirty_one = builder.imm_u64(31);
    let rounded = builder.add(total, thirty_one);
    let mask = builder.not(thirty_one);
    let aligned = builder.and(rounded, mask);
    let allocated = builder.alloc(aligned, crate::mir::AllocationSemantics::INTERNAL);
    builder.make_slice(allocated, total, SliceLocation::Memory)
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
            if matches!(element.as_ref(), AbiType::Word) =>
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
        AbiType::Word => unreachable!("word ABI values are static"),
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

fn offset_ptr(builder: &mut FunctionBuilder<'_>, base: ValueId, offset: u64) -> ValueId {
    if offset == 0 {
        base
    } else if matches!(
        builder.func().value(base),
        Value::Immediate(immediate) if immediate.as_u256().is_some_and(|value| value.is_zero())
    ) {
        builder.imm_u64(offset)
    } else {
        let offset = builder.imm_u64(offset);
        builder.add(base, offset)
    }
}
