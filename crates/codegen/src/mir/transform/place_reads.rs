//! Place each calldata read where the code that uses it runs: shared by a branch's paths, or
//! beside the uses a guard no longer shares.
//!
//! A word at a constant offset, such as an ABI head word, is read by the guard that checks it, by
//! the body that uses it, and in a dispatcher by every route. Three moves put those reads where
//! the backend can price them, and a fourth moves what is computed from them. Calldata reads cannot
//! trap or change during a call, so both moves keep every path's result.
//!
//! **Hoisting.** A word that every continuing path out of a branching block reads is read once in
//! that block, and each read the block dominates uses that value. A block that only reverts does
//! not count as a path, since a read there costs only the revert. A read common to the routes of a
//! dispatcher is then one read before the `switch` instead of one per route, which is the smaller
//! code, and the backend keeps it for the routes that use it. The target prices the hoist
//! ([`Target::hoisted_read_pays`]): one read, a `dup` per further reader, and the `dup` and `swap`
//! a word kept on the stack costs the words around it, against a read at each; too few readers do
//! not repay the longer live range, so their reads stay. A word already read in the block or above
//! it is left alone, and a block with a single continuing successor hoists nothing, which keeps a
//! guard's read apart from the body's.
//!
//! **Placement.** The guard's read and the body's are one value after CSE, and ABI lowering emits
//! them as one. That value is defined at the guard and lives to every later use, across calls and
//! branches. The backend prices keeping a word as one push and a cheap `dup` per use, and leaves
//! out what a long-lived word costs the words around it: the `dup` and `swap` that reach past it,
//! and the stack height every block between the guard and the last use carries. The price is right
//! only where the definition sits at the uses it serves. For each read in a block, the pass moves
//! every use outside that block to a copy of the read at the start of the nearest block outside
//! every loop that dominates those uses. A phi input counts as a use at the end of its
//! predecessor. The entry block keeps its own uses and the original, since the guard needs them,
//! so the body path reads the word a second time and the shorter live range repays it. Another
//! block's read moves entirely, and only when the block has no use of its own, so no path gains a
//! read. A word whose first use sits after a branch is then not read on the path that leaves at
//! the branch. The copy never lands inside a loop that does not contain the original.
//!
//! **Computations.** Code that runs before a guard in source order but is read only after it, such
//! as `Role next = allowed ? A : B;` ahead of a check that an enum read from storage is in range,
//! keeps its result live across the guard's branch, which costs the stack scheduler a `DUP` or
//! `SWAP`, and the revert path computes it for nothing. A guard is a conditional branch with one
//! successor that only reverts. A set of instructions in the guard's block moves, in order, to the
//! start of the block that continues after it. Members are pure instructions and reads of calldata
//! at constant offsets, which cannot change during the call; zero tests stay, since the test move
//! below places them. Every reader of a member is another member, or sits in a block the
//! continuing block dominates, so the reverting successor reads none of the results. The
//! continuing block must have the guard's block as its only predecessor, so a loop header is never
//! a target and the set runs once per entry, as before. Instructions with control effects, such
//! as checked arithmetic, never move: past the guard, a failing guard would hide their panic.
//!
//! The set must also be closed: its operands are constants, results of its own members, or
//! calldata words that nothing else in the block reads. Such a set carries no other value across
//! the guard in place of its results and adds no read; the placement above reads the calldata word
//! again where it is used, which replaces the read that was carried. A set that reads any other
//! value, for example `add ptr, 32` on a memory pointer or `add arg, 64` on an argument, does not
//! move. Moving it makes the operand, which lives on past the guard anyway, be reached again after
//! the guard with a `DUP`, which costs what carrying the result cost, and it changes the order the
//! scheduler sees. A first version moved every pure instruction read only after a guard, and it
//! grew the corpus: solady-lib-string by 111 bytes at `-Ogas`, and tempo-zones-portal's runtime
//! gas by 20. Closed sets add no carried value and no read, so they need no price from the
//! target. This move runs first, so the placement below sees the readers it left.
//!
//! **Tests.** ABI lowering normalizes a validated bool at the guard, as `word != 0`, so the test
//! sits above the branches that read it. A zero test of a read whose only use is in another block
//! moves to that block first. The branch then tests the word itself, as the backend peels, rather
//! than a bool kept across the paths that split before it. The test never moves into a loop.
//!
//! The pass runs once, after the last CSE and before the backend, so no later pass merges the
//! copies again.

