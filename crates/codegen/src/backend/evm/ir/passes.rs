//! EVM IR optimization and layout passes.

use super::*;
use crate::timing::PassTimer;
use alloy_primitives::U256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};

/// A named EVM IR pass exposed to `solar evm-opt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrPass {
    /// No transform; validate and print the module.
    None,
    /// Materialize virtual instruction operands with physical stack operations.
    StackSchedule,
    /// Move cold terminal blocks after hot fallthrough code when this preserves fallthrough edges.
    ColdLayout,
    /// Replace duplicate terminal block bodies with jumps to the first copy when profitable.
    TerminalDedup,
}

/// Options for running an EVM IR pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct EvmIrPassOptions {
    /// Print the time spent in the pass.
    pub time_passes: bool,
}

impl EvmIrPass {
    /// Stable command-line pass name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::StackSchedule => "stack-schedule",
            Self::ColdLayout => "cold-layout",
            Self::TerminalDedup => "terminal-dedup",
        }
    }

    /// Runs this pass on an EVM IR module.
    pub fn run(self, module: &mut EvmIrModule, options: EvmIrPassOptions) -> bool {
        let timer = PassTimer::new(options.time_passes);
        let changed = match self {
            Self::None => false,
            Self::StackSchedule => super::super::ir_stack_schedule::schedule_stack_ops(module),
            Self::ColdLayout => move_cold_terminal_blocks(module),
            Self::TerminalDedup => deduplicate_terminal_blocks(module),
        };
        timer.finish("EVM IR", &module.name, self.name(), changed);
        changed
    }

    /// Looks up a pass by command-line name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "stack-schedule" => Self::StackSchedule,
            "cold-layout" => Self::ColdLayout,
            "terminal-dedup" => Self::TerminalDedup,
            _ => return None,
        })
    }
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const EVM_IR_PASSES: &[EvmIrPass] =
    &[EvmIrPass::None, EvmIrPass::StackSchedule, EvmIrPass::ColdLayout, EvmIrPass::TerminalDedup];

fn move_cold_terminal_blocks(module: &mut EvmIrModule) -> bool {
    let mut kept = Vec::with_capacity(module.blocks.len());
    let mut moved = Vec::new();

    for (block_id, block) in module.blocks.iter_enumerated() {
        if is_movable_cold_terminal_block(module, block_id, block) {
            moved.push(block_id);
        } else {
            kept.push(block_id);
        }
    }

    if moved.is_empty() {
        return false;
    }

    kept.extend(moved);
    remap_block_order(module, &kept);
    true
}

fn is_movable_cold_terminal_block(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    block: &EvmIrBlock,
) -> bool {
    if module.entry_block == Some(block_id) || block_id.index() == 0 {
        return false;
    }
    let Some(term) = &block.terminator else {
        return false;
    };
    if block.metadata.hotness != EvmIrBlockHotness::Cold || !is_evm_terminal(&term.kind) {
        return false;
    }
    let previous = EvmIrBlockId::from_usize(block_id.index() - 1);
    module.blocks[previous].terminator.as_ref().is_some_and(|term| is_layout_barrier(&term.kind))
}

fn is_layout_barrier(kind: &EvmIrTerminatorKind) -> bool {
    matches!(kind, EvmIrTerminatorKind::Jump(_)) || is_evm_terminal(kind)
}

fn deduplicate_terminal_blocks(module: &mut EvmIrModule) -> bool {
    let mut canonical = Vec::<(TerminalBlockKey, EvmIrBlockId)>::new();
    let mut changed = false;

    let block_ids: Vec<_> = module.blocks.indices().collect();
    for block_id in block_ids {
        let block = &module.blocks[block_id];
        if !terminal_block_dedup_is_profitable(block) {
            continue;
        }
        let Some(key) = terminal_block_key(block) else { continue };
        if let Some((_, target)) = canonical.iter().find(|(known, _)| *known == key) {
            module.blocks[block_id].instructions.clear();
            module.blocks[block_id].terminator =
                Some(EvmIrTerminator::new(EvmIrTerminatorKind::Jump(*target)));
            changed = true;
        } else {
            canonical.push((key, block_id));
        }
    }

    changed
}

