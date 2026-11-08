//! Expand semantic builtins after optimization and before revert outlining and ABI lowering.
//!
//! The frontend evaluates arguments and retains typed builtin identities. This pass materializes
//! the call and copy sequences of the target version. Allocation and memory operations remain
//! semantic for the later layout and placement passes. Packed encoding emits array loops and
//! overflow checks, repairing successor phi labels when it splits blocks. Revert outlining can
//! then share builtin failure payloads.
//!
//! Every `address_call` expands through the one call emitter, `emit_call`: the low-level calls
//! with their explicit gas and value options, the stipend-limited `send` and `transfer`, and the
//! `ecrecover`, `sha256` and `ripemd160` precompiles. The operation's policy states what differs
//! between them. A precompile's 32-byte result lands in scratch memory instead of an allocation,
//! and, as in solc, a failed call forwards its return data. Return-data capture allocates and
//! copies after the call, before any later call can replace the data.

use crate::mir::{
    AddressCallKind, AllocationSemantics, Builtin, CallOutput, CallPolicy, Callee, FunctionBuilder,
    InstKind, MemoryObjectKind, MirModuleTarget, Module, PanicCode, PhaseCategory, PtrLocation,
    ValueId,
    pass::{MirPass, run_function_pass},
    transform::utils::expand_block,
    utils::emit_call,
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
        let evm = gcx.sess.opts.evm_version;
        let mut needs_clear = false;
        let mut needs_bytes = false;
        let mut literal_counts = FxHashMap::default();
        for func in &module.functions {
            for id in func.instructions() {
                match &func.inst(id).kind {
                    // Without `STATICCALL` a static call becomes a `CALL`, which only a precompile
                    // call, whose callee keeps no state, can do without changing what the
                    // optimizer assumed of the static kind.
                    InstKind::AddressCall {
                        policy:
                            CallPolicy {
                                kind: AddressCallKind::Static, output: CallOutput::None, ..
                            },
                        ..
                    } if !evm.has_static_call() => {
                        analyses.fail(
                            gcx.dcx()
                                .err("`address_staticcall` of an arbitrary callee needs `STATICCALL`")
                                .emit(),
                        );
                        return false;
                    }
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
        let target = module.target(gcx);
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
                    match inst.kind {
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
                            super::lower_packed::lower_packed(builder, parts, hash, &target)
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
                        InstKind::AddressCall { policy, address, input, gas, value } => {
                            let Some(result) = lower_address_call(
                                builder, evm, policy, address, input, gas, value,
                            ) else {
                                return true;
                            };
                            result
                        }
                        InstKind::ICall {
                            function: Callee::Builtin(Builtin::ReturndataBytes),
                            ..
                        } => lower_returndata(builder, evm),
                        InstKind::ICall { function: Callee::Builtin(Builtin::Erc7201), args } => {
                            let [input] = builtin_args(&args);
                            lower_erc7201(builder, input)
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
    policy: CallPolicy,
    address: ValueId,
    input: Option<ValueId>,
    gas: Option<ValueId>,
    value: Option<ValueId>,
) -> Option<ValueId> {
    // offset = add (ptrtoint input), 32
    // size = mload input
    // result? = emit_call(policy, address, (offset, size), gas?, value?)
    let input = input.map(|input| (builder.memory_data(input), builder.memory_len(input)));
    emit_call(builder, evm, policy, address, input, gas, value, None)
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