use super::word_sequence::is_pure;
use crate::{
    mir::{
        BlockId, Function, InstId, InstKind, Instruction, MirModuleTarget, Module, Terminator,
        Value, ValueId,
        analysis::CfgInfo,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
    },
    target::Target,
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

pub(crate) struct PlaceReads;

impl MirPass for PlaceReads {
    fn name(&self) -> &'static str {
        "place-reads"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let target = module.target(gcx);
        run_function_pass(module, analyses, |func, _| {
            let sunk = sink_guarded(func);
            let tested = sink_tests(func);
            let hoisted = hoist(func, &target);
            run(func) || hoisted || tested || sunk
        })
    }
}

/// Moves each zero test of a read to the block of its only use. A word that ABI lowering
/// normalizes at the guard would otherwise be tested before the paths split, and kept as a bool
/// across them, instead of tested by the branch that reads it.
fn sink_tests(func: &mut Function) -> bool {
    // The instruction in each block that reads a value, or none for a phi or terminator.
    let mut uses = FxHashMap::<ValueId, SmallVec<[(BlockId, Option<InstId>); 1]>>::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            match &func.inst(inst).kind {
                InstKind::Phi(incoming) => {
                    for &(pred, operand) in incoming {
                        uses.entry(operand).or_default().push((pred, None));
                    }
                }
                kind => kind.visit_operands(|operand| {
                    uses.entry(operand).or_default().push((block, Some(inst)));
                }),
            }
        }
        if let Some(term) = &data.terminator {
            term.visit_operands(|operand| uses.entry(operand).or_default().push((block, None)));
        }
    }
    let cfg = CfgInfo::new(func);
    let mut moves = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            let (InstKind::Eq(a, b) | InstKind::Ne(a, b)) = func.inst(inst).kind else { continue };
            let input = match (func.value_u256(a), func.value_u256(b)) {
                (_, Some(zero)) if zero.is_zero() => a,
                (Some(zero), _) if zero.is_zero() => b,
                _ => continue,
            };
            let Some(result) = func.inst_result_value(inst) else { continue };
            if !func.is_constant_calldata_load(input) && !matches!(func.value(input), Value::Arg(_))
            {
                continue;
            }
            let Some(&[(use_block, user)]) = uses.get(&result).map(SmallVec::as_slice) else {
                continue;
            };
            if use_block != block
                && cfg.dominators().dominates(block, use_block)
                && (cfg.cyclic_blocks().contains(block) || !cfg.cyclic_blocks().contains(use_block))
            {
                moves.push((block, inst, use_block, user));
            }
        }
    }
    for &(block, inst, use_block, user) in &moves {
        // test = ne read, 0; ... use => ...; test = ne read, 0; use
        func.blocks[block].instructions.retain(|&other| other != inst);
        let position = user
            .and_then(|user| func.blocks[use_block].instructions.iter().position(|&i| i == user))
            .unwrap_or(func.blocks[use_block].instructions.len());
        func.blocks[use_block].instructions.insert(position, inst);
    }
    !moves.is_empty()
}

