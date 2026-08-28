//! Late dead-value elimination over scheduled EVM IR.
//!
//! Stack scheduling can leave a duplicated value that survives through several physical stack
//! operations only to be discarded by `POP`. Starting from each `DUP`, this pass symbolically
//! follows the copied occurrence through the block. It removes that occurrence, retargets the
//! intervening `DUP`, `SWAP`, and `EXCHANGE` operations to the compressed stack, and accepts the
//! candidate when the omitted copy reaches its discard.
//!
//! The simulation stops when an ordinary instruction consumes the selected occurrence, when the
//! target cannot encode a required replacement stack operation, or when another live occurrence
//! cannot be reached. Candidate costs use the target's lowered stack operations. A rewrite must
//! improve bytes or static gas without making the other metric worse; size mode also disables
//! optional duplicate retargeting that can trade bytes for gas.
//!
//! This is a post-scheduling cleanup. It removes duplicated stack values within one block, then
//! removes a trailing pure stack computation before a halting terminal, including across an
//! unconditional edge to a block that never reads its incoming stack.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Block, BlockId, Instruction, Module, Terminator, TerminatorKind},
    op::{self, StackOp},
};
use solar_config::EvmVersion;
use solar_data_structures::index::IndexVec;
use solar_sema::Gcx;

pub(super) struct Dce;

impl EvmPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let copies_changed = eliminate_dead_stack_copies(
            module,
            !gcx.sess.opts.optimization.is_size(),
            gcx.sess.opts.evm_version,
        );
        cleanup_dead_stack_tails(module) || copies_changed
    }
}

fn cleanup_dead_stack_tails(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        if let Some(range) = block
            .terminator
            .as_ref()
            .and_then(|term| halting_terminal_tail_range(&block.instructions, term))
        {
            block.instructions.drain(range);
            changed = true;
        }
    }
    let ignored_entries =
        module.blocks.iter().map(block_ignores_entry_stack).collect::<IndexVec<BlockId, _>>();
    for block in &mut module.blocks {
        if let Some(TerminatorKind::Jump(target)) = block.terminator.as_ref().map(|term| &term.kind)
            && ignored_entries[*target]
            && let Some(start) = discardable_tail_start(&block.instructions)
        {
            block.instructions.truncate(start);
            changed = true;
        }
    }
    changed
}

fn block_ignores_entry_stack(block: &Block) -> bool {
    let Some(Terminator { kind, implicit_stop: false, .. }) = &block.terminator else {
        return false;
    };
    let Some((inputs, _)) = halting_stack_io(kind) else { return false };
    let mut depth = 0usize;
    for inst in &block.instructions {
        if !inst.has_canonical_stack_effect()
            || inst.as_evm_opcode().is_some_and(is_analysis_boundary)
        {
            return false;
        }
        let (inputs, outputs) = if let Some(stack_op) = inst.as_stack_op() {
            let inputs = stack_op.required_depth();
            let outputs = inputs.checked_add_signed(stack_op.net_growth()).unwrap();
            (inputs, outputs)
        } else if let Some(effect) = inst.effective_stack_effect() {
            (usize::from(effect.inputs), usize::from(effect.outputs))
        } else {
            return false;
        };
        if depth < inputs {
            return false;
        }
        depth = depth - inputs + outputs;
    }
    depth >= usize::from(inputs)
}

fn halting_terminal_tail_range(
    instructions: &[Instruction],
    terminator: &Terminator,
) -> Option<std::ops::Range<usize>> {
    if terminator.implicit_stop {
        return None;
    }
    let (inputs, _) = halting_stack_io(&terminator.kind)?;
    let operands = instructions.len().checked_sub(usize::from(inputs))?;
    if !instructions[operands..]
        .iter()
        .all(|inst| inst.is_encoded_push() && inst.has_canonical_stack_effect())
    {
        return None;
    }
    let start = discardable_tail_start(&instructions[..operands])?;
    Some(start..operands)
}

const fn halting_stack_io(kind: &TerminatorKind) -> Option<(u8, u8)> {
    let TerminatorKind::Op(
        opcode @ (op::STOP | op::INVALID | op::RETURN | op::REVERT | op::SELFDESTRUCT),
    ) = *kind
    else {
        return None;
    };
    op::stack_io(opcode)
}

