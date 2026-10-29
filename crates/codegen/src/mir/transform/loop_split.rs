//! Splitting of counted loops at their lookahead guards.
//!
//! A loop `for (i = a; i < n; i += s)` whose body tests `i + k < n` for
//! literal offsets `k` (a group loop that reads `data[i + 1]` and
//! `data[i + 2]` behind their own guards, or chooses its padding on the
//! last group) pays every guard on every iteration, although each guard
//! holds until the last few. This pass duplicates such a loop into a main
//! loop that runs while `i + K < n`, `K` the largest lookahead offset, and
//! the original loop, which then runs at most the remaining `K / s + 1`
//! iterations. In the main loop the header condition implies every
//! lookahead guard and the original bound, and the following check
//! elimination folds them through its offset-bound rule.
//!
//! Recognition: a natural loop with a preheader and no inner loop, whose
//! header branches on `i < n` into the body and otherwise leaves the loop,
//! where `i` is a header phi with a literal start that every back edge
//! advances by the same literal positive step, `n` is defined outside the
//! loop, and some instruction of the loop reads `i + k < n`.
//!
//! Safety: `i` starts at a literal and grows by a literal step, so within
//! the target's trip-count bound `i + K` cannot wrap and `i + K < n` implies
//! `i < n`; the main loop therefore runs a prefix of the iterations the
//! original loop runs, with the same values and effects, and hands the
//! original loop the exact loop-carried state where it stops. Every loop
//! block is cloned with its instructions and effects unchanged. Edges that
//! leave the loop from its body reach the same blocks from both copies;
//! their phis gain the cloned edge. Loop-defined values may escape only on
//! those direct exit edges; downstream uses would lose their dominating
//! definition when the cloned loop bypasses the original loop.
//!
//! Profitability: gas mode only, since the loop's code is duplicated; the
//! loop is bounded in instructions and must hold a guard to fold. Runs in
//! the lowering pipeline once checked arithmetic has expanded, before the
//! check elimination that folds the guards.

use crate::{
    mir::{
        BlockId, Function, Immediate, InstKind, Instruction, MirType, Module, StorageAlias,
        Terminator, Value, ValueId,
        analysis::{Loop, LoopAnalyzer, LoopInfo},
        pass::{MirPass, run_function_pass},
        utils::rebuild_predecessors,
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Function pass that splits counted loops at their lookahead guards.
pub(crate) struct LoopSplit;

impl MirPass for LoopSplit {
    fn name(&self) -> &'static str {
        "loop-split"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| split_function(func))
    }
}

/// Instructions a loop may hold; the split duplicates all of them.
const MAX_LOOP_INSTRUCTIONS: usize = 96;

/// A loop to split and the facts the split needs.
struct Split {
    header: BlockId,
    preheader: BlockId,
    /// The header's successor inside the loop.
    body: BlockId,
    blocks: DenseBitSet<BlockId>,
    /// The header phi the loop counts on.
    counter: ValueId,
    /// The loop-invariant bound the header compares the counter against.
    bound: ValueId,
    /// The largest literal offset a guard adds to the counter.
    lookahead: U256,
}

fn split_function(func: &mut Function) -> bool {
    let mut changed = false;
    let mut split_headers = FxHashSet::default();
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(split) = loops
            .all_loops()
            .filter(|l| !split_headers.contains(&l.header))
            .find_map(|l| plan(func, &loops, l))
        else {
            break;
        };
        split_headers.insert(split.header);
        apply(func, &split);
        changed = true;
    }
    changed
}

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst) => Some(&func.inst(*inst).kind),
        _ => None,
    }
}

/// `value = base + literal`, as the base and the literal's value.
fn literal_offset(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
    let &InstKind::Add(a, b) = inst_kind(func, value)? else { return None };
    match (func.value_u256(a), func.value_u256(b)) {
        (_, Some(offset)) => Some((a, offset)),
        (Some(offset), _) => Some((b, offset)),
        _ => None,
    }
}

