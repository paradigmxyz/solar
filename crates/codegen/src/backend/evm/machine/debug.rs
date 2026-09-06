//! Source origins attached after physical scheduling without changing executable plans.
//!
//! Each emitted range inherits only the MIR operation that produced it. Generated
//! frame helpers and vanished fallthroughs have no invented source location.
//! Metadata is allocated only when the retained MIR requests debug tracking.

use super::{Context, ir};
use crate::{
    backend::evm::{DebugFunction, DebugFunctionExit},
    mir,
};

pub(super) fn origin(
    context: &Context<'_>,
    metadata: &mir::InstructionMetadata,
) -> Option<Box<ir::DebugMetadata>> {
    context.module.debug_info_is_tracked().then(|| {
        Box::new(ir::DebugMetadata {
            source_spans: metadata.source_spans().collect(),
            modifier_depth: metadata.modifier_depth(),
            ..Default::default()
        })
    })
}

pub(super) fn instructions(
    context: &Context<'_>,
    metadata: &mir::InstructionMetadata,
    output: &mut [ir::Instruction],
) {
    if let Some(origin) = origin(context, metadata) {
        for instruction in output {
            instruction.debug = Some(origin.clone());
        }
    }
}

pub(super) fn function(context: &Context<'_>) -> Option<DebugFunction> {
    let function = context.function;
    if context.module.debug_info_is_tracked() && !function.attributes.is_yul {
        function
            .debug_identifier
            .map(|identifier| DebugFunction { identifier, declaration: function.declaration_span })
    } else {
        None
    }
}

pub(super) fn terminator(
    context: &Context<'_>,
    block: &mir::BasicBlock,
    output: &mut ir::Terminator,
) {
    output.debug = origin(context, &block.terminator_metadata);
    if let Some(metadata) = &mut output.debug {
        metadata.function_exit = match block.terminator {
            Some(mir::Terminator::Return { .. } | mir::Terminator::ReturnData { .. }) => {
                Some(DebugFunctionExit::Return)
            }
            Some(mir::Terminator::Revert { .. } | mir::Terminator::RevertReturndata) => {
                Some(DebugFunctionExit::Revert)
            }
            _ => None,
        };
    }
}
