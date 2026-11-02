//! Expand semantic builtins after optimization and before revert outlining and ABI lowering.
//!
//! The frontend evaluates arguments and retains typed builtin identities. This pass materializes
//! the precompile input/output buffers and target-version-specific call sequence. Allocation and
//! memory operations remain semantic for the later layout and placement passes. Packed encoding
//! emits array loops and overflow checks, repairing successor phi labels when it splits blocks.
//! Revert outlining can then share builtin failure payloads. Precompile calls
//! retain their returndata and memory observations even when their scalar result is unused.
//! As in solc, a failed precompile call forwards its return data, and the 32-byte result lands in
//! scratch memory instead of an allocation.
//! Payable sends and transfers expand to stipend-limited calls; transfers branch to a semantic
//! returndata revert on failure, leaving payload expansion to revert lowering. Low-level address
//! calls expose their bytes input only here and retain explicit gas/value options. Return-data
//! capture allocates and copies after the call, before any later call can replace the data.

use crate::mir::{
    AddressCallKind, AllocationSemantics, Builtin, Callee, FunctionBuilder, InstKind,
    MemoryObjectKind, Module, PanicCode, PhaseCategory, PtrLocation, ValueId,
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass},
    transform::utils::expand_block,
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
    ) -> bool {
        let mut needs_clear = false;
        let mut needs_bytes = false;
        let mut literal_counts = FxHashMap::default();
        for func in &module.functions {
            for id in func.instructions() {
                match &func.inst(id).kind {
                    InstKind::StorageBytesStore(..) => needs_clear = true,
                    InstKind::StorageBytesStoreLiteral { bytes, .. } => {
                        needs_clear = true;
                        if bytes.len() < 32 {
                            *literal_counts.entry(bytes.clone()).or_insert(0) += 1;
                        }
                    }
                    InstKind::StorageArrayLoad {
                        element: crate::mir::ValueLayout::MemoryObject(MemoryObjectKind::Bytes),
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
        let mut literals = literal_counts
            .into_iter()
            .filter_map(|(bytes, count)| (count >= 2).then_some(bytes))
            .collect::<Vec<_>>();
        literals.sort_unstable();
        let literal_helpers = literals
            .into_iter()
            .map(|bytes| {
                // fn store_literal(slot, header) { validate; clear old tail; sstore literal; ret }
                let helper = super::lower_storage_bytes::add_literal_helper(
                    module,
                    &bytes,
                    clear_helper.expect("literal storage store requires a clear helper"),
                );
                (bytes, helper)
            })
            .collect::<FxHashMap<_, _>>();
        let evm = gcx.sess.opts.evm_version;
        run_function_pass(module, analyses, |func, _| {
            if !func.instructions().any(|id| is_builtin(&func.inst(id).kind)) {
                return false;
            }
            let mut replacements = FxHashMap::default();
            for block in func.blocks.indices() {
                let mut builder = FunctionBuilder::new(func);
                expand_block(&mut builder, block, |builder, id| {
                    if !is_builtin(&builder.func().inst(id).kind) {
                        return false;
                    }
                    let inst = builder.func().inst(id).clone();
                    builder.set_debug_context(&inst.metadata);
                    match inst.kind {
                        InstKind::ICall { function: Callee::Builtin(Builtin::Transfer), args } => {
                            let [address, amount] = builtin_args(&args);
                            // success = send(address, amount)
                            let success = lower_send(builder, address, amount);
                            builder.revert_returndata_unless(success);
                            return true;
                        }
                        InstKind::ValidateStorageBytes(header) => {
                            // validate_storage_bytes(header) -> encoding predicate; panic if
                            // invalid
                            super::lower_storage_bytes::validate(builder, header);
                            return true;
                        }
                        InstKind::StorageBytesStore(slot, object) => {
                            // validate header; clear old tail; write header and data
                            super::lower_storage_bytes::store(
                                builder,
                                slot,
                                object,
                                clear_helper.expect("storage store requires a clear helper"),
                            );
                            return true;
                        }
                        InstKind::StorageBytesStoreLiteral { slot, bytes } => {
                            // header = sload(slot)
                            let header = builder.sload(slot);
                            if let Some(&helper) = literal_helpers.get(&bytes) {
                                // icall store_literal, slot, header
                                builder.icall_void(helper, vec![slot, header]);
                                return true;
                            }
                            // validate header; clear old tail; store literal header and data
                            super::lower_storage_bytes::store_literal(
                                builder,
                                slot,
                                header,
                                &bytes,
                                clear_helper
                                    .expect("literal storage store requires a clear helper"),
                            );
                            return true;
                        }
                        InstKind::StorageClearWords(slot, first, end) => {
                            // for index in first..end { sstore(storage_array_data_slot(slot) +
                            // index, 0) }
                            super::lower_storage_bytes::clear_words(builder, slot, first, end);
                            return true;
                        }
                        _ => {}
                    }
                    // builtin(args) -> buffer setup; copies or precompile call; result
                    let result = match inst.kind {
                        InstKind::AbiEncodePacked { parts, hash } => {
                            super::lower_packed::lower_packed(builder, parts, hash)
                        }
                        InstKind::ICall {
                            function:
                                Callee::Builtin(
                                    builtin @ (Builtin::CheckedAddMod | Builtin::CheckedMulMod),
                                ),
                            args,
                        } => {
                            let [a, b, modulus] = builtin_args(&args);
                            // panic_if_zero modulus, division_by_zero
                            // result = addmod/mulmod(a, b, modulus)
                            builder.panic_if_zero(modulus, PanicCode::DivisionByZero);
                            if matches!(builtin, Builtin::CheckedAddMod) {
                                builder.addmod(a, b, modulus)
                            } else {
                                builder.mulmod(a, b, modulus)
                            }
                        }
                        InstKind::StorageArrayLoad { slot, element, enum_variants } => {
                            super::lower_storage_arrays::load(
                                builder,
                                slot,
                                element,
                                enum_variants,
                                bytes_helper,
                            )
                        }
                        InstKind::StorageBytesLoad(slot) => {
                            super::lower_storage_bytes::load(builder, slot)
                        }
                        InstKind::AddressCall { kind, address, input, gas, value } => {
                            lower_address_call(builder, evm, kind, address, input, gas, value)
                        }
                        InstKind::ICall {
                            function: Callee::Builtin(Builtin::ReturndataBytes),
                            ..
                        } => lower_returndata(builder, evm),
                        InstKind::ICall { function: Callee::Builtin(Builtin::Send), args } => {
                            let [address, amount] = builtin_args(&args);
                            lower_send(builder, address, amount)
                        }
                        InstKind::ICall { function: Callee::Builtin(Builtin::Erc7201), args } => {
                            let [input] = builtin_args(&args);
                            lower_erc7201(builder, input)
                        }
                        InstKind::ICall {
                            function:
                                Callee::Builtin(builtin @ (Builtin::Sha256 | Builtin::Ripemd160)),
                            args,
                        } => {
                            let [input] = builtin_args(&args);
                            lower_hash(builder, evm, input, builtin == Builtin::Ripemd160)
                        }
                        InstKind::ICall { function: Callee::Builtin(Builtin::EcRecover), args } => {
                            let [hash, v, r, s] = builtin_args(&args);
                            lower_ecrecover(builder, evm, hash, v, r, s)
                        }
                        _ => unreachable!("builtin checked above"),
                    };
                    let old =
                        builder.func().inst_result_value(id).expect("builtin must produce a value");
                    // result = cast lowered builtin to its declared result type
                    let result =
                        builder.cast(result, builder.func().value_ty(old).expect("typed builtin"));
                    replacements.insert(old, result);
                    true
                });
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn builtin_args<const N: usize>(args: &[ValueId]) -> [ValueId; N] {
    args.try_into().expect("validated builtin arguments")
}

fn is_builtin(kind: &InstKind) -> bool {
    kind.op_def().phase_category == Some(PhaseCategory::Builtin)
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
    // offset = add (ptrtoint input), 32
    // size = mload input
    // gas = explicit_gas | gas() - pre_tangerine_reserve
    // success = call/staticcall/delegatecall(gas, address, value?, offset, size, 0, 0)
    let offset = builder.memory_data(input);
    let size = builder.memory_len(input);
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
    let source = builder.make_slice(zero, length, PtrLocation::Returndata);
    builder.memory_copy_from_slice(object, source);
    object
}

fn lower_send(builder: &mut FunctionBuilder<'_>, address: ValueId, amount: ValueId) -> ValueId {
    // gas = amount == 0 ? 2300 : 0
    // success = call(gas, address, amount, 0, 0, 0, 0)
    let zero = builder.imm(0);
    let stipend = builder.imm(2300);
    let amount_is_zero = builder.eq_zero(amount);
    let gas = builder.select(amount_is_zero, stipend, zero);
    builder.call(gas, address, amount, zero, zero, zero, zero)
}

fn lower_hash(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    input: ValueId,
    ripemd: bool,
) -> ValueId {
    // NOTE: A successful call always writes the whole digest, so it can land in scratch memory.
    // input_ptr = add (ptrtoint input), 32
    // input_len = mload input
    // success = precompile_call(sha256 ? 2 : 3, input_ptr, input_len, scratch, 32)
    // branch success, done, failed
    // failed: revert_returndata
    // done: result = load(scratch)
    let input_ptr = builder.memory_data(input);
    let input_len = builder.memory_len(input);
    let address = builder.imm(if ripemd { 3 } else { 2 });
    let scratch = builder.imm(EvmMemoryLayout::SCRATCH_SLOT);
    let output_size = builder.imm(32);
    let success =
        precompile_call(builder, evm, address, input_ptr, input_len, scratch, output_size);
    builder.revert_returndata_unless(success);
    let value = builder.load(scratch, PtrLocation::Memory);
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
    // NOTE: The precompile consumes its input synchronously, so staging at the free-memory
    // pointer does not reserve it. A rejected signature returns no data; clear the output first.
    // input = fmp
    // store(input, hash)
    // store(input + 32, v)
    // store(input + 64, r)
    // store(input + 96, s)
    // store(scratch, 0)
    // success = precompile_call(1, input, 128, scratch, 32)
    // branch success, done, failed
    // failed: revert_returndata
    // done: result = load(scratch)
    let input = builder.fmp();
    let input = builder.cast_word(input);
    for (offset, value) in [(0, hash), (32, v), (64, r), (96, s)] {
        let address = builder.add_offset(input, offset);
        let value = builder.cast_word(value);
        builder.store(address, value);
    }
    let scratch = builder.imm(EvmMemoryLayout::SCRATCH_SLOT);
    let zero = builder.imm(0);
    builder.store(scratch, zero);
    let address = builder.imm(1);
    let input_size = builder.imm(128);
    let output_size = builder.imm(32);
    let success = precompile_call(builder, evm, address, input, input_size, scratch, output_size);
    builder.revert_returndata_unless(success);
    builder.load(scratch, PtrLocation::Memory)
}

fn precompile_call(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    address: ValueId,
    input: ValueId,
    input_size: ValueId,
    output: ValueId,
    output_size: ValueId,
) -> ValueId {
    let gas = crate::mir::utils::precompile_gas(builder, evm);
    if evm.has_static_call() {
        // staticcall(precompile_gas, address, input, output)
        builder.staticcall(gas, address, input, input_size, output, output_size)
    } else {
        // call(precompile_gas, address, 0, input, output)
        let zero = builder.imm(0);
        builder.call(gas, address, zero, input, input_size, output, output_size)
    }
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
    builder.memory_store_word(object, zero, inner);
    let data = builder.memory_data(object);
    let outer = builder.keccak256(data, length);
    let mask = builder.imm(!U256::from(0xff));
    builder.and(outer, mask)
}
