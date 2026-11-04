//! Adapter for native physical EVM IR rewrites. A plugin splices existing block instruction
//! lists, retaining declarations, relocations, terminators, and CFG identities. Glue boundaries
//! cannot be crossed. The whole candidate is stack-verified and checked against the selected
//! fork before it is committed, including in release builds with `-Zvalidate-ir=false`.

use super::Pass;
use crate::backend::evm::{
    ir::{self, BlockId, Instruction, Module, passes::EvmPass},
    op,
};
use alloy_primitives::U256;
use solar_pass_plugin::{Edit, EvmBlock, EvmInstruction, NewEvmInstruction, Snapshot};
use solar_sema::Gcx;

pub(crate) struct EvmPluginPass(pub(super) Pass);

impl EvmPass for EvmPluginPass {
    fn name(&self) -> &str {
        &self.0.name
    }
    fn is_required(&self) -> bool {
        true
    }
    fn is_cacheable(&self) -> bool {
        false
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        match self.apply(gcx, module) {
            Ok(changed) => changed,
            Err(error) => {
                self.0.error(gcx, error);
                false
            }
        }
    }
}

impl EvmPluginPass {
    pub(crate) fn is_default(&self) -> bool {
        self.0.default
    }

    fn apply(&self, gcx: Gcx<'_>, module: &mut Module) -> Result<bool, String> {
        let snapshot = Snapshot::EvmIr {
            name: module.name().to_string(),
            text: module.to_text().to_string(),
            blocks: module
                .blocks
                .iter_enumerated()
                .map(|(id, block)| EvmBlock {
                    id: id.index() as u32,
                    instructions: block
                        .instructions
                        .iter()
                        .enumerate()
                        .map(|(index, inst)| EvmInstruction {
                            index: index as u32,
                            text: inst.mnemonic().to_string(),
                            opcode: inst.as_evm_opcode().unwrap_or(inst.opcode),
                            immediate: inst.pushed_value().map(|value| format!("{value:#x}")),
                            protected: inst.metadata.keep_with_next
                                || (index != 0
                                    && block.instructions[index - 1].metadata.keep_with_next),
                        })
                        .collect(),
                })
                .collect(),
        };
        let response = self.0.run(gcx, module.optimization.to_string(), snapshot)?;
        if response.edits.is_empty() {
            return Ok(false);
        }
        let verifier = ir::verifier(gcx);
        let before = verifier.target_support_snapshot(module);
        let mut candidate = module.clone();
        for edit in response.edits {
            apply_edit(&mut candidate, edit)?;
        }
        verifier.verify_module(&candidate);
        verifier.verify_between_passes(&candidate, &before);
        gcx.dcx().has_errors().map_err(|_| "produced invalid EVM IR")?;
        // module => verified candidate
        *module = candidate;
        Ok(true)
    }
}

fn apply_edit(module: &mut Module, edit: Edit) -> Result<(), String> {
    let Edit::SpliceEvmInstructions { block, start, remove, instructions } = edit else {
        return Err("MIR edit returned by an EVM IR pass".into());
    };
    if block as usize >= module.blocks.len() {
        return Err("unknown EVM IR block".into());
    }
    let block = &mut module.blocks[BlockId::from_usize(block as usize)];
    let start = start as usize;
    let end = start.checked_add(remove as usize).ok_or("instruction window overflow")?;
    if end > block.instructions.len() {
        return Err("instruction window is out of bounds".into());
    }
    if (start != 0 && block.instructions[start - 1].metadata.keep_with_next)
        || block.instructions[start..end].iter().any(|inst| inst.metadata.keep_with_next)
    {
        return Err("instruction window crosses a protected boundary".into());
    }
    let mut replacement = Vec::with_capacity(instructions.len());
    for instruction in instructions {
        let (mut inst, source) = match instruction {
            NewEvmInstruction::Copy { index } => {
                let inst =
                    block.instructions.get(index as usize).ok_or("unknown copied instruction")?;
                if inst.metadata.keep_with_next
                    || (index != 0
                        && block.instructions[index as usize - 1].metadata.keep_with_next)
                {
                    return Err("cannot copy a protected instruction".into());
                }
                replacement.push(inst.clone());
                continue;
            }
            NewEvmInstruction::Opcode { opcode, source } => {
                if op::definition(opcode).is_none() || (op::PUSH1..=op::PUSH32).contains(&opcode) {
                    return Err("unknown opcode or missing push immediate".into());
                }
                (Instruction::opcode(opcode).with_debug_info_dropped(), source)
            }
            NewEvmInstruction::Push { value, source } => {
                let value = value.parse::<U256>().map_err(|_| "invalid 256-bit push value")?;
                (Instruction::push_value(value).with_debug_info_dropped(), source)
            }
        };
        if let Some(source) = source {
            if !(start..end).contains(&(source as usize)) {
                return Err("debug source must belong to the replaced instruction window".into());
            }
            inst.metadata.copy_debug_info_from(&block.instructions[source as usize].metadata);
        }
        replacement.push(inst);
    }
    // block[start..end] => replacement
    block.instructions.splice(start..end, replacement);
    Ok(())
}
