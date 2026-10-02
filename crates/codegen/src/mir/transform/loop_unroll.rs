//! Unrolling of counted loops by two.
//!
//! A loop `for (i = a; i < n; i += s)` pays its header test, a compare and a
//! conditional jump, once per iteration. This pass gives such a loop a main
//! loop that runs the body twice per test while `i + s < n`, that is while two
//! more iterations remain, and keeps the original loop after it for the last
//! iteration:
//!
//! ```text
//! main_header:
//!   state = phi [preheader: start], [latch_b: next_b]
//!   ahead = add i, s
//!   jumpi (lt ahead, n), body_a, header
//! body_a: first copy; jump header_b
//! header_b: the header's other instructions for `i + s`; jump body_b
//! body_b: second copy; jump main_header
//! header: state = phi [main_header: state], [latch: next]; the original loop
//! ```
//!
//! A loop that runs until its counter reaches the bound, `for (; i != n; i += s)`
//! as assembly loops over pointers are written, knows its remaining count
//! exactly: `n - i` is `s` times the count, so with `s = 2^t * u` for an odd
//! `u`, bit `t` of `n - i` is the count's parity. One peeled iteration evens
//! the count out, and the original header's test then admits two iterations:
//!
//! ```text
//! check:
//!   state = phi [preheader: start]
//!   jumpi (trunc (shr t, (sub n, i)) to i1), body_a, header
//! body_a: peeled copy; jump header
//! header: state = phi [check: state], [latch_a: next_a], [latch_b: next_b]
//!   jumpi (ne i, n), body, exit
//! body: the original body; jump header_b
//! header_b: the header's other instructions for `i + s`; jump body_b
//! body_b: second copy; jump header
//! ```
//!
//! Recognition: as for `loop-split`, a natural loop with a preheader and no
//! inner loop, whose header branches on `i < n` or `i != n` into the body and
//! otherwise leaves the loop, where `i` is a header phi, or a pointer phi the
//! header converts to an integer, that the loop's single back edge advances by
//! a literal positive step, and `n` is defined outside the loop. A `<` test
//! also needs a literal start. The header's other instructions must be free of
//! effects, because the original header repeats them for the iteration the
//! main loop declined. The body must be straight-line: every branch inside it
//! continues the loop on one arm and aborts on the other, such as an arithmetic
//! panic, where a block aborts when it reverts or calls a function that never
//! returns. The body may read words computed before the loop only when the
//! backend rebuilds them where they are read, as calldata words at fixed offsets
//! and environment reads: the stack scheduler spills other such words, and
//! their copies measured slower than the original loop.
//!
//! Safety: for `<`, `i` starts at a literal and grows by a literal step, so
//! within the target's trip-count bound `i + s` cannot wrap, and `i + s < n`
//! implies `i < n`. The main loop therefore runs the iterations the original
//! loop runs next, in the same order with the same values and effects, two at
//! a time. The second copy's header test holds by the main header's test, so it
//! enters its body directly. When the main test fails, the original loop receives
//! the exact loop-carried state and finishes. Every loop block is cloned with
//! its instructions and effects unchanged. Edges that leave the loop from its
//! body, such as panics, reach the same blocks from every copy; their phis gain
//! the cloned edges, and a loop qualifies only when no block reachable from
//! such an edge uses a loop-defined value except as a phi input on that edge,
//! so every definition still dominates its uses. The header's exit stays the
//! only way to leave the original loop normally, so values read after the loop
//! keep their definitions.
//!
//! For `!=`, counts are taken modulo `2^256`, as the counter wraps. When `n - i`
//! is `s` times some count `k`, the loop runs exactly `k` more iterations, and
//! bit `t` of `n - i` is `k`'s parity. A set bit means `i != n`, so the peeled
//! iteration is one the original loop runs; an even count remains after it, so
//! whenever the header admits an iteration a second one follows, and the second
//! copy's test holds. When no such `k` exists, `i` never reaches `n`: the
//! original loop never ends, and neither does the unrolled one, every test of
//! which the original loop also passes.
//!
//! Each copy advances the counter as the original latch does, and the main
//! header's addition feeds only its test. Reusing `ahead` as the first copy's
//! counter saves that addition, but leaves `i` live only on the exit edge and
//! `ahead` only on the body edge, which the stack scheduler pays for with swaps
//! and pops in the copies; recomputing keeps every copy's stack shape the
//! original body's.
//!
//! Profitability: gas mode only, priced by the target over the deployment's
//! expected executions. Each pair of iterations skips a header test and a
//! back-edge jump. For `<`, the first pair skips nothing and every test of the
//! main loop, including the declined one, adds the step first; for `!=`, the
//! parity check costs about one test per entry. The new code holds two more
//! copies of the body and one of the header. Literal bounds give the trip
//! count, and other loops are assumed to run the target's estimate for
//! uncounted loops. Runs after the late loop passes have fixed the loop's
//! physical shape, and before the final CFG cleanup and dead-code elimination
//! remove the cloned tests the copies no longer read.

