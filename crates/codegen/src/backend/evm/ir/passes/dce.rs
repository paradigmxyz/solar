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
//! improve bytes or static gas without making the other metric worse. Duplicate retargeting also
//! runs in size mode, where it shrank runtime code across the benchmark corpus with one contract
//! growing by one byte.
//!
//! This is a post-scheduling cleanup. It removes duplicated stack values within one block, then
//! removes a trailing stack computation before a halting terminal, including across an
//! unconditional edge to a block that never reads its incoming stack. The computation may use pure
//! operations and reads of the call's context, whose only effect is their static gas.
//!
//! Removing a tail that pops more words than it pushes leaves the code after it deeper, which can
//! exceed `MAX_STACK_DEPTH`. The input is valid, so every depth its path reaches fits. A removal
//! is safe when the new path, with its terminator's label push and a jump target's code, never
//! rises above the highest depth the old path reaches from the same entry. Otherwise the entry
//! bound from `Verifier::stack_entry_upper_bounds` must show that the new path fits. When neither
//! holds, the pass removes the longest suffix of the tail that passes, or none. The bounds stay
//! valid across the pass: an edit changes the entry of a halting block only, and no check reads
//! the bound of a halting block after an edit to its predecessor.

use super::{EvmPass, block_layout::terminator_kind};
use crate::backend::evm::{
    codegen::MAX_STACK_DEPTH,
    ir::{Block, BlockId, Instruction, Module, Terminator, TerminatorKind, verify::Verifier},
    op::{self, StackOp},
};
use smallvec::{SmallVec, smallvec};
use solar_config::EvmVersion;
use solar_data_structures::index::IndexVec;
use solar_sema::Gcx;
use std::{cell::OnceCell, ops::Range};

pub(super) struct Dce;

impl EvmPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let copies_changed = eliminate_dead_stack_copies(module, gcx.sess.opts.evm_version);
        cleanup_dead_stack_tails(module, gcx.sess.opts.evm_version) || copies_changed
    }
}

fn cleanup_dead_stack_tails(module: &mut Module, evm_version: EvmVersion) -> bool {
    let mut changed = false;
    let entries = OnceCell::new();
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if let Some(tail) = block
            .terminator
            .as_ref()
            .and_then(|term| halting_terminal_tail_range(&block.instructions, term))
            && let Some(removed) = longest_removal(module, &entries, evm_version, block_id, tail, 0)
        {
            module.blocks[block_id].instructions.drain(removed);
            changed = true;
        }
    }
    let ignored_entries =
        module.blocks.iter().map(block_ignores_entry_stack).collect::<IndexVec<BlockId, _>>();
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if let Some(kind @ TerminatorKind::Jump(target)) = terminator_kind(block)
            && ignored_entries[*target]
            && let Some(start) = discardable_tail_start(&block.instructions)
            && let tail = (start..block.instructions.len())
            && let after = path_peak(&module.blocks[*target].instructions, &(0..0), 0)
                .max(kind.lowering_stack_growth(module.next_block(block_id)))
            && let Some(removed) =
                longest_removal(module, &entries, evm_version, block_id, tail, after)
        {
            module.blocks[block_id].instructions.truncate(removed.start);
            changed = true;
        }
    }
    changed
}

/// The words the instruction leaves on the stack, less the words it takes.
fn net_growth(inst: &Instruction) -> isize {
    let effect = inst.stack_effect();
    isize::from(effect.outputs) - isize::from(effect.inputs)
}

/// The most words the instructions without `removed` leave above the stack they start with, or
/// that `after` more words reach above their end.
fn path_peak(instructions: &[Instruction], removed: &Range<usize>, after: usize) -> usize {
    let mut depth = 0isize;
    let mut peak = 0isize;
    for (index, inst) in instructions.iter().enumerate() {
        if !removed.contains(&index) {
            depth += net_growth(inst);
            peak = peak.max(depth);
        }
    }
    peak.max(depth + after as isize) as usize
}