fn terminal_block_dedup_is_profitable(block: &EvmIrBlock) -> bool {
    let Some(term) = &block.terminator else { return false };
    if !is_evm_terminal(&term.kind) {
        return false;
    }
    // A replacement block still needs `JUMPDEST PUSH2(label) JUMP`. Avoid
    // rewriting tiny revert blocks where size is equal and revert-path gas
    // would get worse.
    let current_size = 1
        + block.instructions.iter().map(estimated_instruction_size).sum::<usize>()
        + estimated_terminator_size(&term.kind);
    let replacement_size = 1 + 3 + 1;
    current_size > replacement_size
}

fn estimated_instruction_size(inst: &EvmIrInstruction) -> usize {
    match &inst.kind {
        EvmIrInstructionKind::Stack(_) => 1,
        EvmIrInstructionKind::Operation(mnemonic) if mnemonic == "push" => {
            match inst.operands.as_slice() {
                [operand] => estimated_push_size(operand),
                _ => 1,
            }
        }
        EvmIrInstructionKind::Operation(mnemonic) if mnemonic == "push_immutable" => 33,
        EvmIrInstructionKind::Operation(_) => 1,
    }
}

fn estimated_terminator_size(kind: &EvmIrTerminatorKind) -> usize {
    let operand_pushes = |operands: &[&EvmIrOperand]| {
        operands.iter().map(|operand| estimated_push_size(operand)).sum::<usize>() + 1
    };
    match kind {
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => operand_pushes(&[offset, size]),
        EvmIrTerminatorKind::SelfDestruct { recipient } => operand_pushes(&[recipient]),
        EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => 1,
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Branch { .. }
        | EvmIrTerminatorKind::Switch { .. } => 0,
    }
}

fn estimated_push_size(operand: &EvmIrOperand) -> usize {
    match operand {
        EvmIrOperand::Immediate(value) if *value == U256::ZERO => 1,
        EvmIrOperand::Immediate(value) => value.byte_len() + 1,
        EvmIrOperand::Block(_) | EvmIrOperand::Symbol(_) => 3,
        EvmIrOperand::Value(_) => 0,
    }
}

fn is_evm_terminal(kind: &EvmIrTerminatorKind) -> bool {
    matches!(
        kind,
        EvmIrTerminatorKind::Return { .. }
            | EvmIrTerminatorKind::Revert { .. }
            | EvmIrTerminatorKind::Stop
            | EvmIrTerminatorKind::Invalid
            | EvmIrTerminatorKind::SelfDestruct { .. }
    ) || matches!(kind, EvmIrTerminatorKind::RawOpcode(opcode) if super::super::assembler::op::is_terminal(*opcode))
}

fn terminal_block_key(block: &EvmIrBlock) -> Option<TerminalBlockKey> {
    let mut locals = FxHashMap::default();
    let mut instructions = Vec::with_capacity(block.instructions.len());

    for inst in &block.instructions {
        let operands =
            inst.operands.iter().map(|operand| terminal_operand_key(operand, &locals)).collect();
        let result = inst.result.map(|value| {
            let index = locals.len();
            locals.insert(value, index);
            index
        });
        instructions.push(TerminalInstructionKey { result, kind: inst.kind.clone(), operands });
    }

    let term = block.terminator.as_ref()?;
    Some(TerminalBlockKey {
        instructions,
        terminator: terminal_terminator_key(&term.kind, &locals),
    })
}

fn terminal_operand_key(
    operand: &EvmIrOperand,
    locals: &FxHashMap<EvmIrValueId, usize>,
) -> TerminalOperandKey {
    match operand {
        EvmIrOperand::Value(value) => locals
            .get(value)
            .copied()
            .map(TerminalOperandKey::LocalValue)
            .unwrap_or(TerminalOperandKey::ExternalValue(*value)),
        EvmIrOperand::Immediate(value) => TerminalOperandKey::Immediate(*value),
        EvmIrOperand::Block(block) => TerminalOperandKey::Block(*block),
        EvmIrOperand::Symbol(symbol) => TerminalOperandKey::Symbol(symbol.clone()),
    }
}

