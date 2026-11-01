//! Expand semantic conditional checks before revert outlining and arithmetic expansion.
//!
//! Each check splits its block at the original execution point and branches to a typed panic or
//! revert payload. Checks in one function share payload blocks, merging their debug origins.
//! Surviving instructions and the original terminator move to the continuation; successor phi
//! predecessors update locally. The session's revert-string mode selects debug payloads without
//! changing when argument evaluation or the check occurs. Require payloads read their evaluated
//! arguments only on failure; short constant strings share a module helper. New helper creation
//! invalidates module analyses along with the rewritten call edges. Optimized checks known to
//! fail after memory writes emit their payload in place, exposing those writes to dead-store
//! elimination. Other failures retain shared helpers. Instructions after an unconditional
//! failure move to an unreachable continuation.
//!
//! Revert payloads are never read after the revert, so none of them reserves memory. Panics,
//! short constant error strings, and custom errors of at most `MAX_SCRATCH_ERROR_WORDS` words
//! stage in the reserved low memory through `FunctionBuilder::revert_scratch`. Constant strings
//! longer than a word write the whole `Error(string)` payload from constants, as stores or a data
//! copy by the target cost model. Other custom errors and error strings encode at the
//! free-memory pointer. None of these advance it.

use crate::mir::{
    AbiLayout, AbiType, Builtin, Callee, ERROR_SELECTOR, EffectKind, Function, FunctionBuilder,
    FunctionId, InstKind, InstructionMetadata, MAX_SCRATCH_ERROR_WORDS, MirType, Module,
    PhaseCategory, PtrLocation, RequireKind, RevertKind, RevertPayload,
    lower::{DataDest, copy_data_to_memory},
    pass::MirPass,
    transform::utils::expand_block,
};
use alloy_primitives::U256;
use solar_interface::{Ident, sym};
use solar_sema::Gcx;
use std::sync::Arc;

pub(crate) struct LowerChecks;

impl MirPass for LowerChecks {
    fn name(&self) -> &'static str {
        "lower-checks"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut helper_context = InstructionMetadata::EMPTY;
        let mut needs_helper = false;
        for function in &module.functions {
            for id in function.instructions() {
                let inst = function.inst(id);
                if matches!(
                    &inst.kind,
                    InstKind::ICall {
                        function: Callee::Builtin(Builtin::Require(RequireKind::ShortString)),
                        ..
                    }
                ) {
                    needs_helper = true;
                    helper_context.merge_debug_context(&inst.metadata);
                }
            }
        }
        let helper = needs_helper.then(|| create_short_string_helper(module, &helper_context));
        let mut changed = false;
        // Long strings intern their data in the module while a function is rewritten.
        let mut functions = std::mem::take(&mut module.functions);
        for function in &mut functions {
            let optimized = !module.objective_of(function).is_none();
            changed |= lower_function(gcx, module, function, helper, optimized);
        }
        module.functions = functions;
        changed
    }
}

fn lower_function(
    gcx: Gcx<'_>,
    module: &mut Module,
    func: &mut Function,
    helper: Option<FunctionId>,
    optimize: bool,
) -> bool {
    if !func.instructions().any(|id| is_check(&func.inst(id).kind)) {
        return false;
    }
    let blocks = func.blocks.indices();
    let mut builder = FunctionBuilder::new(func).with_revert_strings(gcx.sess.opts.revert_strings);
    for block in blocks {
        if !builder.func().blocks[block]
            .instructions
            .iter()
            .any(|&id| is_check(&builder.func().inst(id).kind))
        {
            continue;
        }
        expand_block(&mut builder, block, |builder, id| {
            if !is_check(&builder.func().inst(id).kind) {
                return false;
            }
            let inst = builder.func().inst(id).clone();
            builder.set_debug_context(&inst.metadata);
            match inst.kind {
                InstKind::ICall {
                    function: Callee::Builtin(Builtin::Check { is_zero, failure }),
                    args,
                } => {
                    let condition = args[0];
                    if optimize
                        && builder
                            .func()
                            .value_u256(condition)
                            .is_some_and(|value| value.is_zero() == is_zero)
                        && builder.func().blocks[builder.current_block()].instructions.iter().any(
                            |&id| {
                                builder.func().inst(id).kind.effect_kind()
                                    == EffectKind::MemoryWrite
                            },
                        )
                    {
                        // revert payload; unreachable continuation
                        match failure {
                            RevertKind::Panic(code) => builder.panic(code),
                            RevertKind::Reason(reason) => builder.revert_with(reason),
                        }
                        let continuation = builder.create_block();
                        builder.switch_to_block(continuation);
                    } else {
                        // branch condition, failure, continuation
                        // failure: revert payload
                        builder.branch_to_revert(condition, is_zero, failure);
                    }
                }
                InstKind::ICall { function: Callee::Builtin(Builtin::Require(kind)), args } => {
                    let condition = args[0];
                    let payload = kind.payload(&args[1..]).expect("validated require arguments");
                    // branch condition, continuation, failure
                    // failure: encode evaluated payload; revert
                    let failure = builder.create_block();
                    let continuation = builder.create_block();
                    builder.branch(condition, continuation, failure);
                    builder.switch_to_block(failure);
                    emit_payload(gcx, module, builder, payload, helper);
                    builder.switch_to_block(continuation);
                }
                _ => unreachable!(),
            }
            true
        });
    }
    true
}

