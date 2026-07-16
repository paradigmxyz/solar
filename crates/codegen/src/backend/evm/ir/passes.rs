//! EVM IR optimization and layout passes.

use super::*;
use crate::{
    backend::evm::{assembler::op, ir_stack_schedule},
    timing::PassTimer,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

type PassRunner = fn(&mut Module) -> bool;

/// Registry entry for an EVM IR transform pass.
#[derive(Clone, Copy, Debug)]
pub struct PassInfo {
    /// Command-line and pipeline name.
    pub name: &'static str,
    /// Human-readable help text.
    pub description: &'static str,
    run_pass: PassRunner,
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, run_pass: PassRunner) -> Self {
        Self { name, description, run_pass }
    }
}

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident -> $name:literal = $run_pass:path;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: PassInfo = PassInfo::new(
                $name,
                concat!($($description, "\n"),+).trim_ascii(),
                $run_pass,
            );
        )+
    };
}

declare_passes! {
    /// Materialize virtual instruction operands with physical stack operations.
    pub const STACK_SCHEDULE_PASS -> "stack-schedule" = ir_stack_schedule::schedule_stack_ops;

    /// Move cold terminal blocks after hot code without changing control flow.
    pub const COLD_LAYOUT_PASS -> "cold-layout" = move_cold_terminal_blocks;

    /// Replace duplicate terminal block bodies with jumps to the first copy when profitable.
    pub const TERMINAL_DEDUP_PASS -> "terminal-dedup" = deduplicate_terminal_blocks;

    /// Reorder blocks to maximize jumps assembled as physical fallthroughs.
    pub const BLOCK_LAYOUT_PASS -> "block-layout" = layout_blocks_for_fallthrough;
}

/// Options for running an EVM IR pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassOptions {
    /// Print the time spent in the pass.
    pub time_passes: bool,
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const PASS_REGISTRY: &[PassInfo] =
    &[STACK_SCHEDULE_PASS, COLD_LAYOUT_PASS, TERMINAL_DEDUP_PASS, BLOCK_LAYOUT_PASS];

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
pub const DEFAULT_LAYOUT_PIPELINE: &[PassInfo] =
    &[COLD_LAYOUT_PASS, TERMINAL_DEDUP_PASS, BLOCK_LAYOUT_PASS];

/// Finds a pass in the EVM IR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static PassInfo> {
    PASS_REGISTRY.iter().find(|pass| pass.name == name)
}

/// Runs a named EVM IR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo, options: PassOptions) -> bool {
    let timer = PassTimer::new(options.time_passes);
    let changed = (pass.run_pass)(module);
    timer.finish("EVM IR", &module.name, pass.name, changed);
    changed
}

fn layout_blocks_for_fallthrough(module: &mut Module) -> bool {
    let mut predecessor_counts = vec![0usize; module.blocks.len()];
    for block in &module.blocks {
        if let Some(target) = layout_successor(block)
            && target.index() < predecessor_counts.len()
        {
            predecessor_counts[target.index()] += 1;
        }
    }

    let mut order = Vec::with_capacity(module.blocks.len());
    let mut placed = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.entry_block {
        append_layout_trace(module, entry, &mut placed, &mut order);
    }
    for block in module.blocks.indices() {
        if predecessor_counts[block.index()] == 0 {
            append_layout_trace(module, block, &mut placed, &mut order);
        }
    }
    for block in module.blocks.indices() {
        append_layout_trace(module, block, &mut placed, &mut order);
    }

    if order.iter().copied().eq(module.blocks.indices()) {
        return false;
    }
    remap_block_order(module, &order);
    true
}

fn append_layout_trace(
    module: &Module,
    mut block: BlockId,
    placed: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    while block.index() < module.blocks.len() && placed.insert(block) {
        order.push(block);
        let Some(target) = layout_successor(&module.blocks[block]) else { return };
        block = target;
    }
}

fn layout_successor(block: &Block) -> Option<BlockId> {
    match &block.terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Some(*target),
        _ => None,
    }
}

fn move_cold_terminal_blocks(module: &mut Module) -> bool {
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

fn is_movable_cold_terminal_block(module: &Module, block_id: BlockId, block: &Block) -> bool {
    if module.entry_block == Some(block_id) || block_id.index() == 0 {
        return false;
    }
    let Some(term) = &block.terminator else {
        return false;
    };
    if !block.metadata.hotness.is_cold() || !is_evm_terminal(&term.kind) {
        return false;
    }
    let previous = BlockId::from_usize(block_id.index() - 1);
    module.blocks[previous].terminator.as_ref().is_some_and(|term| is_layout_barrier(&term.kind))
}

fn is_layout_barrier(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::Jump(_)) || is_evm_terminal(kind)
}

fn deduplicate_terminal_blocks(module: &mut Module) -> bool {
    let mut canonical = Vec::<(TerminalBlockKey, BlockId)>::new();
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
                Some(Terminator::new(TerminatorKind::Jump(*target)));
            changed = true;
        } else {
            canonical.push((key, block_id));
        }
    }

    changed
}

