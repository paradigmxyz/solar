//! Late dead-value elimination over scheduled EVM IR.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, default_instruction_stack_effect},
    op::{self, StackOp},
};
use solar_config::EvmVersion;
use solar_sema::Gcx;

pub(super) struct Dce;

impl EvmPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        eliminate_dead_stack_copies(
            module,
            !gcx.sess.opts.optimization.is_size(),
            gcx.sess.opts.evm_version,
        )
    }
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
            let Some(StackOp::Dup(depth)) = stack_op(&instructions[start]) else {
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

    let start_op = stack_op(&instructions[start])?;
    let mut candidate = Candidate::new(start, start_op, evm_version);

    let mut index = start + 1;
    while let Some(inst) = instructions.get(index) {
        let stack_op = stack_op(inst);
        if stack_op.is_some_and(|stack_op| stack_op.assembled_len(evm_version).is_none()) {
            return None;
        }
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
                replacement.assembled_len(evm_version)?;
                candidate.replace(index, StackOp::Dup(depth as u8), vec![replacement], evm_version);
                slots.push(Slot { aliases_copy: selected.aliases_copy, is_ghost: false });
            }
            Some(StackOp::Swap(depth)) => {
                let stack_depth = usize::from(depth) + 1;
                ensure_depth(&mut slots, stack_depth);
                let top = slots.len() - 1;
                let selected = slots.len() - stack_depth;
                let replacement = swap_replacement(&slots, selected, top, max_stack_access)?;
                if replacement.iter().any(|op| op.assembled_len(evm_version).is_none()) {
                    return None;
                }
                candidate.replace(index, StackOp::Swap(depth), replacement, evm_version);
                slots.swap(selected, top);
            }
            Some(StackOp::Exchange(..)) => return None,
            None => {
                if inst.as_legacy_opcode().is_some_and(is_analysis_boundary) {
                    return None;
                }
                let effect =
                    inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))?;
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

fn swap_replacement(
    slots: &[Slot],
    selected: usize,
    top: usize,
    max_stack_access: usize,
) -> Option<Vec<StackOp>> {
    let selected_is_ghost = slots[selected].is_ghost;
    let top_is_ghost = slots[top].is_ghost;
    if !selected_is_ghost && !top_is_ghost {
        let depth = physical_depth(slots, selected);
        return (2..=max_stack_access + 1)
            .contains(&depth)
            .then(|| vec![StackOp::Swap(u8::try_from(depth - 1).unwrap())]);
    }

    let live = slots[selected..=top].iter().filter(|slot| !slot.is_ghost).count();
    if live > max_stack_access {
        return None;
    }
    let mut replacement = Vec::with_capacity(live.saturating_sub(1));
    if selected_is_ghost {
        replacement
            .extend((1..live).rev().map(|depth| StackOp::Swap(u8::try_from(depth).unwrap())));
    } else {
        replacement.extend((1..live).map(|depth| StackOp::Swap(u8::try_from(depth).unwrap())));
    }
    Some(replacement)
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
            instructions.extend(edit.replacement.iter().copied().map(Instruction::stack_op));
        } else {
            instructions.push(inst);
        }
    }
}

fn stack_op(inst: &Instruction) -> Option<StackOp> {
    inst.as_stack_op()
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
