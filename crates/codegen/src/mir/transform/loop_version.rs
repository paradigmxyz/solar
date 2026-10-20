//! Loop versioning on the words a loop's element stores cannot reach (`loop-version`).
//!
//! MIR's memory model lets any two memory references overlap: a caller may pass one object
//! twice, or assembly may retag a word as a reference into another object. So a loop that stores
//! elements of one array and reads the length of another must read that length again after
//! every store, and a paired sort that moves keys and values reloads both lengths before each
//! checked access. Check elimination cannot relate those fresh loads to the loop's counters, so
//! every access keeps its bounds check.
//!
//! This pass clones such a loop. A new block before it reads each word the loop loads from a
//! pointer defined before the loop, tests that no array the loop writes can reach any of those
//! words, and enters the clone when the test holds. The clone uses the test block's values in
//! place of the loads, so later passes see loop-invariant lengths; the original loop runs when
//! the test fails.
//!
//! ```text
//! test: lz = mload z ...; ok = (x | lx | ...) < 2^64 && !(z - (x + 1) < (lx << s) + 31 || ...)
//!       jumpi ok, fast, slow
//! fast: jump header'        // the clone, `mload z` replaced by `lz`
//! slow: jump header         // the original loop
//! ```
//!
//! # Recognition
//!
//! A natural loop with a preheader, exit blocks whose predecessors all lie in the loop, and at most
//! `MAX_LOOP_INSTRUCTIONS` instructions. An exit block that code outside the loop also enters, as
//! the `return false` of a search that also returns early before its loop, first gets a block of
//! its own on each edge from the loop, once the loop otherwise qualifies. Every instruction of the
//! loop that may write memory is an element store:
//!
//! - `mstore (x + (i << 5)) + 32` of a word element, or `mstore8 (x + i) + 32` of a byte, where `x`
//!   is defined before the loop;
//! - dominated by a `panic_if_zero<0x32> (lt i, n)` check of the same index, where `n` is `mload x`
//!   or a value defined before the loop, such as the length a fresh allocation stored.
//!
//! The words are the `mload z` of the loop whose pointer `z` is defined before the loop. A block
//! after the loop that reads a value the loop defines must lie behind exactly one exit block.
//!
//! # Safety
//!
//! The test requires every written array's pointer and bound below `2^64`, one comparison of
//! their bitwise OR, so its data range `[x + 32, x + 32 + (n << s))` does not wrap, and every
//! word's pointer `z` other than `x` to lie outside that range: `z <= x` keeps `[z, z + 32)` below
//! the data, and `z` at or past its end keeps it above. The word of `x` itself is its length,
//! below its data. `z` lies strictly between `x` and the range's end exactly when
//! `z - (x + 1) < (n << s) + 31` in wrapping arithmetic: for `z > x` the difference is exact, and
//! for `z <= x` it wraps above `2^256 - 2^64`, past every span below `2^69`.
//!
//! In the clone, the words hold their test-block values on entry. Each store writes inside its
//! array's data range: its index passed a check against `n`, which is either a value from before
//! the loop or the array's own length word, still at its test-block value. So no store of an
//! iteration changes a word, and by induction every replaced load returns what the original load
//! would have returned. The two copies run the same instructions otherwise, with the same effects
//! and failures. A check that compared with a replaced load now compares with the same value.
//!
//! Values the loop defines and the code after it reads reach that code through a phi in the one
//! exit block that every reading block lies behind, which merges the two copies; existing phis in
//! every exit block gain the clone's edges.
//!
//! # Profitability
//!
//! The copy pays when a bounds check of the loop still compares an index with a reloaded word that
//! another array's stores might reach: against a loop-invariant value, the check elimination after
//! memory lowering folds it. A check that a loop branch on the same comparison dominates, as a
//! short-circuit loop condition does, folds without a copy; a loop with only such checks would
//! save just the reloads, which a short loop does not repay, and stays as it is.
//!
//! Gas builds only, since the loop's code doubles. The test runs once per entry into the loop, and
//! the outermost qualifying loop is versioned, so its inner loops are covered; loops nested in a
//! versioned loop's original copy are left alone. The pass runs after the semantic check
//! elimination and jump threading, on the threaded graph where short-circuit conditions dominate
//! the checks they cover.

