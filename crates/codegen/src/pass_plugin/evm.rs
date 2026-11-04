//! Adapter for native physical EVM IR rewrites. A plugin splices existing block instruction
//! lists and constructs blocks and transfers, retaining existing declarations and relocations.
//! Callable helpers use explicit stack conventions; this IR has no typed function objects. Glue
//! boundaries cannot be crossed. The whole candidate is stack-verified against the selected
//! fork before it is committed, including in release builds with `-Zvalidate-ir=false`.

use super::Pass;
use crate::backend::evm::{
    ir::{
        self, Block, BlockId, CallEffect, Instruction, Module, StackEffect, Terminator,
        TerminatorKind, passes::EvmPass,
    },
    op,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashSet;
use solar_pass_plugin::{
    Edit, EvmBlock, EvmInstruction, EvmTerminator, NewEvmInstruction, Snapshot,
};
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
                    label: block.label,
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
        candidate.peephole_clean.clear();
        verifier.verify_module(&candidate);
        verifier.verify_between_passes(&candidate, &before);
        gcx.dcx().has_errors().map_err(|_| "produced invalid EVM IR")?;
        // module => verified candidate
        *module = candidate;
        Ok(true)
    }
}

fn apply_edit(module: &mut Module, edit: Edit) -> Result<(), String> {
    if let Edit::SplitEvmBlock { block, at } = edit {
        let block = checked_block(module, block)?;
        let at = at as usize;
        let body = &module.blocks[block];
        if at > body.instructions.len() {
            return Err("EVM split position is out of bounds".into());
        }
        if at != 0 && body.instructions[at - 1].metadata.keep_with_next {
            return Err("block split crosses a protected boundary".into());
        }
        let mut label = 0_u32;
        let labels = module.blocks.iter().map(|block| block.label).collect::<FxHashSet<_>>();
        while labels.contains(&label) {
            label = label.checked_add(1).ok_or("no free EVM IR block labels")?;
        }
        let mut next = Block::new(label);
        // bb: prefix; jump next
        // next: suffix; old_terminator
        next.instructions = module.blocks[block].instructions.split_off(at);
        next.terminator = module.blocks[block].terminator.take();
        let next = module.add_block(next);
        module.blocks[block].terminator =
            Some(Terminator::new(TerminatorKind::Jump(next)).with_debug_info_dropped());
        return Ok(());
    }
    if let Edit::AddEvmBlocks { blocks } = edit {
        let first = module.blocks.len();
        let mut labels = module.blocks.iter().map(|block| block.label).collect::<FxHashSet<_>>();
        let mut label = 0_u32;
        for definition in &blocks {
            while labels.contains(&label) {
                label = label.checked_add(1).ok_or("no free EVM IR block labels")?;
            }
            let mut block = Block::new(label);
            labels.insert(label);
            if let Some(effect) = &definition.call_effect {
                if effect.ret >= effect.inputs {
                    return Err("call effect does not consume its return address".into());
                }
                block.metadata.call_effect = Some(CallEffect {
                    ret: effect.ret,
                    effect: StackEffect::new(effect.inputs, effect.outputs),
                });
            }
            // bbN:
            module.add_block(block);
        }
        for (index, definition) in blocks.into_iter().enumerate() {
            let block = BlockId::from_usize(first + index);
            let instructions = materialize(module, block, 0, 0, definition.instructions)?;
            let terminator = build_terminator(module, definition.terminator)?;
            // bb.instructions = instructions
            // bb.terminator = terminator
            module.blocks[block].instructions = instructions;
            module.blocks[block].terminator = Some(terminator);
        }
        return Ok(());
    }
    if let Edit::SetEvmTerminator { block, terminator } = edit {
        let block = checked_block(module, block)?;
        let terminator = build_terminator(module, terminator)?;
        // bb.terminator => terminator
        module.blocks[block].terminator = Some(terminator);
        return Ok(());
    }
    let Edit::SpliceEvmInstructions { block, start, remove, instructions } = edit else {
        return Err("MIR edit returned by an EVM IR pass".into());
    };
    let block = checked_block(module, block)?;
    let body = &module.blocks[block];
    let start = start as usize;
    let end = start.checked_add(remove as usize).ok_or("instruction window overflow")?;
    if end > body.instructions.len() {
        return Err("instruction window is out of bounds".into());
    }
    if (start != 0 && body.instructions[start - 1].metadata.keep_with_next)
        || body.instructions[start..end].iter().any(|inst| inst.metadata.keep_with_next)
    {
        return Err("instruction window crosses a protected boundary".into());
    }
    let replacement = materialize(module, block, start, end, instructions)?;
    // block[start..end] => replacement
    module.blocks[block].instructions.splice(start..end, replacement);
    Ok(())
}

fn materialize(
    module: &Module,
    block: BlockId,
    start: usize,
    end: usize,
    instructions: Vec<NewEvmInstruction>,
) -> Result<Vec<Instruction>, String> {
    let mut replacement = Vec::with_capacity(instructions.len());
    for instruction in instructions {
        let (mut inst, source) = match instruction {
            NewEvmInstruction::Copy { index } => {
                replacement.push(copy_instruction(module, block, index)?);
                continue;
            }
            NewEvmInstruction::CopyFrom { block, index } => {
                replacement.push(copy_instruction(module, checked_block(module, block)?, index)?);
                continue;
            }
            NewEvmInstruction::BlockAddress { block } => {
                replacement.push(
                    Instruction::push_block(checked_block(module, block)?)
                        .with_debug_info_dropped(),
                );
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
            inst.metadata
                .copy_debug_info_from(&module.blocks[block].instructions[source as usize].metadata);
        }
        replacement.push(inst);
    }
    Ok(replacement)
}

fn checked_block(module: &Module, block: u32) -> Result<BlockId, String> {
    if block as usize >= module.blocks.len() {
        return Err("unknown EVM IR block".into());
    }
    Ok(BlockId::from_usize(block as usize))
}

fn copy_instruction(module: &Module, block: BlockId, index: u32) -> Result<Instruction, String> {
    let block = &module.blocks[block];
    let inst = block.instructions.get(index as usize).ok_or("unknown copied instruction")?;
    if inst.metadata.keep_with_next
        || (index != 0 && block.instructions[index as usize - 1].metadata.keep_with_next)
    {
        return Err("cannot copy a protected instruction".into());
    }
    Ok(inst.clone())
}

fn build_terminator(module: &Module, term: EvmTerminator) -> Result<Terminator, String> {
    let kind = match term {
        EvmTerminator::Jump { target } => TerminatorKind::Jump(checked_block(module, target)?),
        EvmTerminator::Branch { then_block, else_block } => TerminatorKind::JumpI {
            then_block: checked_block(module, then_block)?,
            else_block: checked_block(module, else_block)?,
        },
        EvmTerminator::IndexedJump { targets } => {
            if targets.is_empty() {
                return Err("indexed jump requires at least one target".into());
            }
            TerminatorKind::IndexedJump(
                targets
                    .into_iter()
                    .map(|target| checked_block(module, target))
                    .collect::<Result<_, _>>()?,
            )
        }
        EvmTerminator::Opcode { opcode } => {
            if op::definition(opcode).is_none() {
                return Err("unknown EVM terminator opcode".into());
            }
            TerminatorKind::Op(opcode)
        }
    };
    Ok(Terminator::new(kind).with_debug_info_dropped())
}