/// Reads each word that every path out of a branching block must read, once, in that block.
fn hoist(func: &mut Function, target: &Target) -> bool {
    // The constant offsets read anywhere, and the offsets each block reads.
    let mut offsets = Vec::new();
    let mut loads = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            if let Some(result) = func.inst_result_value(inst)
                && func.is_constant_calldata_load(result)
                && let InstKind::Load(address, _) = func.inst(inst).kind
                && let Some(offset) = func.value_u64(address)
            {
                offsets.push(offset);
                loads.push((block, offset, result));
            }
        }
    }
    offsets.sort_unstable();
    offsets.dedup();
    if offsets.is_empty() {
        return false;
    }
    let index = |offset: u64| offsets.binary_search(&offset).expect("offset was collected");
    let cfg = CfgInfo::new(func);
    // A block that only reverts reads nothing, and a read on its path costs only the revert.
    let continuing = func
        .blocks
        .iter()
        .map(|data| {
            let mut next = data.terminator.as_ref().map_or(Vec::new(), |t| t.successors().to_vec());
            next.sort_unstable();
            next.dedup();
            next.retain(|&next| {
                !func.blocks[next].terminator.as_ref().is_some_and(|t| t.reverts())
            });
            next
        })
        .collect::<IndexVec<BlockId, _>>();
    let mut local = index_vec![DenseBitSet::<usize>::new_empty(offsets.len()); func.blocks.len()];
    for &(block, offset, _) in &loads {
        local[block].insert(index(offset));
    }
    // The offsets read on every path from the start of each block, a greatest fixed point.
    let mut anticipated =
        index_vec![DenseBitSet::<usize>::new_filled(offsets.len()); func.blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for &block in cfg.rpo().iter().rev() {
            let mut next = continuing[block].iter().copied();
            let mut reads = DenseBitSet::new_empty(offsets.len());
            if let Some(first) = next.next() {
                reads.union(&anticipated[first]);
                for other in next {
                    reads.intersect(&anticipated[other]);
                }
            }
            reads.union(&local[block]);
            if reads != anticipated[block] {
                anticipated[block] = reads;
                changed = true;
            }
        }
    }
    let mut hoisted = false;
    for &block in cfg.rpo() {
        let next = &continuing[block];
        if next.len() < 2 {
            continue;
        }
        let mut reads = anticipated[next[0]].clone();
        for &other in &next[1..] {
            reads.intersect(&anticipated[other]);
        }
        for offset in reads.iter() {
            let offset = offsets[offset];
            // An offset read here or above already has a value.
            if loads
                .iter()
                .any(|&(home, read, _)| read == offset && cfg.dominators().dominates(home, block))
            {
                continue;
            }
            // The reads below the branch that the hoisted read replaces.
            let below = loads
                .iter()
                .filter(|&&(home, read, _)| {
                    read == offset && cfg.dominators().dominates(block, home)
                })
                .filter_map(|&(_, _, result)| match *func.value(result) {
                    Value::Inst(inst) => Some(inst),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !target.hoisted_read_pays(offset, below.len() as u32) {
                continue;
            }
            // v = calldataload offset, before the branch
            let instruction = func.inst(below[0]);
            let mut copy = Instruction::new(instruction.kind.clone(), instruction.result_ty);
            copy.metadata.preserve_debug_loc(&instruction.metadata);
            copy.metadata
                .merge_debug_locs(below[1..].iter().map(|&other| &func.inst(other).metadata));
            let (inst, value) = func.alloc_value_inst(copy);
            func.blocks[block].instructions.push(inst);
            // read dominated by the branch => the hoisted read
            let mut replacements = FxHashMap::default();
            for (home, read, result) in &mut loads {
                if *read == offset && cfg.dominators().dominates(block, *home) {
                    replacements.insert(*result, value);
                    *home = block;
                    *result = value;
                }
            }
            func.replace_uses(&replacements);
            hoisted = true;
        }
    }
    hoisted
}

fn run(func: &mut Function) -> bool {
    let mut loads = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            if let Some(result) = func.inst_result_value(inst)
                && func.is_constant_calldata_load(result)
            {
                loads.push((block, inst, result));
            }
        }
    }
    if loads.is_empty() {
        return false;
    }
    // The blocks that read each load; a phi reads its input on the edge from its predecessor.
    let mut uses = FxHashMap::<ValueId, SmallVec<[BlockId; 2]>>::default();
    for &(_, _, result) in &loads {
        uses.insert(result, SmallVec::new());
    }
    for (block, data) in func.blocks.iter_enumerated() {
        let mut note = |value: ValueId, block: BlockId| {
            if let Some(blocks) = uses.get_mut(&value) {
                blocks.push(block);
            }
        };
        for &inst in &data.instructions {
            match &func.inst(inst).kind {
                InstKind::Phi(incoming) => {
                    for &(pred, operand) in incoming {
                        note(operand, pred);
                    }
                }
                kind => kind.visit_operands(|operand| note(operand, block)),
            }
        }
        if let Some(term) = &data.terminator {
            term.visit_operands(|operand| note(operand, block));
        }
    }
    let cfg = CfgInfo::new(func);
    let mut sunk = Vec::new();
    for &(home, inst, result) in &loads {
        // Another block keeps its own read only while the entry block's guard still needs it.
        if home != BlockId::ENTRY && uses[&result].contains(&home) {
            continue;
        }
        let mut outside = uses[&result].iter().copied().filter(|&user| user != home);
        let Some(first) = outside.next() else { continue };
        let mut target = outside.fold(first, |a, b| cfg.dominators().common_dominator(a, b));
        while cfg.cyclic_blocks().contains(target)
            && let Some(idom) = cfg.dominators().idom(target)
            && idom != target
        {
            target = idom;
        }
        if target != home && cfg.dominators().dominates(home, target) {
            sunk.push((home, inst, result, target));
        }
    }
    if sunk.is_empty() {
        return false;
    }
    // original read => its block and its copy at the start of the block that dominates the rest
    let mut copies = FxHashMap::<ValueId, (BlockId, ValueId)>::default();
    for (home, inst, result, target) in sunk {
        let instruction = func.inst(inst);
        let mut copy = Instruction::new(instruction.kind.clone(), instruction.result_ty);
        copy.metadata.preserve_debug_loc(&instruction.metadata);
        let (copy_inst, copy_value) = func.alloc_value_inst(copy);
        let position = func.blocks[target]
            .instructions
            .iter()
            .position(|&other| !matches!(func.inst(other).kind, InstKind::Phi(_)))
            .unwrap_or(func.blocks[target].instructions.len());
        func.blocks[target].instructions.insert(position, copy_inst);
        copies.insert(result, (home, copy_value));
    }
    // use of the read outside its block => use of the copy
    for block in func.blocks.indices() {
        let redirect = |value: &mut ValueId| {
            if let Some(&(home, copy)) = copies.get(value)
                && home != block
            {
                *value = copy;
            }
        };
        for inst in func.blocks[block].instructions.clone() {
            match &mut func.inst_mut(inst).kind {
                InstKind::Phi(incoming) => {
                    for (pred, value) in incoming {
                        if let Some(&(home, copy)) = copies.get(value)
                            && home != *pred
                        {
                            *value = copy;
                        }
                    }
                }
                kind => kind.visit_operands_mut(redirect),
            }
        }
        if let Some(term) = &mut func.blocks[block].terminator {
            term.visit_operands_mut(redirect);
        }
    }
    true
}