use super::loop_split::{clone_loop, retarget};
use crate::mir::{
    BlockId, Builtin, Callee, Function, FunctionBuilder, InstId, InstKind, MirType, Module,
    PanicCode, PtrLocation, RevertKind, Terminator, Value, ValueId,
    analysis::{AddressSpace, AliasAnalysis, CfgInfo, Loop, LoopAnalyzer},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
    utils::{rebuild_predecessors, split_edge},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Clones loops behind a run-time test that their element stores cannot reach the words they
/// reload.
pub(crate) struct LoopVersion;

impl MirPass for LoopVersion {
    fn name(&self) -> &'static str {
        "loop-version"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| version_function(func))
    }
}

/// Instructions a loop may hold; versioning duplicates all of them.
const MAX_LOOP_INSTRUCTIONS: usize = 400;

/// An array the loop writes: every store lands below `object + 32 + (bound << shift)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Written {
    object: ValueId,
    bound: Bound,
    /// Five for word elements, zero for bytes.
    shift: u8,
}

/// The value an element store's index was checked against.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Bound {
    /// `mload object`, the array's own length word.
    Length,
    /// A value defined before the loop.
    Value(ValueId),
}

struct Plan {
    header: BlockId,
    preheader: BlockId,
    exits: Vec<BlockId>,
    blocks: Vec<BlockId>,
    loop_blocks: FxHashSet<BlockId>,
    written: Vec<Written>,
    /// Each pointer whose word the loop loads, with those loads.
    words: Vec<(ValueId, Vec<InstId>)>,
}

/// What a qualifying loop needs next.
enum Planned {
    /// The loop can be versioned as it is.
    Version(Plan),
    /// Edges from the loop into exit blocks that code outside the loop also enters, which each
    /// need a block of their own first.
    Split(Vec<(BlockId, BlockId)>),
}

