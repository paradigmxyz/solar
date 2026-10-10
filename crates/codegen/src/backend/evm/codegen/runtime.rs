//! Runtime emission and assembly.

use super::{
    ArtifactKind, BlockId, CallGraphInfo, DenseBitSet, EvmCodegen, FunctionId, MAX_STACK_DEPTH,
    MirPhase, Module, Terminator, run_pipeline,
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Runs the canonical MIR optimization pipeline on the module.
    pub(super) fn run_optimization_passes(&mut self, module: &mut Module) {
        let _changed = run_pipeline(self.gcx, module, None);
    }

    /// Schedules the runtime code of a module into the assembler's EVM IR.
    #[tracing::instrument(
        name = "stack_scheduling",
        level = "debug",
        skip_all,
        fields(artifact = "runtime")
    )]
    pub(super) fn schedule_runtime_code(
        &mut self,
        module: &crate::mir::LoweredModule<'_>,
        call_graph: &CallGraphInfo,
    ) {
        assert_eq!(
            module.phase(),
            MirPhase::Lowered,
            "EVM codegen requires MIR in the final phase"
        );
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Runtime);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        self.reset_artifact(module);
        self.reset_switch_gas_code_growth();
        if !module.functions.is_empty() {
            self.emit_runtime(module, call_graph);
        }
    }

    /// Functions reachable through internal calls from the dispatch entry and external entries.
    pub(super) fn internal_call_targets(
        module: &Module,
        call_graph: &CallGraphInfo,
        entry: FunctionId,
    ) -> DenseBitSet<FunctionId> {
        call_graph.reachable_callees_from(module.functions.iter_enumerated().filter_map(
            |(func_id, func)| {
                (func_id == entry || Self::is_external_entry(func)).then_some(func_id)
            },
        ))
    }

    /// The functions whose whole body is `stop`.
    pub(super) fn empty_stop_functions(module: &Module) -> DenseBitSet<FunctionId> {
        let mut functions = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if func.blocks.len() == 1
                && func.blocks[BlockId::ENTRY].instructions.is_empty()
                && matches!(func.blocks[BlockId::ENTRY].terminator, Some(Terminator::Stop))
            {
                functions.insert(func_id);
            }
        }
        functions
    }

    pub(super) fn report_stack_limit_error(&self) {
        self.gcx
            .dcx()
            .err(format!(
                "codegen cannot keep the generated EVM stack within {MAX_STACK_DEPTH} words"
            ))
            .emit();
    }
}