/// Returns the longest suffix of `tail` that a block can lose and still fit within
/// `MAX_STACK_DEPTH`, if any, given that `after` more words rise above the block's end.
fn longest_removal(
    module: &Module,
    entries: &OnceCell<IndexVec<BlockId, Option<usize>>>,
    evm_version: EvmVersion,
    block_id: BlockId,
    tail: Range<usize>,
    after: usize,
) -> Option<Range<usize>> {
    let instructions = &module.blocks[block_id].instructions;
    let old_peak = path_peak(instructions, &(0..0), after);
    (tail.start..tail.end)
        .find_map(|start| {
            let removed = start..tail.end;
            // The new path stays at or under the old peak, so it fits wherever the old path does.
            // Otherwise the entry bound locates the stack at `start`, which the removal leaves as
            // it was.
            let fits = path_peak(instructions, &removed, after) <= old_peak || {
                let bounds =
                    entries.get_or_init(|| Verifier::stack_entry_upper_bounds(module, evm_version));
                bounds[block_id].is_some_and(|entry| {
                    let depth = instructions[..start].iter().fold(entry, |depth, inst| {
                        let effect = inst.stack_effect();
                        depth.saturating_sub(usize::from(effect.inputs))
                            + usize::from(effect.outputs)
                    });
                    let rest = path_peak(&instructions[tail.end..], &(0..0), after);
                    depth + rest <= MAX_STACK_DEPTH
                })
            };
            fits.then_some(removed)
        })
        .filter(|removed| !removed.is_empty())
}

fn block_ignores_entry_stack(block: &Block) -> bool {
    let Some(operands) = block.terminator.as_ref().and_then(halting_inputs) else { return false };
    let mut depth = 0usize;
    for inst in &block.instructions {
        if inst.as_evm_opcode().is_some_and(is_analysis_boundary) {
            return false;
        }
        let effect = inst.stack_effect();
        let (inputs, outputs) = (usize::from(effect.inputs), usize::from(effect.outputs));
        if depth < inputs {
            return false;
        }
        depth = depth - inputs + outputs;
    }
    depth >= operands
}

fn halting_terminal_tail_range(
    instructions: &[Instruction],
    terminator: &Terminator,
) -> Option<Range<usize>> {
    let operands = instructions.len().checked_sub(halting_inputs(terminator)?)?;
    if !instructions[operands..].iter().all(Instruction::is_encoded_push) {
        return None;
    }
    let start = discardable_tail_start(&instructions[..operands])?;
    Some(start..operands)
}

/// The operand count of a halting terminal opcode, which every one but a computed `JUMP` is.
fn halting_inputs(terminator: &Terminator) -> Option<usize> {
    let TerminatorKind::Op(opcode) = terminator.kind else { return None };
    if terminator.implicit_stop || opcode == op::JUMP {
        return None;
    }
    op::stack_io(opcode).map(|(inputs, _)| usize::from(inputs))
}

fn discardable_tail_start(instructions: &[Instruction]) -> Option<usize> {
    let start = instructions
        .iter()
        .rposition(|inst| !is_discardable_tail_instruction(inst))
        .map_or(0, |index| index + 1);
    (start < instructions.len()).then_some(start)
}

fn is_discardable_tail_instruction(inst: &Instruction) -> bool {
    inst.is_encoded_push()
        || inst.as_stack_op().is_some()
        || inst
            .as_evm_opcode()
            .is_some_and(|opcode| op::is_pure(opcode) || op::is_context_read(opcode))
}

/// Removes stack copies that are eventually discarded without being consumed.
fn eliminate_dead_stack_copies(module: &mut Module, evm_version: EvmVersion) -> bool {
    let mut changed = false;
    let mut edits = Vec::new();
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        changed |=
            eliminate_in_block(&mut block.instructions, &mut edits, &mut scratch, evm_version);
    }
    changed
}