/// Moves computations on calldata words past the guard that precedes their readers; see the
/// module docs.
fn sink_guarded(func: &mut Function) -> bool {
    let guards = guards(func);
    if guards.is_empty() {
        return false;
    }
    // The readers of each value: an instruction, or `None` for a phi, which reads its input on
    // the edge from the predecessor in the block, and for a terminator.
    let mut uses = FxHashMap::<ValueId, SmallVec<[(BlockId, Option<InstId>); 2]>>::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            match &func.inst(inst).kind {
                InstKind::Phi(incoming) => {
                    for &(pred, operand) in incoming {
                        uses.entry(operand).or_default().push((pred, None));
                    }
                }
                kind => kind.visit_operands(|operand| {
                    uses.entry(operand).or_default().push((block, Some(inst)));
                }),
            }
        }
        if let Some(term) = &data.terminator {
            term.visit_operands(|operand| uses.entry(operand).or_default().push((block, None)));
        }
    }
    let cfg = CfgInfo::new(func);
    let mut moved = false;
    for (block, next) in guards {
        // The instructions that could follow the guard: each is movable, and read only after
        // the guard or by another of them. Dropping one that is not closed drops the
        // instructions that feed it, and the readers of one dropped for its operands.
        let mut producers = FxHashMap::<ValueId, InstId>::default();
        let mut members = FxHashSet::<InstId>::default();
        for &inst in &func.blocks[block].instructions {
            if let Some(result) = func.inst_result_value(inst)
                && movable(func, inst)
            {
                producers.insert(result, inst);
                members.insert(inst);
            }
        }
        let mut pending = members.iter().copied().collect::<Vec<_>>();
        while let Some(inst) = pending.pop() {
            if !members.contains(&inst) {
                continue;
            }
            let result = func.inst_result_value(inst).expect("movable instructions have results");
            let readers = uses.get(&result).map_or(&[][..], SmallVec::as_slice);
            let read_after = !readers.is_empty()
                && readers.iter().all(|&(reader, by)| {
                    by.is_some_and(|by| members.contains(&by))
                        || cfg.dominators().dominates(next, reader)
                });
            let mut operands = SmallVec::<[ValueId; 3]>::new();
            func.inst(inst).kind.visit_operands(|operand| operands.push(operand));
            let closed = operands.iter().all(|&operand| {
                // A calldata word read in an earlier block is read again after the guard only
                // when nothing in this block still reads it, so that no read is added.
                let read_again = func.is_constant_calldata_load(operand)
                    && uses.get(&operand).is_none_or(|readers| {
                        readers.iter().all(|&(reader, by)| {
                            reader != block || by.is_some_and(|by| members.contains(&by))
                        })
                    });
                func.value_u256(operand).is_some()
                    || is_member_result(&producers, &members, operand)
                    || read_again
            });
            if read_after && closed {
                continue;
            }
            members.remove(&inst);
            // The instructions that feed this one, read its result, or share its operands may no
            // longer qualify.
            let mut touched = operands.clone();
            touched.push(result);
            let neighbors = operands
                .iter()
                .filter_map(|operand| producers.get(operand).copied())
                .chain(
                    touched
                        .iter()
                        .flat_map(|value| uses.get(value).into_iter().flatten())
                        .filter_map(|&(_, by)| by),
                )
                .collect::<Vec<_>>();
            pending.extend(neighbors.into_iter().filter(|neighbor| members.contains(neighbor)));
        }
        let sunk = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|inst| members.contains(inst))
            .collect::<Vec<_>>();
        // A computation of constants alone is not worth a move, and a calldata read alone is
        // placed by `place-reads`.
        if !sunk.iter().any(|&inst| is_pure(func.inst(inst))) {
            continue;
        }
        for &inst in &sunk {
            let mut operands = SmallVec::<[ValueId; 3]>::new();
            func.inst(inst).kind.visit_operands(|operand| operands.push(operand));
            for operand in operands {
                for reader in uses.get_mut(&operand).into_iter().flatten() {
                    if *reader == (block, Some(inst)) {
                        reader.0 = next;
                    }
                }
            }
        }
        let (moving, staying) =
            func.blocks[block].instructions.iter().partition(|inst| members.contains(inst));
        func.blocks[block].instructions = staying;
        let start = func.blocks[next]
            .instructions
            .iter()
            .position(|&inst| !matches!(func.inst(inst).kind, InstKind::Phi(_)))
            .unwrap_or(func.blocks[next].instructions.len());
        func.blocks[next].instructions.splice(start..start, moving);
        moved = true;
    }
    moved
}