fn plan(func: &Function, loops: &LoopInfo, l: &Loop) -> Option<Split> {
    let preheader = l.preheader?;
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[l.header].terminator
    else {
        return None;
    };
    if !l.blocks.contains(then_block) || l.blocks.contains(else_block) {
        return None;
    }
    if loops.all_loops().any(|other| other.header != l.header && l.blocks.contains(other.header)) {
        return None;
    }
    let loop_insts = l.instructions(func);
    if loop_insts.count() > MAX_LOOP_INSTRUCTIONS {
        return None;
    }
    let defined_in_loop = |value| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(*inst),
        _ => false,
    };

    // header: counter = phi [preheader: start], [latch: counter + step]
    //         jumpi (lt counter, bound), body, exit
    let &InstKind::Lt(counter, bound) = inst_kind(func, condition)? else { return None };
    if defined_in_loop(bound) {
        return None;
    }
    let Value::Inst(phi) = func.value(counter) else { return None };
    if !func.blocks[l.header].instructions.contains(phi) {
        return None;
    }
    let InstKind::Phi(incoming) = &func.inst(*phi).kind else { return None };
    let mut start = None;
    let mut step = None;
    for &(from, value) in incoming {
        if from == preheader {
            start = Some(func.value_u256(value)?);
        } else if l.blocks.contains(from) {
            let (base, amount) = literal_offset(func, value)?;
            if base != counter || amount.is_zero() || step.is_some_and(|step| step != amount) {
                return None;
            }
            step = Some(amount);
        } else {
            return None;
        }
    }
    let (start, step) = (start?, step?);

    let mut lookahead = U256::ZERO;
    for inst in loop_insts.iter() {
        if let &InstKind::Lt(index, limit) = &func.inst(inst).kind
            && limit == bound
            && let Some((base, offset)) = literal_offset(func, index)
            && base == counter
        {
            lookahead = lookahead.max(offset);
        }
    }
    if lookahead.is_zero() {
        return None;
    }
    // The counter travels at most `step << MAX_TRIP_COUNT_BITS` from its
    // start, so `counter + lookahead` never wraps.
    let travel = if let Some(trips) = l.trip_count {
        step.checked_mul(U256::from(trips))?
    } else {
        step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?
    };
    if start.checked_add(travel)?.checked_add(lookahead)?
        > crate::mir::analysis::integers::integer_max(func, counter)
    {
        return None;
    }

    let mut body_exit = false;
    for block in l.blocks.iter() {
        for successor in func.blocks[block].terminator.as_ref()?.successors() {
            if !l.blocks.contains(successor) && !(block == l.header && successor == else_block) {
                body_exit = true;
            }
        }
    }
    if body_exit {
        // Both copies reach the exit blocks: only phis may use loop values there.
        for (block, body) in func.blocks.iter_enumerated() {
            if l.blocks.contains(block) {
                continue;
            }
            let in_instructions = body.instructions.iter().any(|&inst| {
                let kind = &func.inst(inst).kind;
                match kind {
                    InstKind::Phi(incoming) => incoming
                        .iter()
                        .any(|&(from, value)| !l.blocks.contains(from) && defined_in_loop(value)),
                    _ => kind.operands().into_iter().any(defined_in_loop),
                }
            });
            let in_terminator = body
                .terminator
                .as_ref()
                .is_some_and(|term| term.operands().into_iter().any(defined_in_loop));
            if in_instructions || in_terminator {
                return None;
            }
        }
    }
    Some(Split {
        header: l.header,
        preheader,
        body: then_block,
        blocks: l.blocks.clone(),
        counter,
        bound,
        lookahead,
    })
}

fn apply(func: &mut Function, split: &Split) {
    let blocks: Vec<BlockId> = split.blocks.iter().collect();
    let side_exits = side_exits(func, &blocks, &split.blocks, split.header);
    let copy = clone_loop(func, &blocks);
    let main_header = copy.blocks[&split.header];
    let mapped = |value: ValueId| copy.value(value);

    // main_header: ahead = add counter', lookahead
    //              jumpi (lt ahead, bound), body', header
    let ty = func.value_ty(split.counter).unwrap();
    let lookahead =
        func.alloc_value(Value::Immediate(Immediate::for_type(Some(ty), split.lookahead)));
    let (add, ahead) = func.alloc_value_inst(
        Instruction::new(InstKind::Add(mapped(split.counter), lookahead), Some(ty))
            .with_debug_info_dropped(),
    );
    let (lt, condition) = func.alloc_value_inst(
        Instruction::new(InstKind::Lt(ahead, split.bound), Some(MirType::I1))
            .with_debug_info_dropped(),
    );
    func.blocks[main_header].instructions.extend([add, lt]);
    let (_, metadata) = func.blocks[main_header].take_terminator();
    func.blocks[main_header].set_terminator(
        Terminator::Branch {
            condition,
            then_block: copy.blocks[&split.body],
            else_block: split.header,
        },
        metadata,
    );

    // header: v = phi [main_header: v'], [latch: next]
    for inst in func.blocks[split.header].instructions.clone() {
        let Some(result) = func.inst_result_value(inst) else { continue };
        if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
            for (from, value) in incoming.iter_mut() {
                if *from == split.preheader {
                    *from = main_header;
                    *value = copy.values[&result];
                }
            }
        }
    }

    // preheader: ... jump main_header
    let (terminator, metadata) = func.blocks[split.preheader].take_terminator();
    let mut terminator = terminator.expect("the preheader is terminated");
    retarget(
        &mut terminator,
        |successor| {
            if successor == split.header { main_header } else { successor }
        },
    );
    func.blocks[split.preheader].set_terminator(terminator, metadata);

    add_exit_inputs(func, &side_exits, std::slice::from_ref(&copy));

    rebuild_predecessors(func);
}