fn discardable_tail_start(instructions: &[Instruction]) -> Option<usize> {
    let start = instructions
        .iter()
        .rposition(|inst| !is_discardable_tail_instruction(inst))
        .map_or(0, |index| index + 1);
    (start < instructions.len()).then_some(start)
}

fn is_discardable_tail_instruction(inst: &Instruction) -> bool {
    inst.has_canonical_stack_effect()
        && (inst.is_encoded_push()
            || inst.as_stack_op().is_some()
            || inst.as_evm_opcode().is_some_and(op::is_pure))
}

/// Removes stack copies that are eventually discarded without being consumed.
///
/// The pass symbolically executes each straight-line candidate while omitting
/// one occurrence introduced by a `DUPn`. Physical stack operations are
/// retargeted to the compressed stack, including rotations when a `SWAPn`
/// moves the omitted occurrence. Ordinary instructions remain safe as long as
/// they do not consume it. A rewrite is applied only when the omitted
/// occurrence reaches a `POP` and the resulting sequence improves either gas
/// or size without regressing the other.
fn eliminate_dead_stack_copies(
    module: &mut Module,
    allow_dup_retargeting: bool,
    evm_version: EvmVersion,
) -> bool {
    let mut changed = false;
    let mut edits = Vec::new();
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        changed |= eliminate_in_block(
            &mut block.instructions,
            &mut edits,
            &mut scratch,
            allow_dup_retargeting,
            evm_version,
        ) != 0;
    }
    changed
}

fn eliminate_in_block(
    instructions: &mut Vec<Instruction>,
    edits: &mut Vec<Edit>,
    scratch: &mut Vec<Instruction>,
    allow_dup_retargeting: bool,
    evm_version: EvmVersion,
) -> usize {
    let mut rewrites = 0;
    loop {
        edits.clear();
        let mut start = 0;
        while start < instructions.len() {
            let Some(StackOp::Dup(depth)) = instructions[start].as_stack_op() else {
                start += 1;
                continue;
            };
            if StackOp::Dup(depth).assembled_len(evm_version).is_none() {
                start += 1;
                continue;
            }

            let depth = usize::from(depth);
            let candidate = if depth == 1 {
                better_candidate(
                    find_candidate(
                        instructions,
                        start,
                        depth,
                        Ghost::Original,
                        allow_dup_retargeting,
                        evm_version,
                    ),
                    find_candidate(
                        instructions,
                        start,
                        depth,
                        Ghost::Duplicate,
                        allow_dup_retargeting,
                        evm_version,
                    ),
                )
            } else {
                find_candidate(
                    instructions,
                    start,
                    depth,
                    Ghost::Duplicate,
                    allow_dup_retargeting,
                    evm_version,
                )
            };
            let Some(candidate) = candidate else {
                start += 1;
                continue;
            };

            start = candidate.end;
            edits.extend(candidate.edits);
            rewrites += 1;
        }
        if edits.is_empty() {
            return rewrites;
        }

        apply_edits(instructions, edits, scratch);
    }
}

#[derive(Clone, Copy)]
enum Ghost {
    /// Omit the value produced by the candidate `DUPn`.
    Duplicate,
    /// For `DUP1`, omit the indistinguishable original value instead.
    Original,
}

#[derive(Clone, Copy)]
struct Slot {
    aliases_copy: bool,
    is_ghost: bool,
}

impl Slot {
    const OTHER: Self = Self { aliases_copy: false, is_ghost: false };
    const COPY: Self = Self { aliases_copy: true, is_ghost: false };
    const GHOST: Self = Self { aliases_copy: true, is_ghost: true };
}

struct Candidate {
    end: usize,
    edits: Vec<Edit>,
    old_cost: Cost,
    new_cost: Cost,
}

impl Candidate {
    fn new(start: usize, stack_op: StackOp, evm_version: EvmVersion) -> Self {
        let mut candidate = Self {
            end: start + 1,
            edits: Vec::new(),
            old_cost: Cost::default(),
            new_cost: Cost::default(),
        };
        candidate.replace(start, stack_op, Vec::new(), evm_version);
        candidate
    }

    fn replace(
        &mut self,
        index: usize,
        old: StackOp,
        replacement: Vec<StackOp>,
        evm_version: EvmVersion,
    ) {
        if replacement.as_slice() == [old] {
            return;
        }
        self.old_cost += Cost::of_stack_op(old, evm_version);
        for &stack_op in &replacement {
            self.new_cost += Cost::of_stack_op(stack_op, evm_version);
        }
        self.edits.push(Edit { index, replacement });
    }

