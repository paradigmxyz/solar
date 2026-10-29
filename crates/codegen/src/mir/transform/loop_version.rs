//! Loop versioning on facts a run-time test establishes for a copy (`loop-version`): the words a
//! loop's element stores cannot reach, the fences a two-pointer partition's scans stop at, or the
//! lengths a counted loop's bound stays within.
//!
//! MIR's memory model lets any two memory references overlap: a caller may pass one object
//! twice, or assembly may retag a word as a reference into another object. So a loop that stores
//! elements of one array and reads the length of another must read that length again after
//! every store, and a paired sort that moves keys and values reloads both lengths before each
//! checked access. Check elimination cannot relate those fresh loads to the loop's counters, so
//! every access keeps its bounds check.
//!
//! This pass clones such a loop. A new block reads each word the loop loads from a pointer
//! defined before the loop, tests that no array the loop writes can reach any of those words,
//! and enters the clone when the test holds. The clone uses the test block's values in place of
//! the loads, so later passes see loop-invariant lengths; the original loop runs when the test
//! fails. The test runs before the loop, or, when the loop reads a word only behind its trip test
//! or a check, on the loop's back edge after the first iteration (see Safety).
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
//! - dominated by a `panic_if_zero<0x32> (lt i, n)` check of the same index, or an index from which
//!   right shifts derive it, where `n` is `mload x` or a value defined before the loop, such as the
//!   length a fresh allocation stored.
//!
//! A packed hash the lowering builds in the scratch space, such as `keccak256(abi.encode(a, b))`,
//! writes only below `0x40` and may sit in the loop as well.
//!
//! The words are the `mload z` of the loop whose pointer `z` is defined before the loop and that
//! the test may read where it runs. A block after the loop that reads a value the loop defines
//! must lie behind exactly one exit block.
//!
//! # Safety
//!
//! The test must not read memory the loop would not. A read expands memory up to its address,
//! which may be an offset the caller chooses, as for an element at a loop-invariant index behind
//! its bounds check, and a pointer does not prove that it addresses a real object. Before the
//! loop, the loop may run no iteration, or fail or leave before its first read of a word. So a
//! test before the loop reads a word only when every path to the loop already loaded or stored
//! it, which memory then covers, or when the loop reads it on every entry before anything that
//! may fail, end the call, leave the loop, or read the memory size. Otherwise the test sits on the
//! loop's one back edge, after the first iteration's trip test and checks, and reads only words
//! that every iteration reads before it reaches that edge, or that every path to the loop
//! accessed. The loop's other loads stay loads in both copies, and a loop whose written array's
//! own length word the test cannot read is not versioned.
//!
//! The test requires every written array's pointer and bound below `2^64`, one comparison of
//! their bitwise OR, so its data range `[x + 32, x + 32 + (n << s))` does not wrap, and every
//! word's pointer `z` other than `x` to lie outside that range: `z <= x` keeps `[z, z + 32)` below
//! the data, and `z` at or past its end keeps it above. The word of `x` itself is its length,
//! below its data. `z` lies strictly between `x` and the range's end exactly when
//! `z - (x + 1) < (n << s) + 31` in wrapping arithmetic: for `z > x` the difference is exact, and
//! for `z <= x` it wraps above `2^256 - 2^64`, past every span below `2^69`. When the loop hashes
//! in the scratch space up to `e`, the test also requires every word's pointer `z >= e`, so
//! `[z, z + 32)` lies above the hash's stores.
//!
//! In the clone, the words hold their test-block values on entry. Each store writes inside its
//! array's data range: its index passed a check against `n`, which is either a value from before
//! the loop or the array's own length word, still at its test-block value. So no store of an
//! iteration changes a word, and by induction every replaced load returns what the original load
//! would have returned. The two copies run the same instructions otherwise, with the same effects
//! and failures. A check that compared with a replaced load now compares with the same value. A
//! test on the back edge enters the clone's header with the values the back edge carries, so the
//! clone runs the remaining iterations; when it fails, the original loop runs the next iteration
//! and tests again after it.
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
//! Gas builds only, since the loop's code doubles. The test runs once per entry into the loop:
//! before it, or after its first iteration, which then keeps its reloads and checks, while a loop
//! that never runs skips the test. A test on the back edge that fails, for arrays that overlap,
//! runs again after every further iteration. The test goes before the loop whenever the words it
//! can read there make the copy pay, since the clone's counters then start from values before the
//! loop, which check elimination relates to the lengths read there. The outermost qualifying loop
//! is versioned, so its inner loops are covered; loops nested in either copy of a versioned loop
//! are left alone. The pass runs after the semantic check elimination and jump threading, on the
//! threaded graph where short-circuit conditions dominate the checks they cover.
//!
//! # Partitions
//!
//! A Hoare partition runs two scans of one array toward each other, `do ++i; while (a[i] < y)`
//! and `do --j; while (y < a[j])`, and swaps the elements they stop at while `i < j`. Each scan
//! stops at the latest at the element the other one starts from, which a median of three placed,
//! or a swap moved there, but the checks of `a[i]` against the length and of `--j` against zero
//! cannot see that, and cost a compare and a branch on every step. The pass copies such a loop
//! without those checks, behind a test of its entry that the swaps then keep true:
//!
//! ```text
//! test: ok = lt low, high && lt high, n; jumpi ok, read, slow
//! read: up = <a[++i] would continue at a[high]>; down = <a[--j] would continue at a[low]>
//!       jumpi (up || down), slow, fast
//! fast: jump header'        // the clone, its scans without those checks
//! slow: jump header         // the original loop
//! ```
//!
//! A scan is a single-block loop whose one phi is a counter that its update steps by one,
//! `++i` or `--j`, checked or not, that reads one element of an array at the updated counter,
//! checks that index only against an invariant bound or the array's header, computes nothing else
//! with an effect, and leaves on a condition of the element and values defined before it. A bound
//! reloaded in either scan or the enclosing loop may be the array's header when every path to the
//! partition already accessed it. The test reads it once, requires the array below `2^64`, and
//! replaces the copy's header loads. With fences below `2^250`, no swap address can wrap into that
//! header. The loop
//! qualifies when exactly two scans of one array lie in it, one counting up and one down, each
//! starting from a phi of its header whose input on the back edge is the scan's own last index;
//! when a branch on `lt i, j` leaves the loop where it fails and its other edge, the only way
//! into its target, dominates the single latch; and when the latch is the loop's only writer of
//! memory, with exactly `x = mload a[i]; y = mload a[j]; mstore a[j], x; mstore a[i], y`, both
//! loads before both stores. Every path into the loop must already have read or written the
//! elements at both phis' entry values, `low` and `high`, so the test sits before the loop
//! without reading memory the loop would not (see Safety). The scans' exit conditions read only
//! their element and values defined before the loop, so the test evaluates them as every
//! iteration does.
//!
//! Safety, by induction over the iterations. On entry the test establishes `low < high < 2^250`,
//! `high < n` for each bound `n` the scans check, and that the scan up stops at `a[high]` and
//! the scan down at `a[low]`. The loop writes memory only in its latch, after both scans, so in
//! an iteration the scan up visits indices from `i + 1` and stops at or before its fence `j`, which
//! lies below `n`; its update cannot wrap and its checks hold. The scan down visits from `j - 1`
//! down and stops at or after its fence `i >= 0`, so its update never decrements zero. The latch
//! runs only past `i' < j'`; it stores the element that stopped the scan up, which no write has
//! changed since the scan read it, at `j'`, and the one that stopped the scan down at `i'`, so the
//! next iteration starts the scan up at `i'` below its fence `j'`, which lies below the last fence
//! `j` and so below every `n`, and the scan down at `j'` above its fence `i'`, each fence an
//! element that stops its scan. The copy drops both scans' checks of their indices and both scans'
//! overflow checks; all others stay, and the copy runs the same instructions otherwise, except
//! that it compares the addresses of the elements the scans stopped at instead of `i' < j'`:
//! both lie between the fences, whose indices below `2^250` scale to offsets below `2^255`. Their
//! addresses wrap around the address space at most once, and all alike, as the test read `a[low]`
//! below `2^64` unless its address wrapped too, so an element's address grows with its index there
//! and the comparisons agree. A length forged in assembly can let a check pass for a larger index,
//! whose address wraps below the array. Then no test reads the scans' counters, and
//! induction-variable simplification steps the copy's scans as pointers that its loop carries.
//!
//! The test reads two words and compares twice once per entry into the loop, which a partition of
//! a dozen elements already repays: every step of either scan saves a compare and a branch.
//!
//! # Bounds
//!
//! A counted loop whose trip test bounds its counter by a value from before the loop, `c < n`,
//! may check its accesses against lengths, also read before the loop, that check elimination
//! cannot relate to `n`: a heap's sift loop, bounded by its queue's size, checks every access
//! against the lengths of the arrays that hold the queue. Each such check compares an index below
//! `n` with a length on every iteration. The pass copies such a loop behind a test that `n` does
//! not exceed any of those lengths:
//!
//! ```text
//! test: short = or (lt len1, n), (lt len2, n), ...; jumpi short, slow, fast
//! fast: jump header'        // the clone, unchanged
//! slow: jump header         // the original loop
//! ```
//!
//! The clone runs the loop's own instructions, and check elimination folds the checks the edge
//! into it covers: an index below `n` stays below every length `n` does not exceed. The test
//! compares only values the program computed before the loop, so it reads no memory and cannot
//! fail. The copy pays when a check of the loop compares a length from before the loop with an
//! index that stays below `n`: the counter, a right shift of it, the counter plus a constant
//! behind a loop branch that tests that sum against `n`, or a phi of such indices. A length that a
//! bounds check on every path to the loop already compares `n` with needs no copy.
//!
//! An array's own indexed stores may wrap into its header, so its length can remain a load inside
//! the loop. For a counted loop that writes one array, the words test first proves its pointer
//! and length below `2^64`, making those stores non-wrapping. The bounds test then compares `n`
//! with the guarded length, and the clone reuses that length. The header must already be safe to
//! read before the loop under the same rule as other word tests; a zero-trip loop never gains a
//! read of an otherwise untouched pointer. Large forged lengths select the original loop.

