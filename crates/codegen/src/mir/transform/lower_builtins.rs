//! Expand semantic builtins after optimization and before revert outlining and ABI lowering.
//!
//! The frontend evaluates arguments and retains typed builtin identities. This pass materializes
//! the precompile input/output buffers and target-version-specific call sequence. Allocation and
//! memory operations remain semantic for the later layout and placement passes. Packed encoding
//! emits array loops and overflow checks, repairing successor phi labels when it splits blocks.
//! Revert outlining can then share builtin failure payloads. Precompile calls
//! retain their returndata and memory observations even when their scalar result is unused.
//! Payable sends and transfers expand to stipend-limited calls; transfers branch to a semantic
//! returndata revert on failure, leaving payload expansion to revert lowering. Low-level address
//! calls expose their bytes input only here and retain explicit gas/value options. Return-data
//! capture allocates and copies after the call, before any later call can replace the data.

use crate::mir::{
    AddressCallKind, AllocationSemantics, ConcatPart, FunctionBuilder, InstKind, MemoryObjectKind,
    MemoryObjectLayout, MirType, Module, PanicCode, SliceLocation, ValueId,
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;

pub(crate) struct LowerBuiltins;

impl MirPass for LowerBuiltins {
    fn name(&self) -> &'static str {
        "lower-builtins"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> solar_interface::Result<bool> {
        let mut needs_clear = false;
        let mut needs_bytes = false;
        for func in &module.functions {
            for id in func.instructions() {
                match func.inst(id).kind {
                    InstKind::StorageBytesStore(..) | InstKind::StorageBytesStoreLiteral { .. } => {
                        needs_clear = true
                    }
                    InstKind::StorageArrayLoad {
                        element: MirType::MemoryObject(MemoryObjectKind::Bytes),
                        ..
                    } => needs_bytes = true,
                    _ => {}
                }
            }
        }
        // fn clear_storage_words(slot, first, end) { clear_storage_words slot, first, end; ret }
        let clear_helper =
            needs_clear.then(|| super::lower_storage_bytes::add_clear_helper(module));
        // fn load_storage_bytes(slot) { object = load_storage_bytes slot; ret object }
        let bytes_helper = needs_bytes.then(|| super::lower_storage_bytes::add_load_helper(module));
        Ok(run_function_pass(module, analyses, |func, _| {
            if !func.instructions().any(|id| is_builtin(&func.inst(id).kind)) {
                return false;
            }
            let mut replacements = FxHashMap::default();
            for block in func.blocks.indices() {
                if !func.blocks[block]
                    .instructions
                    .iter()
                    .any(|&id| is_builtin(&func.inst(id).kind))
                {
                    continue;
                }
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let (terminator, metadata) = func.blocks[block].take_terminator();
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                for id in instructions {
                    if !is_builtin(&builder.func().inst(id).kind) {
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(id);
                        continue;
                    }
                    let inst = builder.func().inst(id).clone();
                    builder.set_debug_context(&inst.metadata);
                    match inst.kind {
                        InstKind::Transfer(address, amount) => {
                            // success = send(address, amount)
                            // if !success { revert_returndata }
                            let success = lower_send(&mut builder, address, amount);
                            let revert = builder.create_block();
                            let continuation = builder.create_block();
                            builder.branch(success, continuation, revert);
                            builder.switch_to_block(revert);
                            builder.revert_returndata();
                            builder.switch_to_block(continuation);
                            continue;
                        }
                        InstKind::ValidateStorageBytes(header) => {
                            // validate_storage_bytes(header) -> encoding predicate; panic if
                            // invalid
                            super::lower_storage_bytes::validate(&mut builder, header);
                            continue;
                        }
                        InstKind::StorageBytesStore(slot, object) => {
                            // validate header; clear old tail; write header and data
                            super::lower_storage_bytes::store(
                                &mut builder,
                                slot,
                                object,
                                clear_helper.expect("storage store requires a clear helper"),
                            );
                            continue;
                        }
                        InstKind::StorageBytesStoreLiteral { slot, bytes } => {
                            // validate header; clear old tail; store literal header and data
                            super::lower_storage_bytes::store_literal(
                                &mut builder,
                                slot,
                                &bytes,
                                clear_helper
                                    .expect("literal storage store requires a clear helper"),
                            );
                            continue;
                        }
                        InstKind::StorageClearWords(slot, first, end) => {
                            // for index in first..end { sstore(storage_array_data_slot(slot) +
                            // index, 0) }
                            super::lower_storage_bytes::clear_words(&mut builder, slot, first, end);
                            continue;
                        }
                        _ => {}
                    }
                    // builtin(args) -> buffer setup; copies or precompile call; result
                    let result = match inst.kind {
                        InstKind::AbiEncodePacked { parts, hash } => {
                            super::lower_packed::lower_packed(&mut builder, parts, hash)
                        }
                        InstKind::CheckedAddMod(a, b, modulus)
                        | InstKind::CheckedMulMod(a, b, modulus) => {
                            // panic_if_zero modulus, division_by_zero
                            // result = addmod/mulmod(a, b, modulus)
                            builder.panic_if_zero(modulus, PanicCode::DivisionByZero);
                            if matches!(inst.kind, InstKind::CheckedAddMod(..)) {
                                builder.addmod(a, b, modulus)
                            } else {
                                builder.mulmod(a, b, modulus)
                            }
                        }
                        InstKind::StorageArrayLoad { slot, element, enum_variants } => {
                            super::lower_storage_arrays::load(
                                &mut builder,
                                slot,
                                element,
                                enum_variants,
                                bytes_helper,
                            )
                        }
                        InstKind::StorageBytesLoad(slot) => {
                            super::lower_storage_bytes::load(&mut builder, slot)
                        }
                        InstKind::AddressCall { kind, address, input, gas, value } => {
                            lower_address_call(
                                &mut builder,
                                gcx.sess.opts.evm_version,
                                kind,
                                address,
                                input,
                                gas,
                                value,
                            )
                        }
                        InstKind::ReturndataBytes => {
                            lower_returndata(&mut builder, gcx.sess.opts.evm_version)
                        }
                        InstKind::Send(address, amount) => {
                            lower_send(&mut builder, address, amount)
                        }
                        InstKind::Erc7201(input) => lower_erc7201(&mut builder, input),
                        InstKind::Concat(parts) => lower_concat(&mut builder, parts),
                        InstKind::Sha256(input) => {
                            lower_hash(&mut builder, gcx.sess.opts.evm_version, input, false)
                        }
                        InstKind::Ripemd160(input) => {
                            lower_hash(&mut builder, gcx.sess.opts.evm_version, input, true)
                        }
                        InstKind::EcRecover(hash, v, r, s) => {
                            lower_ecrecover(&mut builder, gcx.sess.opts.evm_version, hash, v, r, s)
                        }
                        _ => unreachable!("builtin checked above"),
                    };
                    let old =
                        builder.func().inst_result_value(id).expect("builtin must produce a value");
                    replacements.insert(old, result);
                }
                // continuation: remaining instructions; original terminator
                let end = builder.current_block();
                if let Some(terminator) = terminator {
                    builder.func_mut().blocks[end].set_terminator(terminator, metadata);
                }
                if end != block {
                    super::utils::redirect_successor_predecessors(builder.func_mut(), block, end);
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        }))
    }
}

fn is_builtin(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::ValidateStorageBytes(..)
            | InstKind::StorageBytesLoad(..)
            | InstKind::StorageArrayLoad { .. }
            | InstKind::StorageBytesStore(..)
            | InstKind::StorageBytesStoreLiteral { .. }
            | InstKind::StorageClearWords(..)
            | InstKind::Erc7201(..)
            | InstKind::CheckedAddMod(..)
            | InstKind::CheckedMulMod(..)
            | InstKind::AbiEncodePacked { .. }
            | InstKind::Concat(..)
            | InstKind::Sha256(..)
            | InstKind::Ripemd160(..)
            | InstKind::EcRecover(..)
            | InstKind::Send(..)
            | InstKind::Transfer(..)
            | InstKind::AddressCall { .. }
            | InstKind::ReturndataBytes
    )
}

fn lower_address_call(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    kind: AddressCallKind,
    address: ValueId,
    input: ValueId,
    gas: Option<ValueId>,
    value: Option<ValueId>,
) -> ValueId {
    // offset = memory_object_data input
    // size = memory_object_len input
    // gas = explicit_gas | gas() - pre_tangerine_reserve
    // success = call/staticcall/delegatecall(gas, address, value?, offset, size, 0, 0)
    let offset = builder.memory_object_data(input, MemoryObjectKind::Bytes);
    let size = builder.memory_object_len(input, MemoryObjectKind::Bytes);
    let zero = builder.imm(0);
    // A bare call has no code guard. Like solc, reserve possible account creation even for
    // delegatecall on pre-EIP-150 targets; an explicit zero value option still reserves value gas.
    let gas = gas.unwrap_or_else(|| {
        if evm.can_overcharge_gas_for_call() {
            builder.gas()
        } else {
            crate::mir::utils::pre_tangerine_call_gas(builder, value.is_some(), true)
        }
    });
    match kind {
        AddressCallKind::Call => {
            builder.call(gas, address, value.unwrap_or(zero), offset, size, zero, zero)
        }
        AddressCallKind::Static => builder.staticcall(gas, address, offset, size, zero, zero),
        AddressCallKind::Delegate => builder.delegatecall(gas, address, offset, size, zero, zero),
    }
}

fn lower_returndata(builder: &mut FunctionBuilder<'_>, evm: EvmVersion) -> ValueId {
    // length = supports_returndata ? returndatasize : 0
    // object = bytes(length)
    // copy(returndata(0, length), object.data)
    let length = if evm.supports_returndata() { builder.returndatasize() } else { builder.imm(0) };
    let object = builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
    let zero = builder.imm(0);
    let source = builder.make_slice(zero, length, SliceLocation::Returndata);
    builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
    object
}

fn lower_send(builder: &mut FunctionBuilder<'_>, address: ValueId, amount: ValueId) -> ValueId {
    // gas = amount == 0 ? 2300 : 0
    // success = call(gas, address, amount, 0, 0, 0, 0)
    let zero = builder.imm(0);
    let stipend = builder.imm(2300);
    let amount_is_zero = builder.iszero(amount);
    let gas = builder.select(amount_is_zero, stipend, zero);
    builder.call(gas, address, amount, zero, zero, zero, zero)
}

fn lower_hash(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    input: ValueId,
    ripemd: bool,
) -> ValueId {
    // input_ptr = memory_object_data input
    // input_len = memory_object_len input
    // output = bytes(32)
    // precompile_call(sha256 ? 2 : 3, input, output)
    // result = mload(output.data)
    let input_ptr = builder.memory_object_data(input, MemoryObjectKind::Bytes);
    let input_len = builder.memory_object_len(input, MemoryObjectKind::Bytes);
    let (output_ptr, output_len) = alloc_output(builder);
    let address = builder.imm(if ripemd { 3 } else { 2 });
    let output_size = builder.imm(32);
    precompile_call(builder, evm, address, input_ptr, input_len, output_ptr, output_size);
    let zero = builder.imm(0);
    let output = builder.make_slice(output_ptr, output_len, SliceLocation::Memory);
    let value = builder.memory_slice_load_word(output, zero);
    if ripemd {
        // result = result << 96
        let scale = builder.imm(1_u128 << 96);
        builder.mul(scale, value)
    } else {
        value
    }
}

fn lower_ecrecover(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    hash: ValueId,
    v: ValueId,
    r: ValueId,
    s: ValueId,
) -> ValueId {
    // input = bytes(160)
    // store(input, hash, 0)
    // store(input, v, 32)
    // store(input, r, 64)
    // store(input, s, 96)
    // precompile_call(1, input.data, 128, output.data, 32)
    // result = load(output, 0)
    let size = builder.imm(192);
    let input =
        builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::SOLIDITY_ZEROED);
    let length = builder.imm(160);
    builder.set_memory_object_len(input, length, MemoryObjectKind::Bytes);
    let pointer = builder.memory_object_data(input, MemoryObjectKind::Bytes);
    for (offset, value) in [(0, hash), (32, v), (64, r), (96, s)] {
        let offset = builder.imm(offset);
        builder.memory_object_store_word(input, offset, value);
    }
    let (output, output_len) = alloc_output(builder);
    let address = builder.imm(1);
    let input_size = builder.imm(128);
    let output_size = builder.imm(32);
    precompile_call(builder, evm, address, pointer, input_size, output, output_size);
    let slice = builder.make_slice(output, output_len, SliceLocation::Memory);
    let zero = builder.imm(0);
    builder.memory_slice_load_word(slice, zero)
}

fn alloc_output(builder: &mut FunctionBuilder<'_>) -> (ValueId, ValueId) {
    // output = alloc bytes(64), zeroed
    // memory_object_len output = 32
    // pointer = memory_object_data output
    let size = builder.imm(64);
    let output =
        builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::SOLIDITY_ZEROED);
    let length = builder.imm(32);
    builder.set_memory_object_len(output, length, MemoryObjectKind::Bytes);
    let pointer = builder.memory_object_data(output, MemoryObjectKind::Bytes);
    (pointer, length)
}