fn eliminate_in_block(
    instructions: &mut Vec<Instruction>,
    edits: &mut Vec<Edit>,
    scratch: &mut Vec<Instruction>,
    evm_version: EvmVersion,
) -> bool {
    let mut changed = false;
    loop {
        edits.clear();
        // A candidate ends at a `POP`, so no candidate starts at or after the last one, and
        // the walk never needs to look past it.
        let Some(last_pop) =
            instructions.iter().rposition(|inst| inst.as_stack_op() == Some(StackOp::Pop))
        else {
            return changed;
        };
        let walked = &instructions[..=last_pop];
        let mut start = 0;
        while start < last_pop {
            if let Some(StackOp::Dup(depth)) = instructions[start].as_stack_op()
                && StackOp::Dup(depth).assembled_len(evm_version).is_some()
                // The last maximum wins a tie, so the original value is the preferred ghost.
                && let Some(candidate) = [Ghost::Duplicate, Ghost::Original]
                    .into_iter()
                    .filter_map(|ghost| find_candidate(walked, start, depth, ghost, evm_version))
                    .max_by_key(|candidate| (candidate.gas_saved, candidate.size_saved))
            {
                start = candidate.end;
                edits.extend(candidate.edits);
            } else {
                start += 1;
            }
        }
        if edits.is_empty() {
            return changed;
        }

        changed = true;
        apply_edits(instructions, edits, scratch);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ghost {
    /// Omit the value produced by the candidate `DUPn`.
    Duplicate,
    /// For `DUP1`, omit the indistinguishable original value instead.
    Original,
}

/// The symbolic stack of a candidate, bottom first.
type Slots = SmallVec<[Slot; 64]>;

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
    evm_version: EvmVersion,
    end: usize,
    edits: SmallVec<[Edit; 4]>,
    /// Static gas and bytes the edits save, negative when they cost more.
    gas_saved: isize,
    size_saved: isize,
}

impl Candidate {
    fn new(start: usize, depth: u8, evm_version: EvmVersion) -> Self {
        let mut candidate = Self {
            evm_version,
            end: start + 1,
            edits: SmallVec::new(),
            gas_saved: 0,
            size_saved: 0,
        };
        candidate.replace(start, StackOp::Dup(depth), Replacement::new());
        candidate
    }

    fn replace(&mut self, index: usize, old: StackOp, replacement: Replacement) {
        if replacement.as_slice() == [old] {
            return;
        }
        let evm_version = self.evm_version;
        let cost = |stack_op: StackOp| {
            let metrics = stack_op.metrics(evm_version).unwrap();
            (metrics.static_gas as isize, metrics.assembled_len as isize)
        };
        let (gas, size) = cost(old);
        self.gas_saved += gas;
        self.size_saved += size;
        for &stack_op in &replacement {
            let (gas, size) = cost(stack_op);
            self.gas_saved -= gas;
            self.size_saved -= size;
        }
        self.edits.push(Edit { index, replacement });
    }

    /// A rewrite must improve bytes or static gas without making the other worse.
    fn is_profitable(&self) -> bool {
        self.gas_saved >= 0 && self.size_saved >= 0 && self.gas_saved + self.size_saved > 0
    }
}

/// The stack operations replacing one instruction, usually at most one.
type Replacement = SmallVec<[StackOp; 4]>;

struct Edit {
    index: usize,
    replacement: Replacement,
}

fn find_candidate(
    instructions: &[Instruction],
    start: usize,
    duplicate_depth: u8,
    ghost: Ghost,
    evm_version: EvmVersion,
) -> Option<Candidate> {
    if ghost == Ghost::Original && duplicate_depth != 1 {
        return None;
    }
    let max_stack_access = evm_version.reachable_stack_depth();
    let mut slots = Slots::from_elem(Slot::OTHER, usize::from(duplicate_depth));
    slots[0] = Slot::COPY;
    slots.push(Slot::GHOST);
    if matches!(ghost, Ghost::Original) {
        slots.swap(0, 1);
    }

    let mut candidate = Candidate::new(start, duplicate_depth, evm_version);

    let mut index = start + 1;
    while let Some(inst) = instructions.get(index) {
        let stack_op = inst.as_stack_op();
        match stack_op {
            Some(StackOp::Pop) if slots.last().is_some_and(|slot| slot.is_ghost) => {
                candidate.replace(index, StackOp::Pop, Replacement::new());
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
                let replacement = StackOp::Dup(u8::try_from(physical_depth).ok()?);
                candidate.replace(index, StackOp::Dup(depth as u8), smallvec![replacement]);
                slots.push(Slot { aliases_copy: selected.aliases_copy, is_ghost: false });
            }
            Some(StackOp::Swap(depth)) => {
                let mut replacement = Replacement::new();
                retarget_swap(&mut slots, depth, max_stack_access, &mut replacement)?;
                candidate.replace(index, StackOp::Swap(depth), replacement);
            }
            Some(StackOp::Exchange(n, m)) => {
                let mut replacement = Replacement::new();
                for depth in [n, m, n] {
                    retarget_swap(&mut slots, depth, max_stack_access, &mut replacement)?;
                }
                candidate.replace(index, StackOp::Exchange(n, m), replacement);
            }
            None => {
                if inst.as_evm_opcode().is_some_and(is_analysis_boundary) {
                    return None;
                }
                let effect = inst.stack_effect();
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

fn ensure_depth(slots: &mut Slots, depth: usize) {
    if depth > slots.len() {
        slots.insert_many(0, std::iter::repeat_n(Slot::OTHER, depth - slots.len()));
    }
}

fn physical_depth(slots: &[Slot], selected: usize) -> usize {
    slots[selected..].iter().filter(|slot| !slot.is_ghost).count()
}

fn nearest_alias_depth(slots: &[Slot], max_stack_access: usize) -> Option<usize> {
    let depth =
        slots.iter().rev().filter(|slot| !slot.is_ghost).position(|slot| slot.aliases_copy)? + 1;
    (depth <= max_stack_access).then_some(depth)
}

fn retarget_swap(
    slots: &mut Slots,
    depth: u8,
    max_stack_access: usize,
    replacement: &mut Replacement,
) -> Option<()> {
    let stack_depth = usize::from(depth) + 1;
    ensure_depth(slots, stack_depth);
    let top = slots.len() - 1;
    let selected = slots.len() - stack_depth;

    let live = physical_depth(slots, selected);
    if slots[selected].is_ghost || slots[top].is_ghost {
        if live > max_stack_access {
            return None;
        }
        for step in 1..live {
            push_swap(replacement, if slots[selected].is_ghost { live - step } else { step });
        }
    } else {
        if !(2..=max_stack_access + 1).contains(&live) {
            return None;
        }
        push_swap(replacement, live - 1);
    }
    slots.swap(selected, top);
    Some(())
}

/// Appends `swapN`, cancelling an identical swap before it.
fn push_swap(ops: &mut Replacement, depth: usize) {
    let swap = StackOp::Swap(u8::try_from(depth).unwrap());
    if ops.last() == Some(&swap) {
        ops.pop();
    } else {
        ops.push(swap);
    }
}

fn apply_edits(
    instructions: &mut Vec<Instruction>,
    edits: &[Edit],
    scratch: &mut Vec<Instruction>,
) {
    std::mem::swap(instructions, scratch);
    let mut edits = edits.iter().peekable();
    for (index, inst) in scratch.drain(..).enumerate() {
        if let Some(edit) = edits.next_if(|edit| edit.index == index) {
            instructions.extend(
                edit.replacement.iter().map(|&stack_op| {
                    Instruction::stack_op(stack_op).with_debug_loc(&inst.metadata)
                }),
            );
        } else {
            instructions.push(inst);
        }
    }
}

const fn is_analysis_boundary(opcode: u8) -> bool {
    op::is_terminal(opcode) || opcode == op::JUMPI
}