fn version_function(func: &mut Function) -> bool {
    let mut changed = false;
    // Blocks of loops already versioned, whose original copies are not versioned again.
    let mut done = FxHashSet::default();
    // Headers of loops whose shared exits were split. A loop asks once: a second request means
    // the split did not give it exits of its own, and it is left alone.
    let mut split = FxHashSet::default();
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let cfg = CfgInfo::new(func);
        let aa = AliasAnalysis::new(func);
        // Outermost loops first: a loop with more blocks cannot nest in one with fewer.
        let mut candidates = loops.all_loops().collect::<Vec<_>>();
        candidates.sort_by_key(|l| std::cmp::Reverse(l.blocks.count()));
        let Some(planned) =
            candidates.into_iter().filter(|l| !done.contains(&l.header)).find_map(|l| {
                plan(func, &cfg, &aa, l).filter(|planned| {
                    !matches!(planned, Planned::Split(_)) || split.insert(l.header)
                })
            })
        else {
            break;
        };
        match planned {
            Planned::Version(plan) => {
                done.extend(plan.blocks.iter().copied());
                apply(func, &plan);
            }
            // from -> exit  =>  from -> split -> exit, after which the loop's exits are its own
            // and the next round plans it again.
            Planned::Split(edges) => {
                rebuild_predecessors(func);
                for (from, exit) in edges {
                    split_edge(func, from, exit);
                }
            }
        }
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

/// The condition of a `panic_if_zero<0x32>` check.
fn array_check(kind: &InstKind) -> Option<ValueId> {
    let InstKind::ICall {
        function:
            Callee::Builtin(Builtin::Check {
                is_zero: true,
                failure: RevertKind::Panic(PanicCode::ArrayOutOfBounds),
            }),
        args,
    } = kind
    else {
        return None;
    };
    let [condition] = **args else { return None };
    Some(condition)
}

/// Matches an element address `(object + (index << 5)) + 32`, or `(object + index) + 32` for a
/// byte store. Returns the object, the index, and the shift.
fn element_address(
    func: &Function,
    address: ValueId,
    byte: bool,
) -> Option<(ValueId, ValueId, u8)> {
    let &InstKind::PtrAdd(inner, header) = inst_kind(func, address)? else { return None };
    if func.value_u64(header) != Some(32) {
        return None;
    }
    let &InstKind::PtrAdd(object, offset) = inst_kind(func, inner)? else { return None };
    if byte {
        return Some((object, offset, 0));
    }
    let &InstKind::Shl(shift, index) = inst_kind(func, offset)? else { return None };
    (func.value_u64(shift) == Some(5)).then_some((object, index, 5))
}

fn plan(func: &Function, cfg: &CfgInfo, aa: &AliasAnalysis, l: &Loop) -> Option<Planned> {
    let preheader = l.preheader?;
    let exits: Vec<BlockId> = l.exit_blocks.to_vec();
    let blocks: Vec<BlockId> = l.blocks.iter().collect();
    let mut loop_insts = FxHashSet::default();
    for &block in &blocks {
        loop_insts.extend(func.blocks[block].instructions.iter().copied());
    }
    if loop_insts.len() > MAX_LOOP_INSTRUCTIONS {
        return None;
    }
    // The loop's edges into exit blocks that code outside the loop also enters.
    let mut shared = Vec::new();
    for &exit in &exits {
        let preds = predecessors(func, exit);
        if preds.iter().any(|pred| !l.blocks.contains(*pred)) {
            shared.extend(
                preds.into_iter().filter(|&pred| l.blocks.contains(pred)).map(|pred| (pred, exit)),
            );
        }
    }
    let defined_in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(inst),
        _ => false,
    };

    // Checks of the loop by index: (check block, position, bound).
    let mut checks: FxHashMap<ValueId, Vec<(BlockId, usize, ValueId)>> = FxHashMap::default();
    for &block in &blocks {
        for (position, &inst) in func.blocks[block].instructions.iter().enumerate() {
            let Some(condition) = array_check(&func.inst(inst).kind) else { continue };
            let Some(&InstKind::Lt(index, bound)) = inst_kind(func, condition) else { continue };
            checks.entry(index).or_default().push((block, position, bound));
        }
    }

    let mut written = Vec::new();
    let mut words: Vec<(ValueId, Vec<InstId>)> = Vec::new();
    for &block in &blocks {
        for (position, &inst) in func.blocks[block].instructions.iter().enumerate() {
            let kind = &func.inst(inst).kind;
            if let &InstKind::Load(pointer, PtrLocation::Memory) = kind
                && !defined_in_loop(pointer)
            {
                // The clone reads each word once, as the type its loads give it: a word loaded
                // both as a pointer and as an integer is left alone.
                let ty = func.inst_result_value(inst).and_then(|result| func.value_ty(result));
                match words.iter_mut().find(|(word, _)| *word == pointer) {
                    Some((_, loads)) => {
                        let first = func.inst_result_value(loads[0]).and_then(|r| func.value_ty(r));
                        if first != ty {
                            return None;
                        }
                        loads.push(inst);
                    }
                    None => words.push((pointer, vec![inst])),
                }
                continue;
            }
            if array_check(kind).is_some() {
                continue;
            }
            if !aa.instruction_mod_ref(func, inst).writes_space(AddressSpace::Memory) {
                continue;
            }
            // Every write must be a checked element store.
            let (address, byte) = match *kind {
                InstKind::Store(address, _, PtrLocation::Memory) => (address, false),
                InstKind::Store8(address, _) => (address, true),
                _ => return None,
            };
            let (object, index, shift) = element_address(func, address, byte)?;
            if defined_in_loop(object) {
                return None;
            }
            let bound = checks.get(&index).and_then(|checks| {
                checks.iter().find_map(|&(check_block, check_position, bound)| {
                    let dominates = if check_block == block {
                        check_position < position
                    } else {
                        cfg.dominators().dominates(check_block, block)
                    };
                    if !dominates {
                        return None;
                    }
                    if !defined_in_loop(bound) {
                        return Some(Bound::Value(bound));
                    }
                    let &InstKind::Load(length, PtrLocation::Memory) = inst_kind(func, bound)?
                    else {
                        return None;
                    };
                    (length == object).then_some(Bound::Length)
                })
            })?;
            let entry = Written { object, bound, shift };
            if !written.contains(&entry) {
                written.push(entry);
            }
        }
    }
    // The copy pays when a bounds check still compares with a word another array's stores
    // might reach: against the preheader's value, check elimination can fold it. A check that
    // a loop branch on the same comparison dominates, as a short-circuit loop condition does,
    // folds anyway, and a loop with only such checks would just save the reloads, which a short
    // loop does not repay.
    let reachable = |word: ValueId| written.iter().any(|written| written.object != word);
    let length_of = |bound: ValueId| match inst_kind(func, bound) {
        Some(&InstKind::Load(word, PtrLocation::Memory))
            if defined_in_loop(bound) && !defined_in_loop(word) =>
        {
            Some(word)
        }
        _ => None,
    };
    let covered = |index: ValueId, word: ValueId, block: BlockId| {
        blocks.iter().any(|&branch| {
            branch != block
                && cfg.dominators().dominates(branch, block)
                && matches!(func.blocks[branch].terminator, Some(Terminator::Branch { condition, .. })
                    if matches!(inst_kind(func, condition), Some(&InstKind::Lt(i, b))
                        if i == index && length_of(b) == Some(word)))
        })
    };
    let rechecked = checks.iter().any(|(&index, checks)| {
        checks.iter().any(|&(block, _, bound)| {
            length_of(bound).is_some_and(|word| reachable(word) && !covered(index, word, block))
        })
    });
    if !rechecked {
        return None;
    }
    // A written array's own length word is loaded in the loop when its checks read it.
    for written in &written {
        if written.bound == Bound::Length && !words.iter().any(|(word, _)| *word == written.object)
        {
            return None;
        }
    }
    // Loop values read after the loop reach each reading block through the one exit block that
    // lies before it; an exit block's phis take them on the loop's edges.
    for (block, body) in func.blocks.iter_enumerated() {
        if l.blocks.contains(block) {
            continue;
        }
        let mut readers = Vec::new();
        for &inst in &body.instructions {
            if let InstKind::Phi(incoming) = &func.inst(inst).kind {
                for &(from, value) in incoming {
                    if defined_in_loop(value)
                        && !(exits.contains(&block) && l.blocks.contains(from))
                    {
                        readers.push(from);
                    }
                }
            } else if func.inst(inst).kind.operands().into_iter().any(defined_in_loop) {
                readers.push(block);
            }
        }
        if body.terminator.as_ref().is_some_and(|t| t.operands().into_iter().any(defined_in_loop)) {
            readers.push(block);
        }
        if readers.into_iter().any(|reader| owning_exit(cfg, &exits, reader).is_none()) {
            return None;
        }
    }
    if !shared.is_empty() {
        return Some(Planned::Split(shared));
    }
    let loop_blocks = blocks.iter().copied().collect();
    Some(Planned::Version(Plan {
        header: l.header,
        preheader,
        exits,
        blocks,
        loop_blocks,
        written,
        words,
    }))
}

