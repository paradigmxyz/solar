//! Adapter for native MIR passes. A pass preserves the current representation phase and edits
//! existing SSA identities rather than reparsing the module, so declarations and source metadata
//! remain owned by the compiler. Edits are checked transactionally, including dominance and
//! result types; the ordinary pass manager invalidates analyses and derived backend records.

use super::Pass;
use crate::mir::{
    self, FunctionId, InstId, InstKind, Module, ValueId,
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::bit_set::DenseBitSet;
use solar_pass_plugin::{Edit, Function, MirInstruction, Snapshot, Value};
use solar_sema::Gcx;

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
        let snapshot = Snapshot::Mir {
            name: module.name.to_string(),
            phase: module.phase().name().into(),
            text: module.to_text().to_string(),
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
        for edit in response.edits {
            apply_edit(&mut candidate, edit)?;
        }
        candidate.invalidate_lowered_check();
        candidate.backend_memory = None;
        mir::validate(gcx.dcx(), &candidate);
        gcx.dcx().has_errors().map_err(|_| "produced invalid MIR")?;
        // module => verified candidate
        *module = candidate;
        Ok(true)
    }
}

fn apply_edit(module: &mut Module, edit: Edit) -> Result<(), String> {
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
            if func.value_ty(value).is_none() || func.value_ty(value) != func.value_ty(replacement)
            {
                return Err("replacement SSA value has a different type".into());
            }
            // uses(%value) => %replacement
            func.rewrite_all_operands(|operand| {
                if *operand == value {
                    *operand = replacement;
                }
            });
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