use super::loop_split::{LoopCopy, clone_loop, retarget};
use crate::mir::{
    ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, EffectKind, Function, FunctionBuilder,
    InstId, InstKind, Instruction, MirType, Module, PanicCode, PtrLocation, RevertKind, Terminator,
    Value, ValueId,
    analysis::{AddressSpace, AliasAnalysis, CfgInfo, Loop, LoopAnalyzer, LoopInfo},
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

/// A partition's copy runs only for fences below `2^250`. An index below it scales to an offset
/// below `2^255`, so the addresses of the elements between the fences wrap around the address
/// space at most once, and all alike. A length forged larger in assembly lets a check pass for an
/// index whose address wraps below the array.
const FENCE_INDEX_BITS: usize = 250;

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
    test: Test,
    /// The block whose back edge the test sits on, or `None` for a test before the loop.
    latch: Option<BlockId>,
}

/// What the run-time test before a versioned loop establishes for its copy.
enum Test {
    /// No array the loop writes reaches the words it reloads.
    Words {
        written: Vec<Written>,
        /// Each pointer whose word the test reads, with the loop's loads of it.
        words: Vec<(ValueId, Vec<InstId>)>,
        /// The end of the scratch space the loop's packed hashes write, zero if none.
        scratch_end: u64,
        /// A counted loop's limit and the reloaded length it must stay within.
        bounds: Option<(ValueId, ValueId)>,
    },
    /// A two-pointer partition's scans stop at their fences before leaving the range their checks
    /// guard.
    Fence(Box<Fence>),
    /// A counted loop's limit, the bound of its trip test, does not exceed the lengths its checks
    /// compare with.
    Bounds { limit: ValueId, lengths: Vec<ValueId> },
}

/// A scan: a single-block loop that steps a counter by one, reads the element of an array at
/// the updated counter, `a[++i]` or `a[--j]`, and leaves at the first element its exit
/// condition rejects.
struct Scan {
    header: BlockId,
    /// The counter's value on entry.
    start: ValueId,
    /// The counter's update, the index the loop reads.
    update: InstId,
    next: ValueId,
    ascending: bool,
    array: ValueId,
    load: InstId,
    /// Positions in the loop block of the checks of the read index against `bound`.
    checks: Vec<usize>,
    bound: Option<ValueId>,
    /// The exit condition's instructions over the element, in order, and its value.
    condition: Vec<InstId>,
    test: ValueId,
    /// Whether the loop continues while the exit condition holds.
    continues_on_true: bool,
}