fn terminal_terminator_key(
    kind: &EvmIrTerminatorKind,
    locals: &FxHashMap<EvmIrValueId, usize>,
) -> TerminalTerminatorKey {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) => TerminalTerminatorKey::Fallthrough(*target),
        EvmIrTerminatorKind::FallthroughNext => TerminalTerminatorKey::FallthroughNext,
        EvmIrTerminatorKind::Jump(target) => TerminalTerminatorKey::Jump(*target),
        EvmIrTerminatorKind::Branch { condition, then_block, else_block } => {
            TerminalTerminatorKey::Branch {
                condition: terminal_operand_key(condition, locals),
                then_block: *then_block,
                else_block: *else_block,
            }
        }
        EvmIrTerminatorKind::Switch { value, default, cases } => TerminalTerminatorKey::Switch {
            value: terminal_operand_key(value, locals),
            default: *default,
            cases: cases
                .iter()
                .map(|(case, target)| (terminal_operand_key(case, locals), *target))
                .collect(),
        },
        EvmIrTerminatorKind::Return { offset, size } => TerminalTerminatorKey::Return {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        EvmIrTerminatorKind::Revert { offset, size } => TerminalTerminatorKey::Revert {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        EvmIrTerminatorKind::Stop => TerminalTerminatorKey::Stop,
        EvmIrTerminatorKind::Invalid => TerminalTerminatorKey::Invalid,
        EvmIrTerminatorKind::SelfDestruct { recipient } => TerminalTerminatorKey::SelfDestruct {
            recipient: terminal_operand_key(recipient, locals),
        },
        EvmIrTerminatorKind::RawOpcode(opcode) => TerminalTerminatorKey::RawOpcode(*opcode),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalBlockKey {
    instructions: Vec<TerminalInstructionKey>,
    terminator: TerminalTerminatorKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalInstructionKey {
    result: Option<usize>,
    kind: EvmIrInstructionKind,
    operands: Vec<TerminalOperandKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalTerminatorKey {
    Fallthrough(EvmIrBlockId),
    FallthroughNext,
    Jump(EvmIrBlockId),
    Branch {
        condition: TerminalOperandKey,
        then_block: EvmIrBlockId,
        else_block: EvmIrBlockId,
    },
    Switch {
        value: TerminalOperandKey,
        default: EvmIrBlockId,
        cases: Vec<(TerminalOperandKey, EvmIrBlockId)>,
    },
    Return {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Revert {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Stop,
    Invalid,
    SelfDestruct {
        recipient: TerminalOperandKey,
    },
    RawOpcode(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalOperandKey {
    LocalValue(usize),
    ExternalValue(EvmIrValueId),
    Immediate(U256),
    Block(EvmIrBlockId),
    Symbol(String),
}

fn remap_block_order(module: &mut EvmIrModule, order: &[EvmIrBlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    let mut remap = vec![EvmIrBlockId::from_usize(0); module.blocks.len()];
    let mut old_blocks: Vec<Option<EvmIrBlock>> =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect();
    let mut blocks = IndexVec::with_capacity(old_blocks.len());
    for &old_block in order {
        let block = old_blocks[old_block.index()]
            .take()
            .expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block.index()] = new_block;
    }
    debug_assert!(old_blocks.into_iter().all(|block| block.is_none()));
    module.blocks = blocks;
    module.entry_block = module.entry_block.map(|block| remap[block.index()]);
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            for operand in &mut inst.operands {
                remap_operand_blocks(operand, &remap);
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_operand_blocks(operand: &mut EvmIrOperand, remap: &[EvmIrBlockId]) {
    if let EvmIrOperand::Block(block) = operand {
        *block = remap[block.index()];
    }
}

fn remap_terminator_blocks(kind: &mut EvmIrTerminatorKind, remap: &[EvmIrBlockId]) {
    visit_terminator_targets_mut(kind, |target| *target = remap[target.index()]);
}

fn visit_terminator_targets_mut(
    kind: &mut EvmIrTerminatorKind,
    mut visit: impl FnMut(&mut EvmIrBlockId),
) {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) | EvmIrTerminatorKind::Jump(target) => {
            visit(target)
        }
        EvmIrTerminatorKind::Branch { then_block, else_block, .. } => {
            visit(then_block);
            visit(else_block);
        }
        EvmIrTerminatorKind::Switch { default, cases, .. } => {
            visit(default);
            for (_, target) in cases {
                visit(target);
            }
        }
        EvmIrTerminatorKind::Return { .. }
        | EvmIrTerminatorKind::Revert { .. }
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::SelfDestruct { .. }
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
}