use super::loop_split::{rebuild_predecessors, retarget};
use crate::{
    backend::evm::op,
    mir::{
        BlockId, Callee, EffectKind, Function, FunctionId, Immediate, InstKind, Instruction,
        MirType, Module, OpTraits, Terminator, Value, ValueId,
        analysis::{Loop, LoopAnalyzer, LoopInfo, cold_functions},
        pass::{MirPass, run_function_pass},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Function pass that unrolls counted loops by two.
pub(crate) struct LoopUnroll;

impl MirPass for LoopUnroll {
    fn name(&self) -> &'static str {
        "loop-unroll"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        let cold = cold_functions(module);
        run_function_pass(module, analyses, |func, _| unroll_function(func, &cold, target))
    }
}

/// How the header decides that another iteration runs.
#[derive(Clone, Copy)]
enum Test {
    /// `i < n`, with a literal start so `i + s` cannot wrap.
    Below,
    /// `i != n`.
    Reaches,
}

/// A loop to unroll and the facts the unrolling needs.
struct Unroll {
    header: BlockId,
    preheader: BlockId,
    latch: BlockId,
    /// The header's successor inside the loop.
    body: BlockId,
    blocks: DenseBitSet<BlockId>,
    test: Test,
    /// The counter as the header tests it: the counting phi, or its integer value.
    word: ValueId,
    /// The loop-invariant bound the header compares the counter against.
    bound: ValueId,
    step: U256,
}

fn unroll_function(func: &mut Function, cold: &DenseBitSet<FunctionId>, target: Target) -> bool {
    let mut changed = false;
    let mut done = FxHashSet::default();
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(unroll) = loops
            .all_loops()
            .filter(|l| !done.contains(&l.header))
            .find_map(|l| plan(func, &loops, l, cold, target))
        else {
            break;
        };
        done.insert(unroll.header);
        done.insert(apply(func, &unroll));
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

fn jumps_to(func: &Function, block: BlockId, target: BlockId) -> bool {
    matches!(func.blocks[block].terminator, Some(Terminator::Jump(to)) if to == target)
}

/// Whether the backend rebuilds a word where it is read instead of keeping it live: a calldata
/// word at a fixed offset or a stable environment read.
fn rebuilt_at_use(func: &Function, value: ValueId) -> bool {
    let Some(kind) = inst_kind(func, value) else { return true };
    match kind {
        InstKind::Zext(inner) => rebuilt_at_use(func, *inner),
        InstKind::CalldataLoad(offset) => func.value_u256(*offset).is_some(),
        _ => {
            let def = kind.op_def();
            def.traits.contains(OpTraits::REMATERIALIZABLE) && def.effect != EffectKind::Pure
        }
    }
}

/// Whether a block aborts instead of continuing: it reverts, or calls a cold function.
fn is_cold(func: &Function, block: BlockId, cold: &DenseBitSet<FunctionId>) -> bool {
    let body = &func.blocks[block];
    body.instructions.iter().any(|&inst| {
        matches!(
            func.inst(inst).kind,
            InstKind::ICall { function: Callee::Function(function), .. } if cold.contains(function)
        )
    }) || matches!(
        body.terminator,
        Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
    ) || matches!(body.terminator, Some(Terminator::TailCall { function, .. }) if cold.contains(function))
}

fn plan(
    func: &Function,
    loops: &LoopInfo,
    l: &Loop,
    cold: &DenseBitSet<FunctionId>,
    target: Target,
) -> Option<Unroll> {
    let preheader = l.preheader?;
    let &[latch] = l.back_edges.as_slice() else { return None };
    if latch == l.header || !jumps_to(func, preheader, l.header) || !jumps_to(func, latch, l.header)
    {
        return None;
    }
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
    let mut loop_insts = DenseBitSet::new_empty(func.num_insts());
    for block in l.blocks.iter() {
        for &inst in &func.blocks[block].instructions {
            loop_insts.insert(inst);
        }
    }
    // Only a straight-line body gains: every branch inside it continues the loop on one arm
    // and aborts on the other, such as an arithmetic panic.
    for block in l.blocks.iter() {
        if block == l.header || is_cold(func, block, cold) {
            continue;
        }
        let successors = func.blocks[block].terminator.as_ref()?.successors();
        let continuing: Vec<_> =
            successors.iter().filter(|&&successor| !is_cold(func, successor, cold)).collect();
        if continuing.len() != 1 || !l.blocks.contains(*continuing[0]) {
            return None;
        }
    }
    // The stack planner spills a word the body reads from before the loop, and the copies
    // measured slower than the original loop.
    for block in l.blocks.iter() {
        if block == l.header {
            continue;
        }
        let reads_hoisted = |value: ValueId| {
            matches!(func.value(value), Value::Inst(inst) if !loop_insts.contains(*inst))
                && !rebuilt_at_use(func, value)
        };
        if func.blocks[block].instructions.iter().any(|&inst| {
            let kind = &func.inst(inst).kind;
            !matches!(kind, InstKind::Phi(_)) && kind.operands().into_iter().any(reads_hoisted)
        }) || func.blocks[block]
            .terminator
            .as_ref()
            .is_some_and(|terminator| terminator.operands().into_iter().any(reads_hoisted))
        {
            return None;
        }
    }
    // The original header repeats its instructions for the iteration the main loop declined.
    if func.blocks[l.header].instructions.iter().any(|&inst| {
        let kind = &func.inst(inst).kind;
        !matches!(kind, InstKind::Phi(_)) && kind.has_side_effects()
    }) {
        return None;
    }
    let defined_in_loop = |value| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(*inst),
        _ => false,
    };

    // header: counter = phi [preheader: start], [latch: next]
    //         word = counter | ptrtoint counter
    //         jumpi (lt word, bound) | (ne word, bound), body, exit
    // latch:  next = add word, step | inttoptr (add word, step)
    let (test, word, bound) = match *inst_kind(func, condition)? {
        InstKind::Lt(word, bound) => (Test::Below, word, bound),
        InstKind::Ne(a, b) if defined_in_loop(a) => (Test::Reaches, a, b),
        InstKind::Ne(a, b) => (Test::Reaches, b, a),
        _ => return None,
    };
    if defined_in_loop(bound) {
        return None;
    }
    let counter = match inst_kind(func, word) {
        Some(&InstKind::PtrToInt(pointer, _)) => pointer,
        _ => word,
    };
    let Value::Inst(phi) = func.value(counter) else { return None };
    if !func.blocks[l.header].instructions.contains(phi) {
        return None;
    }
    let InstKind::Phi(incoming) = &func.inst(*phi).kind else { return None };
    let mut start = None;
    let mut next = None;
    for &(from, value) in incoming {
        if from == preheader {
            start = Some(value);
        } else if from == latch {
            next = Some(value);
        } else {
            return None;
        }
    }
    let (start, next) = (start?, next?);
    let advance = match inst_kind(func, next)? {
        &InstKind::IntToPtr(advance) if counter != word => advance,
        _ if counter == word => next,
        _ => return None,
    };
    let &InstKind::Add(a, b) = inst_kind(func, advance)? else { return None };
    let step = match (func.value_u256(a), func.value_u256(b)) {
        (_, Some(step)) if a == word => step,
        (Some(step), _) if b == word => step,
        _ => return None,
    };
    if step.is_zero() {
        return None;
    }
    let trips = match test {
        Test::Below => {
            // The counter travels at most `step << MAX_TRIP_COUNT_BITS` from its start, so
            // `word + step` never wraps.
            let start = func.value_u256(start)?;
            let travel = step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?;
            start.checked_add(travel)?.checked_add(step)?;
            func.value_u256(bound).map_or(Target::UNCOUNTED_LOOP_ITERATIONS, |limit| {
                u64::try_from(limit.saturating_sub(start).div_ceil(step)).unwrap_or(u64::MAX)
            })
        }
        Test::Reaches => match (func.value_u256(start), func.value_u256(bound)) {
            (Some(start), Some(limit)) if (limit.wrapping_sub(start) % step).is_zero() => {
                u64::try_from(limit.wrapping_sub(start) / step).unwrap_or(u64::MAX)
            }
            _ => Target::UNCOUNTED_LOOP_ITERATIONS,
        },
    };
    if !repays(func, l, test, trips, step, target) {
        return None;
    }

    // Blocks the copies can reach by leaving the loop from its body must not read a loop
    // value except through the phi input of that edge.
    let mut reachable = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = Vec::new();
    for block in l.blocks.iter() {
        for successor in func.blocks[block].terminator.as_ref()?.successors() {
            if !l.blocks.contains(successor)
                && !(block == l.header && successor == else_block)
                && reachable.insert(successor)
            {
                worklist.push(successor);
            }
        }
    }
    while let Some(block) = worklist.pop() {
        let body = &func.blocks[block];
        for &inst in &body.instructions {
            let kind = &func.inst(inst).kind;
            match kind {
                InstKind::Phi(incoming) => {
                    if incoming
                        .iter()
                        .any(|&(from, value)| !l.blocks.contains(from) && defined_in_loop(value))
                    {
                        return None;
                    }
                }
                _ => {
                    if kind.operands().into_iter().any(defined_in_loop) {
                        return None;
                    }
                }
            }
        }
        let terminator = body.terminator.as_ref()?;
        if terminator.operands().into_iter().any(defined_in_loop) {
            return None;
        }
        for successor in terminator.successors() {
            if !l.blocks.contains(successor) && reachable.insert(successor) {
                worklist.push(successor);
            }
        }
    }
    Some(Unroll {
        header: l.header,
        preheader,
        latch,
        body: then_block,
        blocks: l.blocks.clone(),
        test,
        word,
        bound,
        step,
    })
}

/// Whether the tests and jumps the main loop skips over the deployment's expected executions
/// outweigh the deposit of its code.
fn repays(func: &Function, l: &Loop, test: Test, trips: u64, step: U256, target: Target) -> bool {
    let compare = match test {
        Test::Below => op::LT,
        Test::Reaches => op::SUB,
    };
    let tested = target.opcode(compare)
        + target.dup().times(2)
        + target.opcode(op::PUSH2)
        + target.opcode(op::JUMPI);
    let back_edge =
        target.opcode(op::PUSH2) + target.opcode(op::JUMP) + target.opcode(op::JUMPDEST);
    let pairs = u32::try_from(trips / 2).unwrap_or(u32::MAX);
    let saving = match test {
        // Every pair of iterations after the first skips a test and a back edge, and every test
        // of the main loop, including the declined one, first adds the step.
        Test::Below => {
            let advance = target.opcode(op::ADD) + target.push(step);
            let skipped = (tested + back_edge).times(pairs.saturating_sub(1));
            skipped.gas.saturating_sub(advance.times(pairs.saturating_add(1)).gas)
        }
        // Every pair skips a test and a back edge, and entering the loop tests the parity of
        // the remaining count once.
        Test::Reaches => {
            let mut parity = tested + target.opcode(op::AND) + target.push(U256::from(1));
            let zeros = step.trailing_zeros();
            if zeros != 0 {
                parity += target.opcode(op::SHR) + target.push(U256::from(zeros));
            }
            (tested + back_edge).times(pairs).gas.saturating_sub(parity.gas)
        }
    };
    // The main loop holds two copies of the body and one of the header.
    let growth: Cost = l
        .blocks
        .iter()
        .map(|block| {
            let copies = if block == l.header { 1 } else { 2 };
            target.block_code_estimate(func, block).times(copies)
        })
        .sum();
    target.lifetime_gas(Cost::new(saving, 0)) > target.lifetime_gas(Cost::new(0, growth.bytes))
}

/// A copy of the loop: each original block's clone and each original value's clone.
struct Copy {
    blocks: FxHashMap<BlockId, BlockId>,
    values: FxHashMap<ValueId, ValueId>,
}

impl Copy {
    fn value(&self, value: ValueId) -> ValueId {
        self.values.get(&value).copied().unwrap_or(value)
    }
}

/// Clones every loop block with its instructions, keeping edges that leave the loop.
fn clone_loop(func: &mut Function, blocks: &[BlockId]) -> Copy {
    let mut copy = Copy { blocks: FxHashMap::default(), values: FxHashMap::default() };
    for &block in blocks {
        copy.blocks.insert(block, func.alloc_block());
    }
    for &block in blocks {
        let originals = func.blocks[block].instructions.clone();
        let mut instructions = Vec::with_capacity(originals.len());
        for inst in originals {
            let original = func.inst(inst);
            let mut instruction = Instruction::new(original.kind.clone(), original.result_ty);
            instruction.metadata.copy_debug_context(&original.metadata);
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
            let kind = &mut func.inst_mut(inst).kind;
            if let InstKind::Phi(incoming) = kind {
                for (from, _) in incoming.iter_mut() {
                    if let Some(&clone_from) = copy.blocks.get(from) {
                        *from = clone_from;
                    }
                }
            }
            kind.visit_operands_mut(|value| *value = copy.value(*value));
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

/// Appends a compiler-generated instruction to a block and returns its result.
fn append(func: &mut Function, block: BlockId, kind: InstKind, ty: MirType) -> ValueId {
    let (inst, value) =
        func.alloc_value_inst(Instruction::new(kind, Some(ty)).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}

/// Ends a block with a branch, keeping its terminator's source context.
fn branch(func: &mut Function, block: BlockId, condition: ValueId, then: BlockId, other: BlockId) {
    let (_, metadata) = func.blocks[block].take_terminator();
    func.blocks[block].set_terminator(
        Terminator::Branch { condition, then_block: then, else_block: other },
        metadata,
    );
}

/// Sends every edge of a block's terminator to `target`.
fn jump_to(func: &mut Function, block: BlockId, target: BlockId) {
    let (terminator, metadata) = func.blocks[block].take_terminator();
    let mut terminator = terminator.expect("loop blocks are terminated");
    retarget(&mut terminator, |_| target);
    func.blocks[block].set_terminator(terminator, metadata);
}

/// Edits the incoming list of the phi defining `phi`.
fn edit_phi(func: &mut Function, phi: ValueId, edit: impl FnOnce(&mut Vec<(BlockId, ValueId)>)) {
    let Value::Inst(inst) = *func.value(phi) else { return };
    if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
        edit(incoming);
    }
}

/// Unrolls the loop and returns the main loop's header.
fn apply(func: &mut Function, unroll: &Unroll) -> BlockId {
    let blocks: Vec<BlockId> = unroll.blocks.iter().collect();
    // Edges that leave the loop from its body, before any terminator changes.
    let side_exits: Vec<_> = blocks
        .iter()
        .filter(|&&block| block != unroll.header)
        .flat_map(|&block| {
            let terminator = func.blocks[block].terminator.as_ref().expect("terminated");
            terminator
                .successors()
                .into_iter()
                .filter(|&successor| !unroll.blocks.contains(successor))
                .map(move |successor| (block, successor))
        })
        .collect();
    let first = clone_loop(func, &blocks);
    let second = clone_loop(func, &blocks);
    let first_latch = first.blocks[&unroll.latch];
    let second_header = second.blocks[&unroll.header];
    let second_latch = second.blocks[&unroll.latch];
    let header_phis: Vec<_> = func.blocks[unroll.header]
        .instructions
        .iter()
        .filter_map(|&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let latch_value = incoming
                .iter()
                .find_map(|&(from, value)| (from == unroll.latch).then_some(value))?;
            Some((func.inst_result_value(inst)?, latch_value))
        })
        .collect();

    let mut replacements = FxHashMap::default();
    let (entry, main_header) = match unroll.test {
        Test::Below => {
            // main_header: ahead = add word', step
            //              jumpi (lt ahead, bound), body', header
            let main_header = first.blocks[&unroll.header];
            let step = func.alloc_value(Value::Immediate(Immediate::I256(unroll.step)));
            let word = first.value(unroll.word);
            let ahead = append(func, main_header, InstKind::Add(word, step), MirType::I256);
            let condition =
                append(func, main_header, InstKind::Lt(ahead, unroll.bound), MirType::I1);
            branch(func, main_header, condition, first.blocks[&unroll.body], unroll.header);

            // latch': jump header''
            // latch'': jump main_header
            jump_to(func, first_latch, second_header);
            jump_to(func, second_latch, main_header);

            // main_header: state' = phi [preheader: start], [latch'': next'']
            // header: state = phi [main_header: state'], [latch: next]
            for &(phi, latch_value) in &header_phis {
                replacements.insert(second.value(phi), first.value(latch_value));
                edit_phi(func, first.value(phi), |incoming| {
                    for (from, value) in incoming.iter_mut() {
                        if *from == first_latch {
                            *from = second_latch;
                            *value = second.value(latch_value);
                        }
                    }
                });
                edit_phi(func, phi, |incoming| {
                    for (from, value) in incoming.iter_mut() {
                        if *from == unroll.preheader {
                            *from = main_header;
                            *value = first.value(phi);
                        }
                    }
                });
            }
            (main_header, main_header)
        }
        Test::Reaches => {
            // check: state' = phi [preheader: start]
            //        left = sub bound, word'
            //        jumpi (trunc (shr zeros(step), left) to i1), body', header
            let check = first.blocks[&unroll.header];
            let word = first.value(unroll.word);
            let mut left = append(func, check, InstKind::Sub(unroll.bound, word), MirType::I256);
            let zeros = unroll.step.trailing_zeros();
            if zeros != 0 {
                let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(zeros))));
                left = append(func, check, InstKind::Shr(shift, left), MirType::I256);
            }
            let odd = append(func, check, InstKind::Trunc(left, 1), MirType::I1);
            branch(func, check, odd, first.blocks[&unroll.body], unroll.header);

            // latch': jump header
            // latch: jump header''
            // latch'': jump header
            jump_to(func, first_latch, unroll.header);
            jump_to(func, unroll.latch, second_header);
            jump_to(func, second_latch, unroll.header);

            // check: state' = phi [preheader: start]
            // header: state = phi [check: state'], [latch': next'], [latch'': next'']
            for &(phi, latch_value) in &header_phis {
                replacements.insert(second.value(phi), latch_value);
                edit_phi(func, first.value(phi), |incoming| {
                    incoming.retain(|&(from, _)| from != first_latch);
                });
                edit_phi(func, phi, |incoming| {
                    for (from, value) in incoming.iter_mut() {
                        if *from == unroll.preheader {
                            *from = check;
                            *value = first.value(phi);
                        } else if *from == unroll.latch {
                            *from = second_latch;
                            *value = second.value(latch_value);
                        }
                    }
                    incoming.push((first_latch, first.value(latch_value)));
                });
            }
            (check, unroll.header)
        }
    };

    // header'': ...; jump body''
    let second_phis: FxHashSet<_> = header_phis.iter().map(|&(phi, _)| second.value(phi)).collect();
    let kept: Vec<_> = func.blocks[second_header]
        .instructions
        .iter()
        .copied()
        .filter(|&inst| {
            func.inst_result_value(inst).is_none_or(|result| !second_phis.contains(&result))
        })
        .collect();
    func.blocks[second_header].instructions = kept;
    let (_, metadata) = func.blocks[second_header].take_terminator();
    func.blocks[second_header]
        .set_terminator(Terminator::Jump(second.blocks[&unroll.body]), metadata);

    // preheader: ... jump entry
    let (terminator, metadata) = func.blocks[unroll.preheader].take_terminator();
    let mut terminator = terminator.expect("the preheader is terminated");
    retarget(
        &mut terminator,
        |successor| {
            if successor == unroll.header { entry } else { successor }
        },
    );
    func.blocks[unroll.preheader].set_terminator(terminator, metadata);

    // exit: v = phi [block: x], ..., [block': x'], [block'': x'']
    for (block, successor) in side_exits {
        for inst in func.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = incoming
                    .iter()
                    .filter(|&&(from, _)| from == block)
                    .flat_map(|&(_, value)| {
                        [
                            (first.blocks[&block], first.value(value)),
                            (second.blocks[&block], second.value(value)),
                        ]
                    })
                    .collect();
                incoming.extend(cloned);
            }
        }
    }

    func.replace_uses(&replacements);
    rebuild_predecessors(func);
    main_header
}