/// A loop that runs two scans of one array toward each other and swaps the elements they stop
/// at, a Hoare partition: the scan up starts where the last one stopped and runs until the
/// element the scan down last stopped at, which rejects it, and the other way round.
struct Fence {
    up: Scan,
    down: Scan,
    /// The array's header loads replaced by a guarded read before the loop.
    lengths: Vec<InstId>,
    /// The scans' starts on entry into the loop: the first is below the second.
    low: ValueId,
    high: ValueId,
    /// The comparison of the scans' last indices, `lt i', j'`, past which the latch runs.
    order: InstId,
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
    // Blocks of loops already versioned, whose copies are not versioned again.
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
        let accesses = element_accesses(func);
        let Some(planned) =
            candidates.into_iter().filter(|l| !done.contains(&l.header)).find_map(|l| {
                plan(func, &cfg, &aa, l)
                    .or_else(|| plan_partition(func, &cfg, &aa, &loops, &accesses, l))
                    .or_else(|| plan_bounds(func, &cfg, l))
                    .filter(|planned| {
                        !matches!(planned, Planned::Split(_)) || split.insert(l.header)
                    })
            })
        else {
            break;
        };
        match planned {
            Planned::Version(plan) => {
                done.extend(plan.blocks.iter().copied());
                done.extend(apply(func, &plan));
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
    let blocks: Vec<BlockId> = l.blocks.iter().collect();
    let mut loop_insts = FxHashSet::default();
    for &block in &blocks {
        loop_insts.extend(func.blocks[block].instructions.iter().copied());
    }
    if loop_insts.len() > MAX_LOOP_INSTRUCTIONS {
        return None;
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
    let mut scratch_end = 0;
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
            // A packed hash built in the scratch space writes only below its end.
            if let InstKind::AbiEncodePacked { parts, hash: true } = kind
                && let Some(end) = super::lower_packed::scratch_hash_end(parts)
            {
                scratch_end = scratch_end.max(end);
                continue;
            }
            // Every other write must be a checked element store.
            let (address, byte) = match *kind {
                InstKind::Store(address, _, PtrLocation::Memory) => (address, false),
                InstKind::Store8(address, _) => (address, true),
                _ => return None,
            };
            let (object, index, shift) = element_address(func, address, byte)?;
            if defined_in_loop(object) {
                return None;
            }
            let bound_for = |checked| {
                checks.get(&checked).and_then(|checks| {
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
                })
            };
            let mut checked = index;
            let mut bound = None;
            for _ in 0..=4 {
                bound = bound_for(checked);
                if bound.is_some() {
                    break;
                }
                // A right shift never exceeds its unsigned operand. The check of that operand
                // must still dominate the store, as required for an exact index above.
                let Some(&InstKind::Shr(shift, value)) = inst_kind(func, checked) else { break };
                if func.value_u64(shift).is_none() {
                    break;
                }
                checked = value;
            }
            let bound = bound?;
            let entry = Written { object, bound, shift };
            if !written.contains(&entry) {
                written.push(entry);
            }
        }
    }
    // The copy pays when a bounds check still compares with a word another array's stores
    // might reach: against the test's value, check elimination can fold it. A check that a loop
    // branch on the same comparison dominates, as a short-circuit loop condition does, folds
    // anyway, and a loop with only such checks would just save the reloads, which a short loop
    // does not repay. A written array's own length word must be among the words when its checks
    // read it.
    let reachable =
        |word: ValueId| scratch_end != 0 || written.iter().any(|written| written.object != word);
    let pays = |words: &[(ValueId, Vec<InstId>)]| {
        let read = |word: ValueId| words.iter().any(|&(read, _)| read == word);
        let length_of = |bound: ValueId| match inst_kind(func, bound) {
            Some(&InstKind::Load(word, PtrLocation::Memory))
                if defined_in_loop(bound) && read(word) =>
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
        rechecked
            && written.iter().all(|written| written.bound != Bound::Length || read(written.object))
    };
    // The test reads only words the program reads anyway, so it expands no memory the loop would
    // not: words every path to the loop accessed, and, before the loop, words the loop reads
    // first on every entry, or, on the one back edge, words every iteration reads before that
    // edge. A word the loop reads only behind its trip test or a check, such as an element at a
    // loop-invariant index, may lie at an offset the caller chooses.
    let accessed = accessed_words(func, cfg, preheader);
    let readable = |ran: &dyn Fn(InstId) -> bool| {
        words
            .iter()
            .filter(|(word, loads)| accessed.contains(word) || loads.iter().any(|&load| ran(load)))
            .cloned()
            .collect::<Vec<_>>()
    };
    let before = readable(&|load| read_first(func, cfg, l, &blocks, load));
    let (words, latch, bounds) = if pays(&before) {
        (before, None, None)
    } else if let Some(bounds) = reloaded_bounds(func, l, &blocks, &written, &before) {
        (before, None, Some(bounds))
    } else {
        let &[latch] = &l.back_edges[..] else { return None };
        let after = readable(&|load| {
            let block =
                blocks.iter().find(|&&block| func.blocks[block].instructions.contains(&load));
            block.is_some_and(|&block| cfg.dominators().dominates(block, latch))
        });
        if !pays(&after) {
            return None;
        }
        (after, Some(latch), None)
    };
    version_plan(
        func,
        cfg,
        l,
        preheader,
        blocks,
        Test::Words { written, words, scratch_end, bounds },
        latch,
    )
}

/// The blocks that read or write each element of each array, by array and index.
type Accesses = FxHashMap<(ValueId, ValueId), Vec<BlockId>>;

/// The element accesses of the function, `mload` or `mstore` at `(array + (index << 5)) + 32`.
/// An access at a phi of such addresses, one per input of a phi of their indices in the same
/// block, as a versioned loop's exit merges both copies' last element, accesses the element at
/// the index phi.
fn element_accesses(func: &Function) -> Accesses {
    let mut accesses = Accesses::default();
    for (block, body) in func.blocks.iter_enumerated() {
        for &inst in &body.instructions {
            let (InstKind::Load(address, PtrLocation::Memory)
            | InstKind::Store(address, _, PtrLocation::Memory)) = func.inst(inst).kind
            else {
                continue;
            };
            for (array, index) in element_indices(func, address) {
                let blocks = accesses.entry((array, index)).or_default();
                if !blocks.contains(&block) {
                    blocks.push(block);
                }
            }
        }
    }
    accesses
}

/// The elements an address reaches: itself, or a phi's index whose inputs match the inputs of
/// an address phi.
fn element_indices(func: &Function, address: ValueId) -> Vec<(ValueId, ValueId)> {
    if let Some((array, index, 5)) = element_address(func, address, false) {
        return vec![(array, index)];
    }
    let Value::Inst(inst) = *func.value(address) else { return Vec::new() };
    let InstKind::Phi(addresses) = &func.inst(inst).kind else { return Vec::new() };
    let Some(elements) = addresses
        .iter()
        .map(|&(from, address)| match element_address(func, address, false) {
            Some((array, index, 5)) => Some((from, array, index)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    let Some(&(_, array, _)) = elements.first() else { return Vec::new() };
    if elements.iter().any(|&(_, other, _)| other != array) {
        return Vec::new();
    }
    let Some(block) = func.blocks.iter().find(|body| body.instructions.contains(&inst)) else {
        return Vec::new();
    };
    block
        .instructions
        .iter()
        .filter_map(|&other| {
            let InstKind::Phi(indices) = &func.inst(other).kind else { return None };
            let matches = indices.len() == elements.len()
                && elements.iter().all(|&(from, _, index)| indices.contains(&(from, index)));
            matches.then(|| func.inst_result_value(other)).flatten().map(|index| (array, index))
        })
        .collect()
}

/// Plans a two-pointer loop's copy whose scans run without the checks their fences make
/// redundant. See [`Fence`].
fn plan_partition(
    func: &Function,
    cfg: &CfgInfo,
    aa: &AliasAnalysis,
    loops: &LoopInfo,
    accesses: &Accesses,
    l: &Loop,
) -> Option<Planned> {
    let preheader = l.preheader?;
    let &[latch] = l.back_edges.as_slice() else { return None };
    let blocks: Vec<BlockId> = l.blocks.iter().collect();
    let instruction_count: usize = blocks.iter().map(|&b| func.blocks[b].instructions.len()).sum();
    if instruction_count > MAX_LOOP_INSTRUCTIONS {
        return None;
    }
    let in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => blocks.iter().any(|&b| func.blocks[b].instructions.contains(inst)),
        _ => false,
    };
    // The two scans, each starting at a phi of the header.
    let scans: Vec<Scan> = loops
        .all_loops()
        .filter(|inner| inner.header != l.header && l.blocks.contains(inner.header))
        .filter_map(|inner| scan(func, inner))
        .collect();
    let [first, second] = <[Scan; 2]>::try_from(scans).ok()?;
    let (up, down) = match (first.ascending, second.ascending) {
        (true, false) => (first, second),
        (false, true) => (second, first),
        _ => return None,
    };
    if up.array != down.array || in_loop(up.array) {
        return None;
    }
    // The copy pays for the test when its scans drop a check every step: the scan up's checks
    // of its index, or an update's overflow check. A copy has none left, and is not copied again.
    let checked_update =
        |scan: &Scan| matches!(func.inst(scan.update).kind, InstKind::CheckedBinary { .. });
    if up.checks.is_empty() && !checked_update(&up) && !checked_update(&down) {
        return None;
    }
    let array = up.array;
    // header: i = phi [preheader: low], [latch: i']; j = phi [preheader: high], [latch: j']
    let header_inputs = |value: ValueId| -> Option<(ValueId, ValueId)> {
        let Value::Inst(inst) = *func.value(value) else { return None };
        if !func.blocks[l.header].instructions.contains(&inst) {
            return None;
        }
        let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
        if incoming.len() != 2 {
            return None;
        }
        let entry = incoming.iter().find(|&&(from, _)| from == preheader)?.1;
        let back = incoming.iter().find(|&&(from, _)| from == latch)?.1;
        Some((entry, back))
    };
    let (low, up_back) = header_inputs(up.start)?;
    let (high, down_back) = header_inputs(down.start)?;
    if up_back != up.next || down_back != down.next {
        return None;
    }
    // The latch repeats the loop only past `next up < next down`, the scans' order: a branch on
    // it leaves the loop when it fails, and its other edge, the only way into its target,
    // dominates the latch.
    let order = blocks.iter().find_map(|&block| {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block].terminator
        else {
            return None;
        };
        let Value::Inst(test) = *func.value(condition) else { return None };
        (matches!(func.inst(test).kind, InstKind::Lt(a, b) if a == up.next && b == down.next)
            && cfg.predecessors(then_block) == [block]
            && cfg.dominators().dominates(then_block, latch)
            && !l.blocks.contains(else_block))
        .then_some(test)
    })?;
    // The loop writes memory only in its latch, where it swaps the elements the scans stopped
    // at: `x = mload a[next up]; y = mload a[next down]; mstore a[next down], x; mstore a[next
    // up], y`, both loads before both stores.
    let element_at =
        |address: ValueId, index: ValueId| element_indices(func, address).contains(&(array, index));
    let mut stores = Vec::new();
    for &block in &blocks {
        for (position, &inst) in func.blocks[block].instructions.iter().enumerate() {
            if !aa.instruction_mod_ref(func, inst).writes_space(AddressSpace::Memory) {
                continue;
            }
            let InstKind::Store(address, value, PtrLocation::Memory) = func.inst(inst).kind else {
                return None;
            };
            if block != latch {
                return None;
            }
            stores.push((position, address, value));
        }
    }
    let [(first_store, ..), (second_store, ..)] = stores[..] else { return None };
    let swapped = |stopped: ValueId, other: ValueId| {
        stores.iter().any(|&(_, address, value)| {
            element_at(address, other)
                && match func.value(value) {
                    Value::Inst(load) => {
                        let load_position =
                            func.blocks[latch].instructions.iter().position(|inst| inst == load);
                        load_position.is_some_and(|p| p < first_store.min(second_store))
                            && matches!(func.inst(*load).kind,
                                InstKind::Load(from, PtrLocation::Memory) if element_at(from, stopped))
                    }
                    _ => false,
                }
        })
    };
    if !swapped(up.next, down.next) || !swapped(down.next, up.next) {
        return None;
    }
    // The fences on entry are elements every path into the loop has read or written, so the
    // test reads no memory the loop would not have, and lie at addresses that do not wrap.
    let touched = |index: ValueId| {
        accesses.get(&(array, index)).is_some_and(|blocks| {
            blocks.iter().any(|&access| cfg.dominators().dominates(access, preheader))
        })
    };
    if !touched(low) || !touched(high) {
        return None;
    }
    // The exit conditions compute, from the element, only pure functions of values defined
    // before the loop, so the test evaluates them as every iteration does, with the loop's own
    // copies of such functions, as an address's widened pivot, cloned in front of them.
    let loop_insts: Vec<InstId> =
        blocks.iter().flat_map(|&b| func.blocks[b].instructions.iter().copied()).collect();
    let mut up = up;
    let mut down = down;
    for scan in [&mut up, &mut down] {
        let element = func.inst_result_value(scan.load)?;
        scan.condition = exit_condition(func, &loop_insts, scan.test, element)?;
    }
    // A header read within the partition cannot be hoisted without proving that its swaps stay
    // above the header. The fence test proves that for its copy, with a low array address and
    // bounded indices; the original loop retains its reloads and checks.
    let reloads_length = [&up, &down].into_iter().any(|scan| scan.bound.is_some_and(in_loop));
    let lengths = if reloads_length {
        if !accessed_words(func, cfg, preheader).contains(&array)
            || [&up, &down].into_iter().any(|scan| {
                scan.bound.is_some_and(|bound| {
                    in_loop(bound)
                        && !matches!(inst_kind(func, bound),
                            Some(&InstKind::Load(address, PtrLocation::Memory)) if address == array)
                })
            })
        {
            return None;
        }
        loop_insts
            .iter()
            .copied()
            .filter(|&inst| {
                matches!(func.inst(inst).kind,
                    InstKind::Load(address, PtrLocation::Memory) if address == array)
                    && func.inst(inst).result_ty == Some(MirType::I256)
            })
            .collect()
    } else {
        Vec::new()
    };
    let fence = Fence { up, down, lengths, low, high, order };
    version_plan(func, cfg, l, preheader, blocks, Test::Fence(Box::new(fence)), None)
}

/// Recognizes a scan. See [`Scan`].
fn scan(func: &Function, l: &Loop) -> Option<Scan> {
    let header = l.header;
    if l.blocks.count() != 1 {
        return None;
    }
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[header].terminator
    else {
        return None;
    };
    let continues_on_true = match (then_block == header, else_block == header) {
        (true, false) => true,
        (false, true) => false,
        _ => return None,
    };
    let instructions = &func.blocks[header].instructions;
    let in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => instructions.contains(inst),
        _ => false,
    };
    // header: counter = phi [preheader: start], [header: next]; next = counter +- 1
    let [phi, ..] = instructions[..] else { return None };
    let InstKind::Phi(incoming) = &func.inst(phi).kind else { return None };
    let counter = func.inst_result_value(phi)?;
    if func.value_ty(counter) != Some(MirType::I256) || incoming.len() != 2 {
        return None;
    }
    let start = incoming.iter().find(|&&(from, _)| from != header)?.1;
    let next = incoming.iter().find(|&&(from, _)| from == header)?.1;
    let Value::Inst(update) = *func.value(next) else { return None };
    if !instructions.contains(&update) {
        return None;
    }
    let one = |value: ValueId| func.value_u64(value) == Some(1);
    let ascending = match func.inst(update).kind {
        InstKind::Add(a, b) if a == counter && one(b) || b == counter && one(a) => true,
        InstKind::Sub(a, b) if a == counter && one(b) => false,
        InstKind::CheckedBinary {
            op: op @ (CheckedOp::Add | CheckedOp::Sub),
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        } if lhs == counter && one(rhs) => op == CheckedOp::Add,
        _ => return None,
    };
    // The one element load, at the update, and the checks of that index.
    let mut load = None;
    let mut header_loads = Vec::new();
    let mut bound = None;
    let mut checks = Vec::new();
    for (position, &inst) in instructions.iter().enumerate() {
        if inst == phi || inst == update {
            continue;
        }
        let kind = &func.inst(inst).kind;
        if let InstKind::Load(address, PtrLocation::Memory) = *kind {
            let Some((array, index, 5)) = element_address(func, address, false) else {
                header_loads.push((inst, address));
                continue;
            };
            if load.is_some() || in_loop(array) || index != next {
                return None;
            }
            load = Some((inst, array));
            continue;
        }
        if let Some(check) = array_check(kind) {
            let Some(&InstKind::Lt(index, limit)) = inst_kind(func, check) else { return None };
            if index != next
                || (in_loop(limit)
                    && !matches!(
                        inst_kind(func, limit),
                        Some(InstKind::Load(_, PtrLocation::Memory))
                    ))
                || bound.is_some_and(|bound| bound != limit)
            {
                return None;
            }
            bound = Some(limit);
            checks.push(position);
            continue;
        }
        if kind.effect_kind() != EffectKind::Pure
            || kind.effects().control.any()
            || matches!(kind, InstKind::Phi(_))
            || func.inst(inst).metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
        {
            return None;
        }
    }
    let (load, array) = load?;
    if header_loads.iter().any(|&(inst, address)| {
        address != array || func.inst(inst).result_ty != Some(MirType::I256)
    }) {
        return None;
    }
    let element = func.inst_result_value(load)?;
    let condition_chain = exit_condition(func, instructions, condition, element)?;
    Some(Scan {
        header,
        start,
        update,
        next,
        ascending,
        array,
        load,
        checks,
        bound,
        condition: condition_chain,
        test: condition,
        continues_on_true,
    })
}

/// The instructions of a loop block that compute `condition` from `element` and values defined
/// before the loop, in order; `None` when it reads any other value the loop defines, or an
/// instruction with an effect.
fn exit_condition(
    func: &Function,
    instructions: &[InstId],
    condition: ValueId,
    element: ValueId,
) -> Option<Vec<InstId>> {
    let mut order = Vec::new();
    let mut reads_element = false;
    let mut work = vec![(condition, false)];
    let mut seen = FxHashSet::default();
    while let Some((value, expanded)) = work.pop() {
        if value == element {
            reads_element = true;
            continue;
        }
        let Value::Inst(inst) = *func.value(value) else { continue };
        if !instructions.contains(&inst) {
            continue;
        }
        if expanded {
            order.push(inst);
            continue;
        }
        if !seen.insert(inst) {
            continue;
        }
        let kind = &func.inst(inst).kind;
        if kind.effect_kind() != EffectKind::Pure
            || kind.effects().control.any()
            || matches!(kind, InstKind::Phi(_))
        {
            return None;
        }
        work.push((value, true));
        work.extend(kind.operands().into_iter().map(|operand| (operand, false)));
    }
    reads_element.then_some(order)
}

/// Recognizes a counted loop whose checks compare indices below its trip test's limit with
/// lengths from before the loop. See the module docs' Bounds section.
fn plan_bounds(func: &Function, cfg: &CfgInfo, l: &Loop) -> Option<Planned> {
    let preheader = l.preheader?;
    let blocks: Vec<BlockId> = l.blocks.iter().collect();
    let loop_insts: FxHashSet<InstId> =
        blocks.iter().flat_map(|&b| func.blocks[b].instructions.iter().copied()).collect();
    if loop_insts.len() > MAX_LOOP_INSTRUCTIONS {
        return None;
    }
    let in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(inst),
        _ => false,
    };
    let (counter, limit) = counted_limit(func, l, &loop_insts)?;
    let compared = compared_before(func, cfg, preheader, limit);
    let mut lengths = Vec::new();
    for &block in &blocks {
        // The header checks before its trip test.
        if block == l.header {
            continue;
        }
        for &inst in &func.blocks[block].instructions {
            let Some(check) = array_check(&func.inst(inst).kind) else { continue };
            let Some(&InstKind::Lt(index, length)) = inst_kind(func, check) else { continue };
            if length == limit
                || func.value_ty(length) != Some(MirType::I256)
                || in_loop(length)
                || compared.contains(&length)
                || lengths.contains(&length)
                // Two constants compare without a copy.
                || func.value_u256(length).is_some() && func.value_u256(limit).is_some()
            {
                continue;
            }
            if below_limit(func, &blocks, counter, limit, index, 0) {
                lengths.push(length);
            }
        }
    }
    if lengths.is_empty() {
        return None;
    }
    version_plan(func, cfg, l, preheader, blocks, Test::Bounds { limit, lengths }, None)
}

/// The lengths that a bounds check on every path to `preheader` compares `limit` with, `limit <
/// length`, which check elimination relates to the loop's indices without a copy.
fn compared_before(
    func: &Function,
    cfg: &CfgInfo,
    preheader: BlockId,
    limit: ValueId,
) -> FxHashSet<ValueId> {
    let mut compared = FxHashSet::default();
    for block in cfg.dominators().self_and_dominators(preheader) {
        for &inst in &func.blocks[block].instructions {
            if let Some(check) = array_check(&func.inst(inst).kind)
                && let Some(&InstKind::Lt(index, length)) = inst_kind(func, check)
                && index == limit
            {
                compared.insert(length);
            }
        }
    }
    compared
}

/// Whether `index` stays below the trip test's limit where the loop checks it: the counter, a
/// right shift of such an index, the counter plus a constant that a branch of the loop tests
/// against the limit, or a phi of such indices. It only decides whether a copy pays; check
/// elimination proves every fold.
fn below_limit(
    func: &Function,
    blocks: &[BlockId],
    counter: ValueId,
    limit: ValueId,
    index: ValueId,
    depth: usize,
) -> bool {
    const MAX_DEPTH: usize = 4;
    if index == counter {
        return true;
    }
    if depth == MAX_DEPTH {
        return false;
    }
    match inst_kind(func, index) {
        Some(&InstKind::Shr(shift, value)) => {
            func.value_u64(shift).is_some()
                && below_limit(func, blocks, counter, limit, value, depth + 1)
        }
        Some(&InstKind::Add(a, b)) => {
            let constant = |x: ValueId, k: ValueId| x == counter && func.value_u64(k).is_some();
            let sum = |value: ValueId| {
                value == index
                    || matches!(inst_kind(func, value), Some(&InstKind::Add(x, y))
                        if (x, y) == (a, b) || (x, y) == (b, a))
            };
            (constant(a, b) || constant(b, a)) && blocks.iter().any(|&block| {
                matches!(func.blocks[block].terminator, Some(Terminator::Branch { condition, .. })
                        if matches!(inst_kind(func, condition), Some(&InstKind::Lt(x, n))
                            if n == limit && sum(x)))
            })
        }
        Some(InstKind::Phi(incoming)) => incoming
            .iter()
            .all(|&(_, input)| below_limit(func, blocks, counter, limit, input, depth + 1)),
        _ => false,
    }
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

/// The addresses of the memory words that every path to `preheader` loads or stores, which
/// memory already covers there.
fn accessed_words(func: &Function, cfg: &CfgInfo, preheader: BlockId) -> FxHashSet<ValueId> {
    let mut accessed = FxHashSet::default();
    for block in cfg.dominators().self_and_dominators(preheader) {
        for &inst in &func.blocks[block].instructions {
            if let InstKind::Load(address, PtrLocation::Memory)
            | InstKind::Store(address, _, PtrLocation::Memory) = func.inst(inst).kind
            {
                accessed.insert(address);
            }
        }
    }
    accessed
}

/// Whether the loop runs `load` on every entry before anything that may leave it or read the
/// memory size: every edge out of the loop leaves from a block behind the load's block, and
/// nothing before the load may fail or end the call, in its block or in a loop block that may
/// run before it.
fn read_first(func: &Function, cfg: &CfgInfo, l: &Loop, blocks: &[BlockId], load: InstId) -> bool {
    let dominators = cfg.dominators();
    let Some(&block) =
        blocks.iter().find(|&&block| func.blocks[block].instructions.contains(&load))
    else {
        return false;
    };
    let exits_behind = blocks.iter().all(|&exiting| {
        let leaves = func.blocks[exiting].terminator.as_ref().is_some_and(|terminator| {
            terminator.successors().into_iter().any(|successor| !l.blocks.contains(successor))
        });
        !leaves || dominators.dominates(block, exiting)
    });
    exits_behind
        && blocks.iter().all(|&other| {
            let instructions = &func.blocks[other].instructions;
            let before = if other == block {
                let position = instructions.iter().position(|&inst| inst == load);
                &instructions[..position.expect("the load is in its block")]
            } else if dominators.dominates(block, other) {
                return true;
            } else {
                &instructions[..]
            };
            before.iter().all(|&inst| {
                let kind = &func.inst(inst).kind;
                !kind.effects().control.any() && !matches!(kind, InstKind::MSize)
            })
        })
}

/// Completes the plan of a loop that otherwise qualifies: loop values read after the loop must
/// reach each reading block through the one exit block that lies before it, where an exit block's
/// phis take them on the loop's edges, and the loop's edges into exit blocks that code outside the
/// loop also enters first get blocks of their own.
fn version_plan(
    func: &Function,
    cfg: &CfgInfo,
    l: &Loop,
    preheader: BlockId,
    blocks: Vec<BlockId>,
    test: Test,
    latch: Option<BlockId>,
) -> Option<Planned> {
    let exits: Vec<BlockId> = l.exit_blocks.to_vec();
    let loop_insts: FxHashSet<InstId> =
        blocks.iter().flat_map(|&b| func.blocks[b].instructions.iter().copied()).collect();
    let in_loop = |value: ValueId| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(inst),
        _ => false,
    };
    for (block, body) in func.blocks.iter_enumerated() {
        if l.blocks.contains(block) {
            continue;
        }
        let mut readers = Vec::new();
        for &inst in &body.instructions {
            if let InstKind::Phi(incoming) = &func.inst(inst).kind {
                for &(from, value) in incoming {
                    if in_loop(value) && !(exits.contains(&block) && l.blocks.contains(from)) {
                        readers.push(from);
                    }
                }
            } else if func.inst(inst).kind.operands().into_iter().any(in_loop) {
                readers.push(block);
            }
        }
        if body.terminator.as_ref().is_some_and(|t| t.operands().into_iter().any(in_loop)) {
            readers.push(block);
        }
        if readers.into_iter().any(|reader| owning_exit(cfg, &exits, reader).is_none()) {
            return None;
        }
    }
    let mut shared = Vec::new();
    for &exit in &exits {
        let preds = predecessors(func, exit);
        if preds.iter().any(|pred| !l.blocks.contains(*pred)) {
            shared.extend(
                preds.into_iter().filter(|&pred| l.blocks.contains(pred)).map(|pred| (pred, exit)),
            );
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
        test,
        latch,
    }))
}