    fn is_profitable(&self) -> bool {
        self.new_cost.size <= self.old_cost.size
            && self.new_cost.gas <= self.old_cost.gas
            && self.new_cost != self.old_cost
    }

    fn gas_savings(&self) -> usize {
        self.old_cost.gas - self.new_cost.gas
    }

    fn size_savings(&self) -> usize {
        self.old_cost.size - self.new_cost.size
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Cost {
    size: usize,
    gas: usize,
}

impl Cost {
    fn of_stack_op(stack_op: StackOp, evm_version: EvmVersion) -> Self {
        let metrics = stack_op.metrics(evm_version).unwrap();
        Self { size: metrics.assembled_len, gas: metrics.static_gas }
    }
}

impl std::ops::AddAssign for Cost {
    fn add_assign(&mut self, rhs: Self) {
        self.size += rhs.size;
        self.gas += rhs.gas;
    }
}

struct Edit {
    index: usize,
    replacement: Vec<StackOp>,
}

fn find_candidate(
    instructions: &[Instruction],
    start: usize,
    duplicate_depth: usize,
    ghost: Ghost,
    allow_dup_retargeting: bool,
    evm_version: EvmVersion,
) -> Option<Candidate> {
    let max_stack_access = evm_version.reachable_stack_depth();
    let mut slots = vec![Slot::OTHER; duplicate_depth];
    slots[0] = Slot::COPY;
    slots.push(Slot::GHOST);
    if matches!(ghost, Ghost::Original) {
        slots.swap(0, 1);
    }

    let start_op = instructions[start].as_stack_op()?;
    let mut candidate = Candidate::new(start, start_op, evm_version);

    let mut index = start + 1;
    while let Some(inst) = instructions.get(index) {
        let stack_op = inst.as_stack_op();
        match stack_op {
            Some(StackOp::Pop) if slots.last().is_some_and(|slot| slot.is_ghost) => {
                candidate.replace(index, StackOp::Pop, Vec::new(), evm_version);
                candidate.end = index + 1;
                return candidate.is_profitable().then_some(candidate);
            }
            Some(StackOp::Pop) => {
                slots.pop();
            }
            Some(StackOp::Dup(depth)) => {
                let depth = usize::from(depth);
                ensure_depth(&mut slots, depth);
                let selected = slots[slots.len() - depth];
                let physical_depth = if selected.is_ghost {
                    nearest_alias_depth(&slots, max_stack_access)?
                } else {
                    physical_depth(&slots, slots.len() - depth)
                };
                // Changing a `DUPn` depth is byte-neutral and can perturb the
                // later size-oriented layout. Gas mode keeps the substitution
                // because it unlocks profitable copy cleanup.
                if !allow_dup_retargeting && physical_depth != depth {
                    return None;
                }
                let replacement = StackOp::Dup(u8::try_from(physical_depth).ok()?);
                candidate.replace(index, StackOp::Dup(depth as u8), vec![replacement], evm_version);
                slots.push(Slot { aliases_copy: selected.aliases_copy, is_ghost: false });
            }
            Some(StackOp::Swap(depth)) => {
                let mut replacement = Vec::new();
                retarget_swap(&mut slots, depth, max_stack_access, &mut replacement)?;
                candidate.replace(index, StackOp::Swap(depth), replacement, evm_version);
            }
            Some(StackOp::Exchange(n, m)) => {
                let mut replacement = Vec::new();
                for depth in [n, m, n] {
                    retarget_swap(&mut slots, depth, max_stack_access, &mut replacement)?;
                }
                candidate.replace(index, StackOp::Exchange(n, m), replacement, evm_version);
            }
            None => {
                if !inst.has_canonical_stack_effect()
                    || inst.as_evm_opcode().is_some_and(is_analysis_boundary)
                {
                    return None;
                }
                let effect = inst.effective_stack_effect()?;
                let inputs = usize::from(effect.inputs);
                if inputs > slots.len()
                    || slots[slots.len() - inputs..].iter().any(|slot| slot.is_ghost)
                {
                    return None;
                }
                slots.truncate(slots.len() - inputs);
                slots.extend(std::iter::repeat_n(Slot::OTHER, usize::from(effect.outputs)));
            }
        }
        index += 1;
    }
    None
}

fn ensure_depth(slots: &mut Vec<Slot>, depth: usize) {
    if depth > slots.len() {
        slots.splice(..0, std::iter::repeat_n(Slot::OTHER, depth - slots.len()));
    }
}

fn physical_depth(slots: &[Slot], selected: usize) -> usize {
    slots[selected..].iter().filter(|slot| !slot.is_ghost).count()
}

fn nearest_alias_depth(slots: &[Slot], max_stack_access: usize) -> Option<usize> {
    let mut depth = 0;
    for slot in slots.iter().rev() {
        if slot.is_ghost {
            continue;
        }
        depth += 1;
        if slot.aliases_copy {
            return (depth <= max_stack_access).then_some(depth);
        }
    }
    None
}

fn retarget_swap(
    slots: &mut Vec<Slot>,
    depth: u8,
    max_stack_access: usize,
    replacement: &mut Vec<StackOp>,
) -> Option<()> {
    let stack_depth = usize::from(depth) + 1;
    ensure_depth(slots, stack_depth);
    let top = slots.len() - 1;
    let selected = slots.len() - stack_depth;

    let selected_is_ghost = slots[selected].is_ghost;
    let top_is_ghost = slots[top].is_ghost;
    if !selected_is_ghost && !top_is_ghost {
        let depth = physical_depth(slots, selected);
        if !(2..=max_stack_access + 1).contains(&depth) {
            return None;
        }
        push_simplified_stack_op(replacement, StackOp::Swap(u8::try_from(depth - 1).unwrap()));
    } else {
        let live = slots[selected..=top].iter().filter(|slot| !slot.is_ghost).count();
        if live > max_stack_access {
            return None;
        }
        if selected_is_ghost {
            for depth in (1..live).rev() {
                push_simplified_stack_op(replacement, StackOp::Swap(u8::try_from(depth).unwrap()));
            }
        } else {
            for depth in 1..live {
                push_simplified_stack_op(replacement, StackOp::Swap(u8::try_from(depth).unwrap()));
            }
        }
    }
    slots.swap(selected, top);
    Some(())
}

fn push_simplified_stack_op(ops: &mut Vec<StackOp>, stack_op: StackOp) {
    if ops.last() == Some(&stack_op) && matches!(stack_op, StackOp::Swap(_)) {
        ops.pop();
        return;
    }
    ops.push(stack_op);
    if let [.., StackOp::Swap(first), StackOp::Swap(second), StackOp::Swap(third)] = ops.as_slice()
        && let Some(exchange) = StackOp::from_swaps(*first, *second, *third)
    {
        ops.truncate(ops.len() - 3);
        ops.push(exchange);
    }
}

fn better_candidate(left: Option<Candidate>, right: Option<Candidate>) -> Option<Candidate> {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left_score = (left.gas_savings(), left.size_savings());
            let right_score = (right.gas_savings(), right.size_savings());
            Some(if right_score > left_score { right } else { left })
        }
        (candidate @ Some(_), None) | (None, candidate @ Some(_)) => candidate,
        (None, None) => None,
    }
}