/// Whether `operand` is the result of an instruction that is still a member.
fn is_member_result(
    producers: &FxHashMap<ValueId, InstId>,
    members: &FxHashSet<InstId>,
    operand: ValueId,
) -> bool {
    producers.get(&operand).is_some_and(|producer| members.contains(producer))
}

/// Whether an instruction can run after a guard it ran before: a pure instruction, or a read of
/// calldata at a constant offset. A zero test stays, since `place-reads` places it by its reader.
fn movable(func: &Function, inst: InstId) -> bool {
    let instruction = func.inst(inst);
    let Some(result) = func.inst_result_value(inst) else { return false };
    let zero_test = matches!(instruction.kind, InstKind::Eq(a, b) | InstKind::Ne(a, b)
        if [a, b].iter().any(|&operand| func.value_u256(operand).is_some_and(|word| word.is_zero())));
    !matches!(instruction.kind, InstKind::Phi(_))
        && !zero_test
        && !instruction.kind.effects().observes_execution()
        && !instruction.kind.effects().control.any()
        && (is_pure(instruction) || func.is_constant_calldata_load(result))
}

/// Each block that ends in a guard, with the block that continues after it: a branch with one
/// successor that only reverts, and another that this block alone enters.
fn guards(func: &Function) -> Vec<(BlockId, BlockId)> {
    let reverts =
        |block: BlockId| func.blocks[block].terminator.as_ref().is_some_and(Terminator::reverts);
    func.blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            let Some(&Terminator::Branch { then_block, else_block, .. }) = data.terminator.as_ref()
            else {
                return None;
            };
            let next = match (reverts(then_block), reverts(else_block)) {
                (true, false) => else_block,
                (false, true) => then_block,
                _ => return None,
            };
            (next != block && func.blocks[next].predecessors.as_slice() == [block])
                .then_some((block, next))
        })
        .collect()
}