/// Versions the loop and returns the blocks of its clone.
fn apply(func: &mut Function, plan: &Plan) -> Vec<BlockId> {
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

    // preheader | latch: ... jump test
    let test = func.alloc_block();
    let entry = plan.latch.unwrap_or(plan.preheader);
    let (terminator, metadata) = func.blocks[entry].take_terminator();
    let mut terminator = terminator.expect("the block entering the test is terminated");
    retarget(&mut terminator, |successor| if successor == plan.header { test } else { successor });
    func.blocks[entry].set_terminator(terminator, metadata);
    let mut builder = FunctionBuilder::new_semantic(func);
    builder.switch_to_block(test);
    let fast = builder.create_block();
    let slow = builder.create_block();
    let replacements = match &plan.test {
        Test::Words { written, words, scratch_end, bounds } => {
            let guarded = if bounds.is_some() { builder.create_block() } else { fast };
            let replacements =
                words_test(&mut builder, written, words, *scratch_end, &copy, guarded, slow);
            if let Some((limit, length)) = bounds {
                // guarded: short = lt len, n; jumpi short, slow, fast
                builder.switch_to_block(guarded);
                let length = replacements[&copy.value(*length)];
                bounds_test(&mut builder, *limit, &[length], fast, slow);
            }
            replacements
        }
        Test::Fence(fence) => fence_test(&mut builder, fence, &copy, fast, slow),
        Test::Bounds { limit, lengths } => {
            bounds_test(&mut builder, *limit, lengths, fast, slow);
            FxHashMap::default()
        }
    };
    builder.switch_to_block(fast);
    builder.jump(copy.blocks[&plan.header]);
    builder.switch_to_block(slow);
    builder.jump(plan.header);

    let header_copy = copy.blocks[&plan.header];
    match plan.latch {
        // header: v = phi [preheader: init], ... -> [slow: init] / header': [fast: init]
        None => {
            for (header, entry) in [(plan.header, slow), (header_copy, fast)] {
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
        }
        // header: v = phi [preheader: init], [latch: x] -> [preheader: init], [slow: x]
        // header': v' = phi [preheader: init], [latch': x'] -> [fast: x], [latch': x']
        Some(latch) => {
            let phis = func.blocks[plan.header]
                .instructions
                .iter()
                .copied()
                .zip(func.blocks[header_copy].instructions.iter().copied())
                .collect::<Vec<_>>();
            for (phi, phi_copy) in phis {
                let InstKind::Phi(incoming) = &mut func.inst_mut(phi).kind else { continue };
                let mut carried = None;
                for (from, value) in incoming.iter_mut() {
                    if *from == latch {
                        *from = slow;
                        carried = Some(*value);
                    }
                }
                let carried = carried.expect("the latch enters the header");
                let InstKind::Phi(incoming) = &mut func.inst_mut(phi_copy).kind else {
                    unreachable!("the copy of a phi is a phi")
                };
                for (from, value) in incoming.iter_mut() {
                    if *from == plan.preheader {
                        *from = fast;
                        *value = carried;
                    }
                }
            }
        }
    }

    // The check positions name the original headers, before any cloned length loads are removed.
    if let Test::Fence(fence) = &plan.test {
        drop_fenced_checks(func, fence, &copy);
    }
    match &plan.test {
        // The clone reads the test's words.
        Test::Words { .. } | Test::Fence(_) => {
            func.replace_uses(&replacements);
            for &block in copy.blocks.values() {
                let replaced = |inst: &InstId| {
                    func.inst_result_value(*inst)
                        .is_some_and(|result| replacements.contains_key(&result))
                };
                let instructions = func.blocks[block]
                    .instructions
                    .iter()
                    .copied()
                    .filter(|inst| !replaced(inst))
                    .collect();
                func.blocks[block].instructions = instructions;
            }
        }
        // The clone runs the loop as it is; check elimination folds its checks.
        Test::Bounds { .. } => {}
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
    copy.blocks.into_values().collect()
}

/// Builds the test that no array the loop writes reaches the words it reloads, branching to
/// `fast` when it holds and to `slow` otherwise, and returns the clone's loads of those words
/// with the test's values that replace them.
fn words_test(
    builder: &mut FunctionBuilder<'_>,
    written: &[Written],
    words: &[(ValueId, Vec<InstId>)],
    scratch_end: u64,
    copy: &LoopCopy,
    fast: BlockId,
    slow: BlockId,
) -> FxHashMap<ValueId, ValueId> {
    // test: lz = mload z, ...; ok = ...; jumpi ok, fast, slow
    let mut values = FxHashMap::default();
    for (word, loads) in words {
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
    for written in written {
        let bound = match written.bound {
            Bound::Length => values[&written.object],
            Bound::Value(bound) => bound,
        };
        let object = pointer_word(builder, written.object);
        // bits = bits | object | bound
        bits = builder.or(bits, object);
        bits = builder.or(bits, bound);
        // first = object + 1; span = (bound << shift) + 31
        let shift = builder.imm(u64::from(written.shift));
        let size = builder.shl(bound, shift);
        let first = builder.add(object, one);
        let span = builder.add(size, last_header_byte);
        for &(word, _) in words {
            if word == written.object {
                continue;
            }
            // inside = inside || word - first < span
            let word = pointer_word(builder, word);
            let offset = builder.sub(word, first);
            let within = builder.lt(offset, span);
            inside = builder.or(inside, within);
        }
    }
    if scratch_end != 0 {
        let end = builder.imm(scratch_end);
        for &(word, _) in words {
            // inside = inside || word < scratch_end
            let word = pointer_word(builder, word);
            let below = builder.lt(word, end);
            inside = builder.or(inside, below);
        }
    }
    // ok = bits < 2^64 && !inside
    let small = builder.lt(bits, limit);
    let clear = builder.eq(inside, no);
    let ok = builder.and(small, clear);
    builder.branch(ok, fast, slow);
    let mut replacements = FxHashMap::default();
    for (word, loads) in words {
        for &load in loads {
            if let Some(result) = builder.func().inst_result_value(load) {
                replacements.insert(copy.value(result), values[word]);
            }
        }
    }
    replacements
}

/// Builds the test that a two-pointer loop's scans start in order, below `2^250` and below the
/// bounds of both scans' checks, and that each rejects the element the other starts from,
/// branching to `fast` when it holds and to `slow` otherwise.
fn fence_test(
    builder: &mut FunctionBuilder<'_>,
    fence: &Fence,
    copy: &LoopCopy,
    fast: BlockId,
    slow: BlockId,
) -> FxHashMap<ValueId, ValueId> {
    let mut bounds = FxHashMap::default();
    if !fence.lengths.is_empty() {
        // length = mload array
        let length = builder.load_as(fence.up.array, PtrLocation::Memory, MirType::I256);
        for &inst in &fence.lengths {
            if let Some(result) = builder.func().inst_result_value(inst) {
                bounds.insert(result, length);
            }
        }
    }
    // test: ok = and (and (lt low, high), (lt high, 2^250)), (lt high, bound); jumpi ok, read, slow
    let ordered = builder.lt(fence.low, fence.high);
    let limit = builder.imm(U256::from(1) << FENCE_INDEX_BITS);
    let small = builder.lt(fence.high, limit);
    let mut ok = builder.and(ordered, small);
    for bound in [fence.up.bound, fence.down.bound].into_iter().flatten() {
        let bound = bounds.get(&bound).copied().unwrap_or(bound);
        // ok = and ok, (lt high, bound)
        let below = builder.lt(fence.high, bound);
        ok = builder.and(ok, below);
    }
    if !fence.lengths.is_empty() {
        // ok = and ok, (lt (ptrtoint array), 2^64)
        let array = pointer_word(builder, fence.up.array);
        let limit = builder.imm(U256::from(1) << 64);
        let low_memory = builder.lt(array, limit);
        ok = builder.and(ok, low_memory);
    }
    let read = builder.create_block();
    builder.branch(ok, read, slow);
    // read: up = <exit test of up at a[high]>; down = <exit test of down at a[low]>
    //       jumpi (or (continues up), (continues down)), slow, fast
    builder.switch_to_block(read);
    let up = fence_continues(builder, &fence.up, fence.high);
    let down = fence_continues(builder, &fence.down, fence.low);
    let continues = builder.or(up, down);
    builder.branch(continues, slow, fast);
    bounds.into_iter().map(|(value, bound)| (copy.value(value), bound)).collect()
}

/// Appends to the builder's block whether `scan` would read past the element at `index`:
/// `x = mload ((array + (index << 5)) + 32)`, then the scan's exit condition over `x`, true
/// when it would continue there.
fn fence_continues(builder: &mut FunctionBuilder<'_>, scan: &Scan, index: ValueId) -> ValueId {
    let shift = builder.imm(5);
    let offset = builder.shl(index, shift);
    let row = builder.ptradd(scan.array, offset);
    let header = builder.imm(32);
    let address = builder.ptradd(row, header);
    let func = builder.func();
    let ty = func.inst_result_value(scan.load).and_then(|result| func.value_ty(result));
    let element = builder.load_as(address, PtrLocation::Memory, ty.unwrap_or(MirType::I256));
    let loaded = builder.func().inst_result_value(scan.load).expect("loads have results");
    let mut values = FxHashMap::default();
    values.insert(loaded, element);
    for &inst in &scan.condition {
        let original = builder.func().inst(inst);
        let mut kind = original.kind.clone();
        let result_ty = original.result_ty;
        kind.visit_operands_mut(|operand| {
            if let Some(&value) = values.get(operand) {
                *operand = value;
            }
        });
        let (_, cloned) =
            builder.append_instruction(Instruction::new(kind, result_ty).with_debug_info_dropped());
        let result = builder.func().inst_result_value(inst);
        if let (Some(result), Some(cloned)) = (result, cloned) {
            values.insert(result, cloned);
        }
    }
    let test = values.get(&scan.test).copied().unwrap_or(scan.test);
    if scan.continues_on_true {
        test
    } else {
        let no = builder.imm_bool(false);
        builder.eq(test, no)
    }
}

/// Removes from the fast copy the checks the fences make redundant: both scans' checks of their
/// indices against their bounds, and both scans' overflow checks of their updates. The copy
/// compares the addresses of the elements the scans stopped at instead of their indices: both lie
/// between fences below `2^250`, where an element's address grows with its index, so the
/// comparisons agree, and the scans' counters then step only to address elements.
fn drop_fenced_checks(func: &mut Function, fence: &Fence, copy: &LoopCopy) {
    let headers = [&fence.up, &fence.down].map(|scan| copy.blocks[&scan.header]);
    for (scan, block) in [&fence.up, &fence.down].into_iter().zip(headers) {
        // header': ...; panic_if_zero (lt index, bound); ...  =>  header': ...; ...
        let mut position = 0;
        func.blocks[block].instructions.retain(|_| {
            let keep = !scan.checks.contains(&position);
            position += 1;
            keep
        });
        // header': next = checked_add counter, 1  =>  next = add counter, 1
        let Some(update) = func.inst_result_value(scan.update) else { continue };
        let Value::Inst(cloned) = *func.value(copy.value(update)) else { continue };
        if let InstKind::CheckedBinary { op, lhs, rhs, .. } = func.inst(cloned).kind {
            func.inst_mut(cloned).kind = match op {
                CheckedOp::Add => InstKind::Add(lhs, rhs),
                _ => InstKind::Sub(lhs, rhs),
            };
        }
    }
    // The scans' headers define the addresses beside `i'` and `j'`, so they dominate a
    // comparison of the two outside them.
    let addresses = [&fence.up, &fence.down].map(|scan| match func.inst(scan.load).kind {
        InstKind::Load(address, _) => Some(copy.value(address)),
        _ => None,
    });
    let Some(order) = func.inst_result_value(fence.order) else { return };
    let Value::Inst(order) = *func.value(copy.value(order)) else { return };
    let Some(block) = copy
        .blocks
        .values()
        .copied()
        .find(|&block| func.blocks[block].instructions.contains(&order))
    else {
        return;
    };
    let [Some(up), Some(down)] = addresses else { return };
    if headers.contains(&block) {
        return;
    }
    // order': lt i', j'  =>  i'' = ptrtoint &a[i']; j'' = ptrtoint &a[j']; lt i'', j''
    let words = [up, down].map(|address| match func.value_ty(address) {
        Some(MirType::I256) | None => (address, None),
        Some(_) => {
            let (cast, word) = func.alloc_value_inst(
                Instruction::new(InstKind::PtrToInt(address, 256), Some(MirType::I256))
                    .with_debug_info_dropped(),
            );
            (word, Some(cast))
        }
    });
    let position = func.blocks[block]
        .instructions
        .iter()
        .position(|&inst| inst == order)
        .expect("the order's block holds it");
    let casts = words.iter().filter_map(|&(_, cast)| cast).collect::<Vec<_>>();
    func.blocks[block].instructions.splice(position..position, casts);
    func.inst_mut(order).kind = InstKind::Lt(words[0].0, words[1].0);
}

/// The word of a pointer value, for comparisons.
fn pointer_word(builder: &mut FunctionBuilder<'_>, value: ValueId) -> ValueId {
    match builder.func().value_ty(value) {
        Some(MirType::I256) | None => value,
        Some(_) => builder.cast(value, MirType::I256),
    }
}

/// Builds the test that a counted loop's limit does not exceed any of the lengths, branching to
/// `fast` when it holds and to `slow` otherwise.
fn bounds_test(
    builder: &mut FunctionBuilder<'_>,
    limit: ValueId,
    lengths: &[ValueId],
    fast: BlockId,
    slow: BlockId,
) {
    // test: short = or (lt len1, n), (lt len2, n), ...; jumpi short, slow, fast
    let mut short = None;
    for &length in lengths {
        let below = builder.lt(length, limit);
        short = Some(match short {
            Some(short) => builder.or(short, below),
            None => below,
        });
    }
    let short = short.expect("a planned bounds test compares a length");
    builder.branch(short, slow, fast);
}

/// The counter and invariant limit of a loop whose header tests `counter < limit`.
fn counted_limit(
    func: &Function,
    l: &Loop,
    loop_insts: &FxHashSet<InstId>,
) -> Option<(ValueId, ValueId)> {
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[l.header].terminator
    else {
        return None;
    };
    let &InstKind::Lt(counter, limit) = inst_kind(func, condition)? else { return None };
    if !l.blocks.contains(then_block)
        || l.blocks.contains(else_block)
        || matches!(func.value(limit), Value::Inst(inst) if loop_insts.contains(inst))
        || func.value_ty(limit) != Some(MirType::I256)
    {
        return None;
    }
    let Value::Inst(phi) = *func.value(counter) else { return None };
    if !func.blocks[l.header].instructions.contains(&phi)
        || !matches!(func.inst(phi).kind, InstKind::Phi(_))
    {
        return None;
    }
    Some((counter, limit))
}

/// A counted loop over one written array can reuse its length after the words test proves that
/// its stores cannot wrap into its header. The words passed here are already safe to read before
/// the loop, including when its first trip test fails.
fn reloaded_bounds(
    func: &Function,
    l: &Loop,
    blocks: &[BlockId],
    written: &[Written],
    words: &[(ValueId, Vec<InstId>)],
) -> Option<(ValueId, ValueId)> {
    let [written] = written else { return None };
    let (_, loads) = words.iter().find(|(word, _)| *word == written.object)?;
    let loop_insts =
        blocks.iter().flat_map(|&block| func.blocks[block].instructions.iter().copied()).collect();
    let (counter, limit) = counted_limit(func, l, &loop_insts)?;
    for &block in blocks {
        if block == l.header {
            continue;
        }
        for &inst in &func.blocks[block].instructions {
            let Some(check) = array_check(&func.inst(inst).kind) else { continue };
            let Some(&InstKind::Lt(index, length)) = inst_kind(func, check) else { continue };
            if let Value::Inst(load) = func.value(length)
                && loads.contains(load)
                && func.value_ty(length) == Some(MirType::I256)
                && below_limit(func, blocks, counter, limit, index, 0)
            {
                return Some((limit, length));
            }
        }
    }
    None
}
