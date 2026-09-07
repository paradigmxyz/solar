//! Expand semantic conditional checks before revert outlining and arithmetic expansion.
//!
//! Each check splits its block at the original execution point and branches to a typed panic or
//! revert payload. Checks in one function share payload blocks, merging their debug origins.
//! Surviving instructions and the original terminator move to the continuation; successor phi
//! predecessors update locally. The session's revert-string mode selects debug payloads without
//! changing when argument evaluation or the check occurs. Require payloads read their evaluated
//! arguments only on failure; short constant strings share a module helper. New helper creation
//! invalidates module analyses along with the rewritten call edges.

use crate::{
    mir::{
        AbiLayout, AbiType, ERROR_SELECTOR, Function, FunctionBuilder, FunctionId, InstKind,
        InstructionMetadata, MirType, Module, RevertPayload, SliceLocation,
    },
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_config::RevertStrings;
use solar_interface::{Ident, sym};
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
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut helper_context = InstructionMetadata::EMPTY;
        let mut needs_helper = false;
        for function in &module.functions {
            for id in function.instructions() {
                let inst = function.inst(id);
                if let InstKind::Require { payload, .. } = &inst.kind
                    && matches!(payload.as_ref(), RevertPayload::ShortString { .. })
                {
                    needs_helper = true;
                    helper_context.merge_debug_context(&inst.metadata);
                }
            }
        }
        let helper = needs_helper.then(|| create_short_string_helper(module, &helper_context));
        let mut changed = false;
        for function in &mut module.functions {
            changed |= lower_function(function, helper, gcx.sess.opts.revert_strings);
        }
        changed
    }
}

fn lower_function(
    func: &mut Function,
    helper: Option<FunctionId>,
    revert_strings: RevertStrings,
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
        let instructions = std::mem::take(&mut builder.func_mut().blocks[block].instructions);
        let (terminator, metadata) = builder.func_mut().blocks[block].take_terminator();
        builder.switch_to_block(block);
        for id in instructions {
            if !is_check(&builder.func().inst(id).kind) {
                // continuation: original instruction
                let current = builder.current_block();
                builder.func_mut().blocks[current].instructions.push(id);
                continue;
            }
            let inst = builder.func().inst(id).clone();
            builder.set_debug_context(&inst.metadata);
            match inst.kind {
                InstKind::Check { condition, is_zero, failure } => {
                    // branch condition, failure, continuation
                    // failure: revert payload
                    builder.branch_to_revert(condition, is_zero, failure);
                }
                InstKind::Require { condition, payload } => {
                    // branch condition, continuation, failure
                    // failure: encode evaluated payload; revert
                    let failure = builder.create_block();
                    let continuation = builder.create_block();
                    builder.branch(condition, continuation, failure);
                    builder.switch_to_block(failure);
                    emit_payload(&mut builder, *payload, helper);
                    builder.switch_to_block(continuation);
                }
                _ => unreachable!(),
            }
        }
        // continuation: original terminator
        let end = builder.current_block();
        if let Some(terminator) = terminator {
            builder.func_mut().blocks[end].set_terminator(terminator, metadata);
        }
        if end != block {
            redirect_successor_predecessors(builder.func_mut(), block, end);
        }
    }
    true
}

fn is_check(kind: &InstKind) -> bool {
    matches!(kind, InstKind::Check { .. } | InstKind::Require { .. })
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
            // mstore(0, Error.selector); mstore(4, 32); mstore(36, 0); revert(0, 68)
            let selector = builder.imm(ERROR_SELECTOR);
            let zero = builder.imm(0);
            builder.mstore(zero, selector);
            let offset = builder.imm(4);
            let tuple_offset = builder.imm(32);
            builder.mstore(offset, tuple_offset);
            let length = builder.imm(36);
            builder.mstore(length, zero);
            let size = builder.imm(68);
            builder.revert(zero, size);
        }
        RevertPayload::ErrorString(value) => {
            // payload = abi_encode(Error.selector, value)
            // revert(payload.ptr, payload.len)
            let selector = builder.imm(ERROR_SELECTOR);
            let layout = Arc::new(AbiLayout::new(
                vec![AbiType::Bytes(SliceLocation::Memory)].into_boxed_slice(),
            ));
            let encoded =
                builder.abi_encode(layout, Some(selector), vec![value].into_boxed_slice());
            let pointer = builder.slice_ptr(encoded);
            let length = builder.slice_len(encoded);
            builder.revert(pointer, length);
        }
        RevertPayload::CustomError { selector, layout, values } => {
            // payload = abi_encode(selector, values)
            // revert(payload.ptr, payload.len)
            let encoded = builder.abi_encode(layout, Some(selector), values);
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
    // mstore(0, Error.selector); mstore(4, 32)
    // mstore(36, length); mstore(68, data); revert(0, 100)
    let length = builder.add_param(MirType::uint256());
    let value = builder.add_param(MirType::uint256());
    let selector = builder.imm(ERROR_SELECTOR);
    let zero = builder.imm(0);
    builder.mstore(zero, selector);
    let offset = builder.imm(4);
    let tuple_offset = builder.imm(32);
    builder.mstore(offset, tuple_offset);
    let length_offset = builder.imm(36);
    builder.mstore(length_offset, length);
    let data_offset = builder.imm(68);
    builder.mstore(data_offset, value);
    let size = builder.imm(100);
    builder.revert(zero, size);
    module.add_function(function)
}
