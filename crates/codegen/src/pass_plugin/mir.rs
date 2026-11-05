//! Adapter for native MIR passes. A pass preserves the current representation phase and edits
//! SSA identities and CFGs without reparsing the module, so existing declarations and source
//! metadata remain owned by the compiler. New internal functions preserve the phase. Edits are
//! checked transactionally, including dominance and
//! result types; the ordinary pass manager invalidates analyses and derived backend records.

use super::Pass;
use crate::mir::{
    self, FunctionId, InstId, InstKind, Module, ValueId,
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_pass_plugin::{Edit, Function, MirInstruction, Snapshot, Value};
use solar_sema::Gcx;

mod graph;

pub(crate) struct MirPluginPass(pub(super) Pass);

impl MirPass for MirPluginPass {
    fn name(&self) -> &str {
        &self.0.name
    }

    fn is_required(&self) -> bool {
        true
    }

    fn cache_key(&self) -> Option<(&'static str, &str)> {
        None
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        match self.apply(gcx, module) {
            Ok(changed) => changed,
            Err(error) => {
                analyses.fail(self.0.error(gcx, error));
                false
            }
        }
    }
}

impl MirPluginPass {
    pub(crate) fn is_default(&self, phase: mir::MirPhase) -> bool {
        self.0.default && self.0.phase == Some(phase)
    }

    fn apply(&self, gcx: Gcx<'_>, module: &mut Module) -> Result<bool, String> {
        if self.0.phase != Some(module.phase()) {
            return Err(format!(
                "requires {} MIR, got {}",
                self.0.phase.unwrap().name(),
                module.phase().name()
            ));
        }
        let mut printable = module.clone();
        for function in &mut printable.functions {
            function.debug_signature = None;
            for block in &mut function.blocks {
                block.label_metadata.copy_debug_context(&mir::InstructionMetadata::EMPTY);
                block.terminator_metadata.copy_debug_context(&mir::InstructionMetadata::EMPTY);
            }
            function.for_each_instruction_mut(|_, inst| {
                inst.metadata.copy_debug_context(&mir::InstructionMetadata::EMPTY);
                inst.metadata.set_hir_expr(None);
            });
        }
        let snapshot = Snapshot::Mir {
            name: module.name.to_string(),
            phase: module.phase().name().into(),
            unsafe_assembly: module.has_unsafe_assembly(),
            text: printable.to_text().to_string(),
            functions: module
                .functions
                .iter_enumerated()
                .map(|(id, func)| {
                    let mut values = DenseBitSet::new_empty(func.num_values());
                    for value in func.live_values() {
                        values.insert(value);
                    }
                    Function {
                        id: id.index() as u32,
                        name: func.name.to_string(),
                        optimization: func
                            .attributes
                            .optimization
                            .unwrap_or(module.optimization)
                            .to_string(),
                        blocks: func.blocks.indices().map(|id| id.index() as u32).collect(),
                        instructions: func
                            .blocks
                            .iter_enumerated()
                            .flat_map(|(block, data)| {
                                data.instructions.iter().map(move |&id| {
                                    let inst = func.inst(id);
                                    MirInstruction {
                                        id: id.index() as u32,
                                        block: block.index() as u32,
                                        operation: inst.kind.mnemonic().into(),
                                        operands: inst
                                            .operands()
                                            .iter()
                                            .map(|id| id.index() as u32)
                                            .collect(),
                                        result: func
                                            .inst_result_value(id)
                                            .map(|id| id.index() as u32),
                                        result_type: inst.result_ty.map(|ty| ty.to_string()),
                                    }
                                })
                            })
                            .collect(),
                        values: values
                            .iter()
                            .map(|id| Value {
                                id: id.index() as u32,
                                ty: func
                                    .value_ty(id)
                                    .map_or_else(|| "void".into(), |ty| ty.to_string()),
                                constant: func.value_u256(id).map(|word| format!("{word:#x}")),
                            })
                            .collect(),
                    }
                })
                .collect(),
        };
        let response = self.0.run(gcx, module.optimization.to_string(), snapshot)?;
        if response.edits.is_empty() {
            return Ok(false);
        }
        let mut candidate = module.clone();
        let mut locals = FxHashMap::default();
        for edit in response.edits {
            apply_edit(&mut candidate, edit, &mut locals)?;
        }
        // Graph edits can refer forward to values whose producers were not known at insertion.
        for function in &mut candidate.functions {
            function.canonicalize_operands();
        }
        clear_unverified_memory_proofs(&mut candidate);
        candidate.invalidate_lowered_check();
        candidate.backend_memory = None;
        // Plugin operations do not carry the source lowerer's memory-placement proof.
        candidate.set_unsafe_assembly(true);
        mir::validate(gcx.dcx(), &candidate);
        gcx.dcx().has_errors().map_err(|_| "produced invalid MIR")?;
        // module => verified candidate
        *module = candidate;
        Ok(true)
    }
}

fn apply_edit(module: &mut Module, edit: Edit, locals: &mut graph::Locals) -> Result<(), String> {
    if matches!(
        &edit,
        Edit::AddMirFunctions { .. }
            | Edit::AddMirBlocks { .. }
            | Edit::SpliceMirInstructions { .. }
            | Edit::SetMirTerminator { .. }
            | Edit::SplitMirBlock { .. }
            | Edit::ReplaceMirValueWith { .. }
    ) {
        return graph::apply_edit(module, edit, locals);
    }
    let (Edit::ReplaceMirValue { function, .. } | Edit::RewriteMirInstruction { function, .. }) =
        &edit
    else {
        return Err("EVM IR edit returned by a MIR pass".into());
    };
    if *function as usize >= module.functions.len() {
        return Err("unknown MIR function".into());
    }
    let func = &mut module.functions[FunctionId::from_usize(*function as usize)];
    match edit {
        Edit::ReplaceMirValue { value, replacement, .. } => {
            let value = checked_value(func, value)?;
            let replacement = checked_value(func, replacement)?;
            replace_value(func, value, replacement)?;
        }
        Edit::RewriteMirInstruction { instruction, operation, operands, .. } => {
            if instruction as usize >= func.num_insts() {
                return Err("unknown or inactive MIR instruction".into());
            }
            let instruction = InstId::from_usize(instruction as usize);
            if !func.instructions().any(|active| active == instruction) {
                return Err("unknown or inactive MIR instruction".into());
            }
            let operands = operands
                .into_iter()
                .map(|value| checked_value(func, value))
                .collect::<Result<Vec<_>, _>>()?;
            let kind = InstKind::from_operands(&operation, &operands)
                .ok_or("unknown operand-only operation or invalid operand count")?;
            let inst = func.inst(instruction);
            if !inst.can_replace_kind(&kind) {
                return Err("rewrite changes the instruction's result representation".into());
            }
            // %result = old(...) => %result = operation(operands)
            func.replace_inst_kind(instruction, kind);
        }
        _ => unreachable!("checked MIR edit above"),
    }
    Ok(())
}

fn checked_value(func: &mir::Function, value: u32) -> Result<ValueId, String> {
    if value as usize >= func.num_values() {
        return Err("unknown MIR value".into());
    }
    Ok(ValueId::from_usize(value as usize))
}

fn replace_value(
    func: &mut mir::Function,
    value: ValueId,
    replacement: ValueId,
) -> Result<(), String> {
    if func.value_ty(value).is_none() || func.value_ty(value) != func.value_ty(replacement) {
        return Err("replacement SSA value has a different type".into());
    }
    // uses(%value) => %replacement
    func.rewrite_all_operands(|operand| {
        if *operand == value {
            *operand = replacement;
        }
    });
    Ok(())
}

/// Foreign edits can change an offset's producer or bypass its original bounds check without
/// directly replacing the flagged instruction. Treat all retained memory proofs as unverified.
fn clear_unverified_memory_proofs(module: &mut Module) {
    for function in &mut module.functions {
        let instructions = function.instructions().collect::<Vec<_>>();
        for instruction in instructions {
            function.inst_mut(instruction).metadata.clear_analysis_facts();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{
        AllocationSemantics, FunctionBuilder, MemoryRegion, MirType, Value as MirValue,
    };
    use solar_interface::{Ident, Session};

    #[test]
    fn foreign_rewrites_drop_checked_offsets_and_dependent_proofs() {
        let sess = Session::builder().with_test_emitter().build();
        sess.enter_sequential(|| {
            let mut function = mir::Function::new(Ident::from_str("offset"));
            function.set_return_type(MirType::I256);
            let mut builder = FunctionBuilder::new(&mut function);
            let base = builder.add_param(MirType::I256);
            let offset = builder.add_param(MirType::I256);
            let replacement = builder.add_param(MirType::I256);
            let sum = builder.add(base, offset);
            let dependent = builder.add(sum, offset);
            let size = builder.imm(32);
            let allocation = builder.alloc(size, AllocationSemantics::INTERNAL);
            builder.ret([dependent]);
            let MirValue::Inst(add) = *function.value(sum) else { unreachable!() };
            let MirValue::Inst(user) = *function.value(dependent) else { unreachable!() };
            let MirValue::Inst(alloc) = *function.value(allocation) else { unreachable!() };
            function.inst_mut(alloc).metadata.set_preserves_fmp(true);
            function.inst_mut(alloc).metadata.set_deferred_alloc();
            for instruction in [add, user] {
                let metadata = &mut function.inst_mut(instruction).metadata;
                metadata.set_checked_object_offset();
                metadata.set_alloc_ptr(true);
                metadata.set_disjoint(true);
                metadata.set_memory_region(Some(MemoryRegion::Heap));
                metadata.set_effect(Some(mir::EffectKind::Pure));
                metadata.set_storage_alias(Some(mir::StorageAlias::Symbolic(base)));
                metadata.mark_debug_info_dropped();
            }
            let debug = function.inst(add).metadata.debug_context();
            let mut module = Module::new(Ident::from_str("M"));
            let id = module.add_function(function);
            apply_edit(
                &mut module,
                Edit::RewriteMirInstruction {
                    function: id.index() as u32,
                    instruction: add.index() as u32,
                    operation: "add".into(),
                    operands: vec![base.index() as u32, replacement.index() as u32],
                },
                &mut FxHashMap::default(),
            )
            .unwrap();
            // The equivalent-rewrite API retains the original proof until the plugin boundary.
            assert!(module.functions[id].inst(add).metadata.checked_object_offset());
            clear_unverified_memory_proofs(&mut module);
            for instruction in [add, user] {
                let metadata = &module.functions[id].inst(instruction).metadata;
                assert!(!metadata.checked_object_offset());
                assert!(!metadata.alloc_ptr());
                assert!(!metadata.disjoint());
                assert_eq!(metadata.memory_region(), None);
                assert_eq!(metadata.effect(), None);
                assert_eq!(metadata.storage_alias(), None);
                assert!(!metadata.preserves_fmp());
                assert!(!metadata.deferred_alloc());
            }
            assert!(module.functions[id].inst(alloc).metadata.preserves_fmp());
            assert!(module.functions[id].inst(alloc).metadata.deferred_alloc());
            assert!(crate::mir::analysis::pointer_bases(&module.functions[id], sum, 0).is_empty());
            let metadata = &module.functions[id].inst(add).metadata;
            assert_eq!(metadata.debug_context(), debug);
        });
    }
}