pub(super) fn is_check(kind: &InstKind) -> bool {
    kind.op_def().phase_category == Some(PhaseCategory::ConditionalCheck)
}

fn emit_payload(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    payload: RevertPayload,
    helper: Option<FunctionId>,
) {
    match payload {
        RevertPayload::ShortString { length, data } => {
            // icall revert_error(length, data); unreachable
            builder.icall_void(helper.expect("short string helper"), vec![length, data]);
            builder.invalid();
        }
        RevertPayload::LongString(symbol) => {
            let bytes = symbol.as_byte_str();
            // pointer = fmp
            // write message at pointer + 68: stores, a splat, or a data copy by cost
            // mstore pointer + 36, length
            // mstore pointer, Error.selector
            // mstore pointer + 4, 32
            // revert(pointer, 68 + padded length)
            let padded = bytes.len().next_multiple_of(32);
            let pointer = builder.fmp();
            let dest = DataDest { base: pointer, offset: 68 };
            copy_data_to_memory(gcx, module, builder, dest, bytes, padded, None);
            // NOTE: the selector word zeroes bytes 4..32, so the offset store follows it, and the
            // message-independent stores come last so that tail merging shares them.
            for (offset, word) in
                [(36, U256::from(bytes.len())), (0, ERROR_SELECTOR), (4, U256::from(32))]
            {
                let address = builder.address_value(pointer, offset);
                let word = builder.imm(word);
                builder.store(address, word);
            }
            let size = builder.imm(68 + padded as u64);
            builder.revert(pointer, size);
        }
        RevertPayload::EmptyString => {
            // revert_scratch(Error.selector, [32, 0])
            let tuple_offset = builder.imm(32);
            let length = builder.imm(0);
            builder.revert_scratch(ERROR_SELECTOR, &[tuple_offset, length]);
        }
        RevertPayload::ErrorString(value) => {
            // payload = abi_encode_scratch(Error.selector, value)
            // revert(payload.ptr, payload.len)
            let selector = builder.imm(ERROR_SELECTOR);
            let layout = Arc::new(AbiLayout::new(
                vec![AbiType::Bytes(PtrLocation::Memory)].into_boxed_slice(),
            ));
            let encoded =
                builder.abi_encode_scratch(layout, Some(selector), vec![value].into_boxed_slice());
            let pointer = builder.slice_ptr(encoded);
            let length = builder.slice_len(encoded);
            builder.revert(pointer, length);
        }
        RevertPayload::CustomError { selector, layout, values } => {
            if let Some(selector) = builder.func().value_u256(selector)
                && values.len() <= MAX_SCRATCH_ERROR_WORDS
                && layout.types.iter().all(|ty| matches!(ty, AbiType::Word(_)))
            {
                // revert_scratch(selector, values)
                builder.revert_scratch(selector, &values);
                return;
            }
            // payload = abi_encode_scratch(selector, values)
            // revert(payload.ptr, payload.len)
            let encoded = builder.abi_encode_scratch(layout, Some(selector), values);
            let pointer = builder.slice_ptr(encoded);
            let length = builder.slice_len(encoded);
            builder.revert(pointer, length);
        }
    }
}

fn create_short_string_helper(module: &mut Module, context: &InstructionMetadata) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::revert_error));
    let mut builder = FunctionBuilder::new(&mut function);
    builder.set_debug_context(context);
    // revert_scratch(Error.selector, [32, length, data])
    let length = builder.add_param(MirType::I256);
    let value = builder.add_param(MirType::I256);
    let tuple_offset = builder.imm(32);
    builder.revert_scratch(ERROR_SELECTOR, &[tuple_offset, length, value]);
    module.add_function(function)
}
