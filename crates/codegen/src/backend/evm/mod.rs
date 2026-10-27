//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `codegen::stackify`: MIR-to-EVM stack scheduling for DUP/SWAP generation

mod codegen;
use codegen::PlanCache;
pub(crate) use codegen::select;
pub use codegen::{EvmArtifact, EvmCodegen};

mod debug_info;
pub(crate) use debug_info::DebugInfoBuilder;
pub use debug_info::{
    DebugFunction, DebugFunctionExit, DebugInfo, DebugInstruction, DebugInstructions,
    DebugLocation, DebugSpans, MAX_DEBUG_SPANS,
};

mod disasm;
pub use disasm::{disassemble, disassemble_standard_json};

pub mod ir;

mod layout;

pub use solar_evmir::op;

/// What code generation keeps for a whole compilation, on its [`Gcx`](solar_sema::Gcx): the
/// results of its searches and its plans, which every contract may read and fill.
#[derive(Default)]
pub(crate) struct CodegenState {
    schedules: ir::SharedSchedules,
    plans: PlanCache,
}

impl CodegenState {
    /// Frees the stored plans.
    pub(crate) fn clear_plans(&self) {
        self.plans.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_isle_prelude_matches_table() {
        snapbox::assert_data_eq!(
            op::isle_prelude(),
            snapbox::file!["../../../isle/evm-ir/prelude.isle"]
        );
    }
}