fn terminal_block_dedup_is_profitable(block: &Block) -> bool {
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

fn estimated_instruction_size(inst: &Instruction) -> usize {
    match &inst.kind {
        InstructionKind::Stack(_) => 1,
        InstructionKind::Operation(mnemonic) if mnemonic == "push" => {
            match inst.operands.as_slice() {
                [operand] => estimated_push_size(operand),
                _ => 1,
            }
        }
        InstructionKind::Operation(mnemonic) if mnemonic == "push_immutable" => 33,
        InstructionKind::Operation(_) => 1,
    }
}

fn estimated_terminator_size(kind: &TerminatorKind) -> usize {
    let operand_pushes = |operands: &[&Operand]| {
        operands.iter().map(|operand| estimated_push_size(operand)).sum::<usize>() + 1
    };
    match kind {
        TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
            operand_pushes(&[offset, size])
        }
        TerminatorKind::SelfDestruct { recipient } => operand_pushes(&[recipient]),
        TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::RawOpcode(_) => 1,
        TerminatorKind::Continue
        | TerminatorKind::Jump(_)
        | TerminatorKind::Branch { .. }
        | TerminatorKind::Switch { .. } => 0,
    }
}

fn estimated_push_size(operand: &Operand) -> usize {
    match operand {
        Operand::Immediate(value) if *value == U256::ZERO => 1,
        Operand::Immediate(value) => value.byte_len() + 1,
        Operand::Block(_) | Operand::Symbol(_) => 3,
        Operand::Value(_) => 0,
    }
}

fn is_evm_terminal(kind: &TerminatorKind) -> bool {
    matches!(
        kind,
        TerminatorKind::Return { .. }
            | TerminatorKind::Revert { .. }
            | TerminatorKind::Stop
            | TerminatorKind::Invalid
            | TerminatorKind::SelfDestruct { .. }
    ) || matches!(kind, TerminatorKind::RawOpcode(opcode) if op::is_terminal(*opcode))
}

fn terminal_block_key(block: &Block) -> Option<TerminalBlockKey> {
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
    operand: &Operand,
    locals: &FxHashMap<ValueId, usize>,
) -> TerminalOperandKey {
    match operand {
        Operand::Value(value) => locals
            .get(value)
            .copied()
            .map(TerminalOperandKey::LocalValue)
            .unwrap_or(TerminalOperandKey::ExternalValue(*value)),
        Operand::Immediate(value) => TerminalOperandKey::Immediate(*value),
        Operand::Block(block) => TerminalOperandKey::Block(*block),
        Operand::Symbol(symbol) => TerminalOperandKey::Symbol(symbol.clone()),
    }
}

fn terminal_terminator_key(
    kind: &TerminatorKind,
    locals: &FxHashMap<ValueId, usize>,
) -> TerminalTerminatorKey {
    match kind {
        TerminatorKind::Continue => TerminalTerminatorKey::Continue,
        TerminatorKind::Jump(target) => TerminalTerminatorKey::Jump(*target),
        TerminatorKind::Branch { condition, then_block, else_block } => {
            TerminalTerminatorKey::Branch {
                condition: terminal_operand_key(condition, locals),
                then_block: *then_block,
                else_block: *else_block,
            }
        }
        TerminatorKind::Switch { value, default, cases } => TerminalTerminatorKey::Switch {
            value: terminal_operand_key(value, locals),
            default: *default,
            cases: cases
                .iter()
                .map(|(case, target)| (terminal_operand_key(case, locals), *target))
                .collect(),
        },
        TerminatorKind::Return { offset, size } => TerminalTerminatorKey::Return {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        TerminatorKind::Revert { offset, size } => TerminalTerminatorKey::Revert {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        TerminatorKind::Stop => TerminalTerminatorKey::Stop,
        TerminatorKind::Invalid => TerminalTerminatorKey::Invalid,
        TerminatorKind::SelfDestruct { recipient } => TerminalTerminatorKey::SelfDestruct {
            recipient: terminal_operand_key(recipient, locals),
        },
        TerminatorKind::RawOpcode(opcode) => TerminalTerminatorKey::RawOpcode(*opcode),
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
    kind: InstructionKind,
    operands: Vec<TerminalOperandKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalTerminatorKey {
    Continue,
    Jump(BlockId),
    Branch {
        condition: TerminalOperandKey,
        then_block: BlockId,
        else_block: BlockId,
    },
    Switch {
        value: TerminalOperandKey,
        default: BlockId,
        cases: Vec<(TerminalOperandKey, BlockId)>,
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
    ExternalValue(ValueId),
    Immediate(U256),
    Block(BlockId),
    Symbol(String),
}

fn remap_block_order(module: &mut Module, order: &[BlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    let mut remap = vec![BlockId::from_usize(0); module.blocks.len()];
    let mut old_blocks: Vec<Option<Block>> =
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

fn remap_operand_blocks(operand: &mut Operand, remap: &[BlockId]) {
    if let Operand::Block(block) = operand {
        *block = remap[block.index()];
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &[BlockId]) {
    visit_terminator_targets_mut(kind, |target| *target = remap[target.index()]);
}

fn visit_terminator_targets_mut(kind: &mut TerminatorKind, mut visit: impl FnMut(&mut BlockId)) {
    match kind {
        TerminatorKind::Jump(target) => visit(target),
        TerminatorKind::Branch { then_block, else_block, .. } => {
            visit(then_block);
            visit(else_block);
        }
        TerminatorKind::Switch { default, cases, .. } => {
            visit(default);
            for (_, target) in cases {
                visit(target);
            }
        }
        TerminatorKind::Return { .. }
        | TerminatorKind::Revert { .. }
        | TerminatorKind::Continue
        | TerminatorKind::Stop
        | TerminatorKind::Invalid
        | TerminatorKind::SelfDestruct { .. }
        | TerminatorKind::RawOpcode(_) => {}
    }
}