/// The exit block a block after the loop lies behind: the one exit that dominates it.
fn owning_exit(cfg: &CfgInfo, exits: &[BlockId], block: BlockId) -> Option<BlockId> {
    let mut owners = exits.iter().filter(|&&exit| cfg.dominators().dominates(exit, block));
    let (&owner, None) = (owners.next()?, owners.next()) else { return None };
    Some(owner)
}

/// The predecessors of `block`, from the terminators.
fn predecessors(func: &Function, block: BlockId) -> Vec<BlockId> {
    func.blocks
        .iter_enumerated()
        .filter(|(_, body)| {
            body.terminator.as_ref().is_some_and(|t| t.successors().into_iter().any(|s| s == block))
        })
        .map(|(id, _)| id)
        .collect()
}

fn apply(func: &mut Function, plan: &Plan) {
    // Loop values the code after the loop reads, other than through the exits' phis, with the
    // exit block each reading block lies behind.
    let cfg = CfgInfo::new(func);
    let loop_insts: FxHashSet<InstId> =
        plan.blocks.iter().flat_map(|&b| func.blocks[b].instructions.iter().copied()).collect();
    let in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(inst),
        _ => false,
    };
    let owner = |block: BlockId| owning_exit(&cfg, &plan.exits, block).expect("planned");
    let mut escaping = Vec::new();
    for (block, body) in func.blocks.iter_enumerated() {
        if plan.loop_blocks.contains(&block) {
            continue;
        }
        let mut reads = Vec::new();
        for &inst in &body.instructions {
            let kind = &func.inst(inst).kind;
            match kind {
                InstKind::Phi(incoming) => {
                    for &(from, value) in incoming {
                        if !(plan.exits.contains(&block) && plan.loop_blocks.contains(&from)) {
                            reads.push((from, value));
                        }
                    }
                }
                _ => reads.extend(kind.operands().into_iter().map(|value| (block, value))),
            }
        }
        if let Some(terminator) = &body.terminator {
            reads.extend(terminator.operands().into_iter().map(|value| (block, value)));
        }
        for (reader, value) in reads {
            if in_loop(value) {
                let entry = (value, owner(reader));
                if !escaping.contains(&entry) {
                    escaping.push(entry);
                }
            }
        }
    }
    let exiting: FxHashMap<BlockId, Vec<BlockId>> =
        plan.exits.iter().map(|&exit| (exit, predecessors(func, exit))).collect();
    let owners: FxHashMap<BlockId, BlockId> = func
        .blocks
        .indices()
        .filter(|block| !plan.loop_blocks.contains(block))
        .filter_map(|block| Some((block, owning_exit(&cfg, &plan.exits, block)?)))
        .collect();

    let copy = clone_loop(func, &plan.blocks);

    // preheader: ... jump test
    // test: lz = mload z, ...; ok = ...; jumpi ok, fast, slow
    let test = func.alloc_block();
    let (terminator, metadata) = func.blocks[plan.preheader].take_terminator();
    let mut terminator = terminator.expect("the preheader is terminated");
    retarget(&mut terminator, |successor| if successor == plan.header { test } else { successor });
    func.blocks[plan.preheader].set_terminator(terminator, metadata);
    let mut builder = FunctionBuilder::new_semantic(func);
    builder.switch_to_block(test);
    let mut values = FxHashMap::default();
    for (word, loads) in &plan.words {
        // A pointer word stays a pointer, so the clone's addresses built from it keep their type.
        let ty = builder
            .func()
            .inst_result_value(loads[0])
            .and_then(|result| builder.func().value_ty(result))
            .unwrap_or(MirType::I256);
        let value = builder.load_as(*word, PtrLocation::Memory, ty);
        values.insert(*word, value);
    }
    let limit = builder.imm(U256::from(1) << 64);
    let one = builder.imm(1);
    let last_header_byte = builder.imm(31);
    let no = builder.imm_bool(false);
    let mut bits = builder.imm(0);
    let mut inside = no;
    for written in &plan.written {
        let bound = match written.bound {
            Bound::Length => values[&written.object],
            Bound::Value(bound) => bound,
        };
        let object = pointer_word(&mut builder, written.object);
        // bits = bits | object | bound
        bits = builder.or(bits, object);
        bits = builder.or(bits, bound);
        // first = object + 1; span = (bound << shift) + 31
        let shift = builder.imm(u64::from(written.shift));
        let size = builder.shl(bound, shift);
        let first = builder.add(object, one);
        let span = builder.add(size, last_header_byte);
        for &(word, _) in &plan.words {
            if word == written.object {
                continue;
            }
            // inside = inside || word - first < span
            let word = pointer_word(&mut builder, word);
            let offset = builder.sub(word, first);
            let within = builder.lt(offset, span);
            inside = builder.or(inside, within);
        }
    }
    // ok = bits < 2^64 && !inside
    let small = builder.lt(bits, limit);
    let clear = builder.eq(inside, no);
    let ok = builder.and(small, clear);
    let fast = builder.create_block();
    let slow = builder.create_block();
    builder.branch(ok, fast, slow);
    builder.switch_to_block(fast);
    builder.jump(copy.blocks[&plan.header]);
    builder.switch_to_block(slow);
    builder.jump(plan.header);

    // header: v = phi [preheader: init], ... -> [slow: init] / header': [fast: init]
    for (header, entry) in [(plan.header, slow), (copy.blocks[&plan.header], fast)] {
        for inst in func.blocks[header].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                for (from, _) in incoming.iter_mut() {
                    if *from == plan.preheader {
                        *from = entry;
                    }
                }
            }
        }
    }

    // The clone reads the preheader's words.
    let mut replacements = FxHashMap::default();
    for (word, loads) in &plan.words {
        for &load in loads {
            if let Some(result) = func.inst_result_value(load) {
                replacements.insert(copy.value(result), values[word]);
            }
        }
    }
    func.replace_uses(&replacements);
    for &block in copy.blocks.values() {
        let replaced = |inst: &InstId| {
            func.inst_result_value(*inst).is_some_and(|result| replacements.contains_key(&result))
        };
        let instructions = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|inst| !replaced(inst))
            .collect();
        func.blocks[block].instructions = instructions;
    }

    // exit: v = phi [block: x], ..., [block': x']
    for &exit in &plan.exits {
        for inst in func.blocks[exit].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = incoming
                    .iter()
                    .filter(|&&(from, _)| plan.loop_blocks.contains(&from))
                    .map(|&(from, value)| {
                        let value = copy.value(value);
                        (copy.blocks[&from], replacements.get(&value).copied().unwrap_or(value))
                    })
                    .collect();
                incoming.extend(cloned);
            }
        }
    }

    // exit: e = phi [exiting: v], [exiting': v']; uses behind the exit read e
    let mut merged = FxHashMap::default();
    for &(value, exit) in &escaping {
        let cloned = copy.value(value);
        let cloned = replacements.get(&cloned).copied().unwrap_or(cloned);
        let incoming = exiting[&exit]
            .iter()
            .flat_map(|&block| [(block, value), (copy.blocks[&block], cloned)])
            .collect();
        let mut builder = FunctionBuilder::new_semantic(func);
        builder.switch_to_block(exit);
        let phi = builder.phi(incoming);
        merged.insert((value, exit), phi);
    }
    if !merged.is_empty() {
        let new_phis: FxHashSet<InstId> = merged
            .values()
            .filter_map(|&phi| match func.value(phi) {
                Value::Inst(inst) => Some(*inst),
                _ => None,
            })
            .collect();
        // Move the new phis to the front of their exit blocks.
        for &exit in &plan.exits {
            let instructions = std::mem::take(&mut func.blocks[exit].instructions);
            let (phis, rest): (Vec<_>, Vec<_>) =
                instructions.into_iter().partition(|inst| new_phis.contains(inst));
            func.blocks[exit].instructions = phis.into_iter().chain(rest).collect();
        }
        let clone_blocks: FxHashSet<BlockId> = copy.blocks.values().copied().collect();
        let merged_at = |value: ValueId, reader: BlockId| {
            owners.get(&reader).and_then(|&exit| merged.get(&(value, exit)).copied())
        };
        for (block, body) in func.blocks.clone().iter_enumerated() {
            if plan.loop_blocks.contains(&block) || clone_blocks.contains(&block) {
                continue;
            }
            for &inst in &body.instructions {
                if new_phis.contains(&inst) {
                    continue;
                }
                let is_exit_phi = plan.exits.contains(&block);
                let kind = &mut func.inst_mut(inst).kind;
                if let InstKind::Phi(incoming) = kind {
                    for (from, value) in incoming.iter_mut() {
                        if is_exit_phi
                            && (plan.loop_blocks.contains(from) || clone_blocks.contains(from))
                        {
                            continue;
                        }
                        if let Some(phi) = merged_at(*value, *from) {
                            *value = phi;
                        }
                    }
                } else {
                    kind.visit_operands_mut(|value| {
                        if let Some(phi) = merged_at(*value, block) {
                            *value = phi;
                        }
                    });
                }
            }
            if let Some(mut terminator) = func.blocks[block].terminator.clone() {
                terminator.visit_operands_mut(|value| {
                    if let Some(phi) = merged_at(*value, block) {
                        *value = phi;
                    }
                });
                let metadata = func.blocks[block].terminator_metadata.clone();
                func.blocks[block].set_terminator(terminator, metadata);
            }
        }
    }
    rebuild_predecessors(func);
}

/// The word of a pointer value, for comparisons.
fn pointer_word(builder: &mut FunctionBuilder<'_>, value: ValueId) -> ValueId {
    match builder.func().value_ty(value) {
        Some(MirType::I256) | None => value,
        Some(_) => builder.cast(value, MirType::I256),
    }
}