fn apply_edits(
    instructions: &mut Vec<Instruction>,
    edits: &[Edit],
    scratch: &mut Vec<Instruction>,
) {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    let new_len = scratch.len() + edits.iter().map(|edit| edit.replacement.len()).sum::<usize>()
        - edits.len();
    instructions.reserve(new_len);

    let mut edits = edits.iter().peekable();
    for (index, inst) in scratch.drain(..).enumerate() {
        if edits.peek().is_some_and(|edit| edit.index == index) {
            let edit = edits.next().unwrap();
            instructions.extend(edit.replacement.iter().copied().map(|stack_op| {
                let mut replacement = Instruction::stack_op(stack_op);
                replacement.metadata.set_source_spans(inst.metadata.source_spans().iter().copied());
                replacement
            }));
        } else {
            instructions.push(inst);
        }
    }
}

const fn is_analysis_boundary(opcode: u8) -> bool {
    op::is_terminal(opcode)
        || matches!(
            opcode,
            op::JUMPI
                | op::RJUMP
                | op::RJUMPI
                | op::RJUMPV
                | op::CALLF
                | op::RETF
                | op::JUMPF
                // Reject malformed raw extended operations. Logical operations are handled above.
                | op::DUPN
                | op::SWAPN
                | op::EXCHANGE
                | op::RETURNCONTRACT
        )
}
