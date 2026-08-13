//! Late dead-value elimination over scheduled EVM IR.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, default_instruction_stack_effect},
    op,
};
use solar_sema::Gcx;

pub(super) struct Dce;

impl EvmPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        eliminate_dead_stack_copies(module, !gcx.sess.opts.optimization.is_size())
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
fn eliminate_dead_stack_copies(module: &mut Module, allow_dup_retargeting: bool) -> bool {
    let mut changed = false;
    let mut edits = Vec::new();
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        changed |= eliminate_in_block(
            &mut block.instructions,
            &mut edits,
            &mut scratch,
            allow_dup_retargeting,
        ) != 0;
    }
    changed
}

fn eliminate_in_block(
    instructions: &mut Vec<Instruction>,
    edits: &mut Vec<Edit>,
    scratch: &mut Vec<Instruction>,
    allow_dup_retargeting: bool,
) -> usize {
    let mut rewrites = 0;
    loop {
        edits.clear();
        let mut start = 0;
        while start < instructions.len() {
            let Some(opcode) = raw_opcode(&instructions[start]) else {
                start += 1;
                continue;
            };
            if !(op::DUP1..=op::DUP16).contains(&opcode) {
                start += 1;
                continue;
            }

            let depth = usize::from(opcode - op::DUP1 + 1);
            let candidate = if depth == 1 {
                better_candidate(
                    find_candidate(
                        instructions,
                        start,
                        depth,
                        Ghost::Original,
                        allow_dup_retargeting,
                    ),
                    find_candidate(
                        instructions,
                        start,
                        depth,
                        Ghost::Duplicate,
                        allow_dup_retargeting,
                    ),
                )
            } else {
                find_candidate(instructions, start, depth, Ghost::Duplicate, allow_dup_retargeting)
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
    fn new(start: usize, opcode: u8) -> Self {
        let mut candidate = Self {
            end: start + 1,
            edits: Vec::new(),
            old_cost: Cost::default(),
            new_cost: Cost::default(),
        };
        candidate.replace(start, opcode, &[]);
        candidate
    }

    fn replace(&mut self, index: usize, old: u8, replacement: &[u8]) {
        if replacement == [old] {
            return;
        }
        self.old_cost += Cost::of_stack_op(old);
        for &opcode in replacement {
            self.new_cost += Cost::of_stack_op(opcode);
        }
        self.edits.push(Edit { index, replacement: replacement.to_vec() });
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
    fn of_stack_op(opcode: u8) -> Self {
        debug_assert!(
            opcode == op::POP
                || (op::DUP1..=op::DUP16).contains(&opcode)
                || (op::SWAP1..=op::SWAP16).contains(&opcode)
        );
        Self { size: 1, gas: if opcode == op::POP { 2 } else { 3 } }
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
    replacement: Vec<u8>,
}

fn find_candidate(
    instructions: &[Instruction],
    start: usize,
    duplicate_depth: usize,
    ghost: Ghost,
    allow_dup_retargeting: bool,
) -> Option<Candidate> {
    let mut slots = vec![Slot::OTHER; duplicate_depth];
    slots[0] = Slot::COPY;
    slots.push(Slot::GHOST);
    if matches!(ghost, Ghost::Original) {
        slots.swap(0, 1);
    }

    let start_opcode = raw_opcode(&instructions[start])?;
    let mut candidate = Candidate::new(start, start_opcode);

    let mut index = start + 1;
    while let Some(inst) = instructions.get(index) {
        let opcode = raw_opcode(inst);
        match opcode {
            Some(op::POP) if slots.last().is_some_and(|slot| slot.is_ghost) => {
                candidate.replace(index, op::POP, &[]);
                candidate.end = index + 1;
                return candidate.is_profitable().then_some(candidate);
            }
            Some(op::POP) => {
                slots.pop();
            }
            Some(opcode) if is_analysis_boundary(opcode) => return None,
            Some(opcode) if (op::DUP1..=op::DUP16).contains(&opcode) => {
                let depth = usize::from(opcode - op::DUP1 + 1);
                ensure_depth(&mut slots, depth);
                let selected = slots[slots.len() - depth];
                let physical_depth = if selected.is_ghost {
                    nearest_alias_depth(&slots)?
                } else {
                    physical_depth(&slots, slots.len() - depth)
                };
                // Changing a `DUPn` depth is byte-neutral and can perturb the
                // later size-oriented layout. Gas mode keeps the substitution
                // because it unlocks profitable copy cleanup.
                if !allow_dup_retargeting && physical_depth != depth {
                    return None;
                }
                let replacement = op::dup(u8::try_from(physical_depth).ok()?);
                candidate.replace(index, opcode, &[replacement]);
                slots.push(Slot { aliases_copy: selected.aliases_copy, is_ghost: false });
            }
            Some(opcode) if (op::SWAP1..=op::SWAP16).contains(&opcode) => {
                let depth = usize::from(opcode - op::SWAP1 + 2);
                ensure_depth(&mut slots, depth);
                let top = slots.len() - 1;
                let selected = slots.len() - depth;
                let replacement = swap_replacement(&slots, selected, top)?;
                candidate.replace(index, opcode, &replacement);
                slots.swap(selected, top);
            }
            _ => {
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

fn nearest_alias_depth(slots: &[Slot]) -> Option<usize> {
    let mut depth = 0;
    for slot in slots.iter().rev() {
        if slot.is_ghost {
            continue;
        }
        depth += 1;
        if slot.aliases_copy {
            return (depth <= 16).then_some(depth);
        }
    }
    None
}

fn swap_replacement(slots: &[Slot], selected: usize, top: usize) -> Option<Vec<u8>> {
    let selected_is_ghost = slots[selected].is_ghost;
    let top_is_ghost = slots[top].is_ghost;
    if !selected_is_ghost && !top_is_ghost {
        let depth = physical_depth(slots, selected);
        return (2..=17).contains(&depth).then(|| vec![op::swap(u8::try_from(depth - 1).unwrap())]);
    }

    let live = slots[selected..=top].iter().filter(|slot| !slot.is_ghost).count();
    if live > 16 {
        return None;
    }
    let mut replacement = Vec::with_capacity(live.saturating_sub(1));
    if selected_is_ghost {
        replacement.extend((1..live).rev().map(|depth| op::swap(u8::try_from(depth).unwrap())));
    } else {
        replacement.extend((1..live).map(|depth| op::swap(u8::try_from(depth).unwrap())));
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
            instructions.extend(edit.replacement.iter().copied().map(Instruction::opcode));
        } else {
            instructions.push(inst);
        }
    }
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
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
                // EOF extended stack operations access or rearrange words selected by their
                // immediate operand. EVM IR does not model that selection yet, so a ghost copy
                // cannot be tracked safely across them from their net stack effect alone.
                | op::DUPN
                | op::SWAPN
                | op::EXCHANGE
                | op::RETURNCONTRACT
        )
}