fn precompile_call(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    address: ValueId,
    input: ValueId,
    input_size: ValueId,
    output: ValueId,
    output_size: ValueId,
) {
    let gas = crate::mir::utils::precompile_gas(builder, evm);
    if evm.has_static_call() {
        // staticcall(precompile_gas, address, input, output)
        builder.staticcall(gas, address, input, input_size, output, output_size);
    } else {
        // call(precompile_gas, address, 0, input, output)
        let zero = builder.imm(0);
        builder.call(gas, address, zero, input, input_size, output, output_size);
    }
}

fn lower_concat(builder: &mut FunctionBuilder<'_>, parts: Vec<ConcatPart>) -> ValueId {
    // total = sum(part lengths)
    // output = alloc_bytes(padded_size(total))
    // memory_object_len output = total
    let mut total = builder.imm(0);
    let lengths = parts
        .iter()
        .map(|part| {
            let length = match *part {
                ConcatPart::Bytes(value) => {
                    builder.memory_object_len(value, MemoryObjectKind::Bytes)
                }
                ConcatPart::Fixed { size, .. } => builder.imm(size.bytes()),
            };
            total = builder.add(total, length);
            length
        })
        .collect::<Vec<_>>();
    let size = builder.padded_size(total);
    let output = builder.alloc_object(
        size,
        MemoryObjectLayout::Bytes,
        AllocationSemantics::SOLIDITY_UNINITIALIZED,
    );
    builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);
    let mut offset = builder.imm(0);
    for (part, length) in parts.into_iter().zip(lengths) {
        match part {
            ConcatPart::Bytes(value) => {
                // source = memory_slice(memory_object_data(value), length)
                // copy(output, offset, source)
                let pointer = builder.memory_object_data(value, MemoryObjectKind::Bytes);
                let source = builder.make_slice(pointer, length, SliceLocation::Memory);
                builder.memory_object_copy_from_slice_at(
                    output,
                    MemoryObjectKind::Bytes,
                    offset,
                    source,
                );
            }
            ConcatPart::Fixed { value, .. } => {
                // store_word(output, offset, value)
                builder.memory_object_store_word(output, offset, value);
            }
        }
        // offset += length
        offset = builder.add(offset, length);
    }
    output
}

fn lower_erc7201(builder: &mut FunctionBuilder<'_>, input: ValueId) -> ValueId {
    // inner = keccak256_bytes(input) - 1
    // object = bytes(32)
    // store_word(object, 0, inner)
    // slot = keccak256(object.data, 32) & ~0xff
    let hash = builder.keccak256_bytes(input);
    let one = builder.imm(1);
    let inner = builder.sub(hash, one);
    let length = builder.imm(32);
    let object = builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
    let zero = builder.imm(0);
    builder.memory_object_store_word(object, zero, inner);
    let data = builder.memory_object_data(object, MemoryObjectKind::Bytes);
    let outer = builder.keccak256(data, length);
    let mask = builder.imm(!U256::from(0xff));
    builder.and(outer, mask)
}