/// A copy of the loop: each original block's clone and each original value's clone.
pub(super) struct LoopCopy {
    pub(super) blocks: FxHashMap<BlockId, BlockId>,
    pub(super) values: FxHashMap<ValueId, ValueId>,
}

impl LoopCopy {
    pub(super) fn value(&self, value: ValueId) -> ValueId {
        self.values.get(&value).copied().unwrap_or(value)
    }
}

/// Clones every loop block with its instructions, keeping edges that leave the loop. A clone keeps
/// its original's metadata, which states facts about the instruction itself, such as an
/// allocation whose address inline assembly observes, with the values a storage alias names
/// renamed to their clones.
pub(super) fn clone_loop(func: &mut Function, blocks: &[BlockId]) -> LoopCopy {
    let mut copy = LoopCopy { blocks: FxHashMap::default(), values: FxHashMap::default() };
    for &block in blocks {
        copy.blocks.insert(block, func.alloc_block());
    }
    for &block in blocks {
        let originals = func.blocks[block].instructions.clone();
        let mut instructions = Vec::with_capacity(originals.len());
        for inst in originals {
            let original = func.inst(inst);
            let mut instruction = Instruction::new(original.kind.clone(), original.result_ty);
            instruction.metadata = original.metadata.clone();
            let cloned = if let Some(result) = func.inst_result_value(inst) {
                let (cloned, cloned_result) = func.alloc_value_inst(instruction);
                copy.values.insert(result, cloned_result);
                cloned
            } else {
                func.alloc_inst(instruction)
            };
            instructions.push(cloned);
        }
        func.blocks[copy.blocks[&block]].instructions = instructions;
    }
    for &block in blocks {
        let clone = copy.blocks[&block];
        for inst in func.blocks[clone].instructions.clone() {
            let instruction = func.inst_mut(inst);
            if let InstKind::Phi(incoming) = &mut instruction.kind {
                for (from, _) in incoming.iter_mut() {
                    if let Some(&clone_from) = copy.blocks.get(from) {
                        *from = clone_from;
                    }
                }
            }
            instruction.kind.visit_operands_mut(|value| *value = copy.value(*value));
            if let Some(alias) = instruction.metadata.storage_alias() {
                let alias = match alias {
                    StorageAlias::Slot(slot) => StorageAlias::Slot(slot),
                    StorageAlias::Symbolic(value) => StorageAlias::Symbolic(copy.value(value)),
                    StorageAlias::Offset { base, offset } => {
                        StorageAlias::Offset { base: copy.value(base), offset }
                    }
                };
                instruction.metadata.set_storage_alias(Some(alias));
            }
        }
        let (mut terminator, metadata) = {
            let body = &func.blocks[block];
            (
                body.terminator.clone().expect("loop blocks are terminated"),
                body.terminator_metadata.clone(),
            )
        };
        terminator.visit_operands_mut(|value| *value = copy.value(*value));
        retarget(&mut terminator, |successor| {
            copy.blocks.get(&successor).copied().unwrap_or(successor)
        });
        func.blocks[clone].set_terminator(terminator, metadata);
    }
    copy
}

/// The edges that leave the loop from a block other than its header.
pub(super) fn side_exits(
    func: &Function,
    blocks: &[BlockId],
    loop_blocks: &DenseBitSet<BlockId>,
    header: BlockId,
) -> Vec<(BlockId, BlockId)> {
    blocks
        .iter()
        .filter(|&&block| block != header)
        .flat_map(|&block| {
            let terminator = func.blocks[block].terminator.as_ref().expect("terminated");
            terminator
                .successors()
                .into_iter()
                .filter(|&successor| !loop_blocks.contains(successor))
                .map(move |successor| (block, successor))
        })
        .collect()
}

/// Gives the phis at the far end of each side exit an input from every copy of the exiting
/// block.
pub(super) fn add_exit_inputs(
    func: &mut Function,
    side_exits: &[(BlockId, BlockId)],
    copies: &[LoopCopy],
) {
    // exit: v = phi [block: x], [block_1: x_1], ..., [block_n: x_n]
    for &(block, successor) in side_exits {
        for inst in func.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = incoming
                    .iter()
                    .filter(|&&(from, _)| from == block)
                    .flat_map(|&(_, value)| {
                        copies.iter().map(move |copy| (copy.blocks[&block], copy.value(value)))
                    })
                    .collect();
                incoming.extend(cloned);
            }
        }
    }
}

/// Replaces every successor of a terminator through `map`.
pub(super) fn retarget(terminator: &mut Terminator, map: impl Fn(BlockId) -> BlockId) {
    match terminator {
        Terminator::Jump(target) => *target = map(*target),
        Terminator::Branch { then_block, else_block, .. } => {
            *then_block = map(*then_block);
            *else_block = map(*else_block);
        }
        Terminator::Switch { default, cases, .. } => {
            *default = map(*default);
            for (_, block) in cases {
                *block = map(*block);
            }
        }
        _ => {}
    }
}
