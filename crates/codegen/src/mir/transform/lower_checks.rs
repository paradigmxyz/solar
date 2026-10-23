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
//! constant and short error strings, and custom errors of at most `MAX_SCRATCH_ERROR_WORDS` words
//! stage in the reserved low memory through `FunctionBuilder::revert_scratch`. Other custom errors
//! and error strings encode at the free-memory pointer without advancing it.

use crate::mir::{
    AbiLayout, AbiType, Builtin, Callee, ERROR_SELECTOR, EffectKind, Function, FunctionBuilder,
    FunctionId, InstKind, InstructionMetadata, MirType, Module, PhaseCategory, PtrLocation,
    RequireKind, RevertKind, RevertPayload, pass::MirPass, transform::utils::expand_block,
};
use solar_config::{OptimizationMode, RevertStrings};
use solar_interface::{Ident, sym};
use std::sync::Arc;

/// Word arguments of a custom error staged in scratch memory. The payload spans
/// `[28, 32 * (words + 1))`; a longer one would end above the heap start at `0x80`, and a
/// constant memory range there moves the spill area of every entry reaching it.
const MAX_SCRATCH_ERROR_WORDS: usize = 3;

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
        gcx: solar_sema::Gcx<'_>,
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
        let optimized = !matches!(module.optimization(gcx), OptimizationMode::None);
        let mut changed = false;
        for function in &mut module.functions {
            changed |= lower_function(function, helper, gcx.sess.opts.revert_strings, optimized);
        }
        changed
    }
}

fn lower_function(
    func: &mut Function,
    helper: Option<FunctionId>,
    revert_strings: RevertStrings,
    optimize: bool,
) -> bool {
    if !func.instructions().any(|id| is_check(&func.inst(id).kind)) {
        return false;
    }
    let blocks = func.blocks.indices();
    let mut builder = FunctionBuilder::new(func).with_revert_strings(revert_strings);
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
                    emit_payload(builder, payload, helper);
                    builder.switch_to_block(continuation);
                }
                _ => unreachable!(),
            }
            true
        });
    }
    true
}

fn is_check(kind: &InstKind) -> bool {
    kind.op_def().phase_category == Some(PhaseCategory::ConditionalCheck)
}

fn emit_payload(
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
