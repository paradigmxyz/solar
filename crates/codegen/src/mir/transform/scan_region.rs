//! Replaces a branchy search over the highest set bit of a word with the `CLZ` opcode.
//!
//! [`bit_scan`](super::bit_scan) proves the scans of one block. Some libraries spell a scan with
//! branches that if-conversion keeps, because each arm updates two values. The initial estimate
//! of PRBMath's and OpenZeppelin's `sqrt` halves the word and the estimate together:
//!
//! ```text
//! if (x >= 2**128) { x >>= 128; r <<= 64; }
//! if (x >= 2**64) { x >>= 64; r <<= 32; }
//! ...
//! if (x >= 4) { r <<= 1; }
//! ```
//!
//! which leaves `r = 2**(m / 2)` for the index `m` of the highest set bit: seven branches and
//! six joins with two phis each, where `1 << ((255 - clz(x | 1)) >> 1)` takes six operations.
//!
//! # Recognition
//!
//! A test of the upper half of a word `x` (a comparison with `2**128` or `2**128 - 1`, or a
//! right shift by 128 bits) starts a region at its block. The pass runs the region from there on
//! one input per class of inputs, zero and `2**m` for every `m`, and follows its branches until
//! it reaches an instruction it cannot run: one with an effect, or a branch whose condition it
//! does not know.
//!
//! A run tracks three kinds of value. `x` shifted right by a known amount is one: a right
//! shift by a known amount shifts it further, and a select or a phi passes it on. A comparison
//! of such a value with a constant `c` is known when it tests a threshold of a power of two:
//! `< c` with `c` zero or a power of two, `> c` with `c + 1` one, and `==` and `!=` with zero.
//! Every such test of `x >> s` asks whether `m >= s + k` for some `k`, so it has the same result
//! for every input with the same highest set bit as long as `s` does. Pure operations on known
//! values are known; any other operation that reads `x` makes an unknown value, and so does
//! anything that reads one. By induction every known value and every branch a run takes depends
//! only on the highest set bit of `x`, so the run on `2**m` gives what every input of its class
//! gets, and the runs on the 257 classes prove a value for every input.
//!
//! # Rewrite
//!
//! All runs must stop in one block, the region's last, and the blocks they pass must form a
//! region its first block enters: the predecessors of every other block lie in it, and no run
//! loops. A value of the last block that every run computed before stopping is a function of
//! `m`, given by a table of 257 entries, and it is replaced when the table is one of
//!
//! ```text
//! index >> k        (255 - clz(x | 1)) >> k, or with clz(x)
//! leading zeros     clz(x | 1), or clz(x)
//! 2**(index / 2)    1 << ((255 - clz(x | 1)) >> 1), or with clz(x)
//! ```
//!
//! with the form its value for zero picks. Nothing else the region computes may be read past it:
//! the first block then jumps straight to the last one, and the other blocks, the phis of the last
//! block and the instructions only the region read go. The rewrite is taken when the target
//! prices the new operations below the removed instructions and branches.
//!
//! # Limits and placement
//!
//! The target must have `CLZ` (Osaka). The region has no loop and no instruction with an effect
//! before its last block, and the runs of one function evaluate at most [`WORK`] instructions.
//! The pass runs after `bit-scan`, which proves the straight-line scans first.

use super::word_sequence::is_pure;
use crate::{
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, Terminator,
        Value, ValueId,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
        utils::{eval::eval_typed_inst, invalidate_unreachable_block, replace_terminator},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

/// The instructions the runs of one function may evaluate.
const WORK: u32 = 1 << 18;
/// The regions one function may try.
const MAX_REGIONS: usize = 64;
/// The classes tried first, which reject most regions.
const PROBES: [u32; 5] = [0, 128, 255, 64, 1];

pub(crate) struct ScanRegion;

impl MirPass for ScanRegion {
    fn name(&self) -> &'static str {
        "scan-region"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if !gcx.sess.opts.evm_version.has_clz() {
            return false;
        }
        run_function_pass(module, analyses, |func, analyses| {
            run_function(func, &analyses.target(gcx))
        })
    }
}

/// A value of one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    /// The same number for every input of the class.
    Known(U256),
    /// The input shifted right by this many bits, fewer than 256.
    Shifted(u32),
    /// Anything else.
    Unknown,
}

/// Where a run stopped, and the known values it computed.
struct Run {
    stop_block: BlockId,
    /// The index of the instruction the run could not run, or the block's length for its
    /// terminator.
    stop_at: usize,
    known: FxHashMap<ValueId, U256>,
    blocks: Vec<BlockId>,
}

/// What a proved value computes from the index `m` of the highest set bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// `m >> k`.
    Index(u32),
    /// `255 - m`.
    LeadingZeros,
    /// `2**(m >> 1)`.
    HalfPower,
}

fn run_function(func: &mut Function, target: &Target) -> bool {
    let mut changed = false;
    let mut work = WORK;
    let mut tried = FxHashSet::default();
    for _ in 0..MAX_REGIONS {
        let Some((entry, input)) = next_region(func, &mut tried) else { break };
        changed |= rewrite_region(func, target, &mut work, entry, input);
        if work == 0 {
            break;
        }
    }
    changed
}

/// The next untried block that tests the upper half of a word, and the word.
fn next_region(
    func: &Function,
    tried: &mut FxHashSet<(BlockId, ValueId)>,
) -> Option<(BlockId, ValueId)> {
    for (block, data) in func.blocks.iter_enumerated() {
        if matches!(data.terminator, None | Some(Terminator::Invalid)) {
            continue;
        }
        for &inst in &data.instructions {
            if let Some(input) = half_test(func, inst)
                && tried.insert((block, input))
            {
                return Some((block, input));
            }
        }
    }
    None
}

/// The word whose upper half `inst` tests.
fn half_test(func: &Function, inst: InstId) -> Option<ValueId> {
    let half = U256::ONE << 128;
    let is_half = |value| {
        func.value_u256(value).is_some_and(|limit| limit == half || limit == half - U256::ONE)
    };
    let input = match func.inst(inst).kind {
        InstKind::Lt(a, b) | InstKind::Gt(a, b) if is_half(b) => a,
        InstKind::Lt(a, b) | InstKind::Gt(a, b) if is_half(a) => b,
        InstKind::Shr(shift, word) if func.value_u256(shift) == Some(U256::from(128)) => word,
        _ => return None,
    };
    (func.value_u256(input).is_none() && func.value_ty(input) == Some(MirType::I256))
        .then_some(input)
}

/// Whether `x < c` asks only for a bound on the highest set bit of `x`.
fn is_threshold(c: U256) -> bool {
    c.is_zero() || c.is_power_of_two()
}

/// Whether a comparison of `x >> s` with the constant `c` is decided by the highest set bit
/// of `x`, where `shifted_first` tells which operand `x >> s` is.
fn decides(kind: &InstKind, shifted_first: bool, c: U256) -> bool {
    match (kind, shifted_first) {
        // x < c, c > x
        (InstKind::Lt(..), true) | (InstKind::Gt(..), false) => is_threshold(c),
        // x > c, c < x
        (InstKind::Gt(..), true) | (InstKind::Lt(..), false) => {
            is_threshold(c.wrapping_add(U256::ONE))
        }
        (InstKind::Eq(..) | InstKind::Ne(..), _) => c.is_zero(),
        _ => false,
    }
}

/// The value of `value` in a run.
fn word(func: &Function, env: &FxHashMap<ValueId, Word>, input: ValueId, value: ValueId) -> Word {
    if value == input {
        Word::Shifted(0)
    } else if let Some(c) = func.value_u256(value) {
        Word::Known(c)
    } else {
        env.get(&value).copied().unwrap_or(Word::Unknown)
    }
}

/// Runs the region of `input` from `entry` on `probe`, or returns `None` when it loops or
/// the work runs out.
fn run(
    func: &Function,
    entry: BlockId,
    start: usize,
    input: ValueId,
    probe: U256,
    work: &mut u32,
) -> Option<Run> {
    let mut env = FxHashMap::default();
    let mut blocks = vec![entry];
    let mut block = entry;
    let mut pred = None;
    loop {
        let data = &func.blocks[block];
        let mut index = if block == entry { start } else { 0 };
        // Phis read the edge the run came along, all at once.
        let mut phis = Vec::new();
        while let Some(&inst) = data.instructions.get(index) {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { break };
            let value = func.inst_result_value(inst)?;
            let word = pred
                .and_then(|pred| incoming.iter().find(|&&(from, _)| from == pred))
                .map_or(Word::Unknown, |&(_, value)| word(func, &env, input, value));
            phis.push((value, word));
            index += 1;
        }
        env.extend(phis);
        while let Some(&inst) = data.instructions.get(index) {
            *work = work.checked_sub(1)?;
            let instruction = func.inst(inst);
            if !is_pure(instruction) || matches!(instruction.kind, InstKind::Phi(_)) {
                return Some(Run { stop_block: block, stop_at: index, known: known(env), blocks });
            }
            if let Some(value) = instruction.result() {
                let result = eval(func, &instruction.kind, &env, input, probe);
                env.insert(value, result);
            }
            index += 1;
        }
        let next = match &data.terminator {
            Some(Terminator::Jump(next)) => Some(*next),
            Some(Terminator::Branch { condition, then_block, else_block }) => {
                match word(func, &env, input, *condition) {
                    Word::Known(c) => Some(if c.is_zero() { *else_block } else { *then_block }),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(next) = next else {
            return Some(Run { stop_block: block, stop_at: index, known: known(env), blocks });
        };
        // A loop, or a return to the entry, is not a region the rewrite takes.
        if blocks.contains(&next) {
            return None;
        }
        blocks.push(next);
        pred = Some(block);
        block = next;
    }
}

fn known(env: FxHashMap<ValueId, Word>) -> FxHashMap<ValueId, U256> {
    env.into_iter()
        .filter_map(|(value, word)| match word {
            Word::Known(c) => Some((value, c)),
            _ => None,
        })
        .collect()
}

/// Evaluates one pure instruction of a run on `probe`.
fn eval(
    func: &Function,
    kind: &InstKind,
    env: &FxHashMap<ValueId, Word>,
    input: ValueId,
    probe: U256,
) -> Word {
    let get = |value| word(func, env, input, value);
    match *kind {
        InstKind::Shr(shift, value) => {
            if let (Word::Known(shift), Word::Shifted(s)) = (get(shift), get(value)) {
                let total = U256::from(s).saturating_add(shift);
                return if total >= U256::from(256) {
                    Word::Known(U256::ZERO)
                } else {
                    Word::Shifted(total.to::<u32>())
                };
            }
        }
        InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::Eq(a, b) | InstKind::Ne(a, b) => {
            let (shifted, c, shifted_first) = match (get(a), get(b)) {
                (Word::Shifted(s), Word::Known(c)) => (s, c, true),
                (Word::Known(c), Word::Shifted(s)) => (s, c, false),
                (Word::Shifted(_), _) | (_, Word::Shifted(_)) => return Word::Unknown,
                _ => (0, U256::ZERO, false),
            };
            if matches!(get(a), Word::Shifted(_)) || matches!(get(b), Word::Shifted(_)) {
                if !decides(kind, shifted_first, c) {
                    return Word::Unknown;
                }
                let x = probe >> shifted as usize;
                let (a, b) = if shifted_first { (x, c) } else { (c, x) };
                let result = match kind {
                    InstKind::Lt(..) => a < b,
                    InstKind::Gt(..) => a > b,
                    InstKind::Eq(..) => a == b,
                    _ => a != b,
                };
                return Word::Known(U256::from(result));
            }
        }
        InstKind::Select(condition, a, b) => {
            return match get(condition) {
                Word::Known(c) => get(if c.is_zero() { b } else { a }),
                _ => Word::Unknown,
            };
        }
        _ => {}
    }
    let mut operands_known = true;
    kind.visit_operands(|operand| operands_known &= matches!(get(operand), Word::Known(_)));
    if !operands_known {
        return Word::Unknown;
    }
    let result = eval_typed_inst(func, kind, |operand| match get(operand) {
        Word::Known(c) => Ok(c),
        _ => Err(()),
    });
    match result {
        Ok(Some(c)) => Word::Known(c),
        _ => Word::Unknown,
    }
}

/// The shape of a table of results by highest set bit, and whether its value for zero needs
/// `x | 1` rather than `x`.
fn shape(table: &[U256], zero: U256) -> Option<(Shape, bool)> {
    let fits = |f: &dyn Fn(u32) -> U256| (0..256u32).all(|m| table[m as usize] == f(m));
    for k in 0..8u32 {
        if fits(&|m| U256::from(m >> k)) {
            return if zero.is_zero() {
                Some((Shape::Index(k), true))
            } else if zero == U256::MAX >> k as usize {
                Some((Shape::Index(k), false))
            } else {
                None
            };
        }
    }
    if fits(&|m| U256::from(255 - m)) {
        return if zero == U256::from(255) {
            Some((Shape::LeadingZeros, true))
        } else if zero == U256::from(256) {
            Some((Shape::LeadingZeros, false))
        } else {
            None
        };
    }
    if fits(&|m| U256::ONE << (m >> 1) as usize) {
        return if zero == U256::ONE {
            Some((Shape::HalfPower, true))
        } else if zero.is_zero() {
            Some((Shape::HalfPower, false))
        } else {
            None
        };
    }
    None
}

/// Whether a result for the class `m` fits one of the shapes.
fn may_fit(m: u32, result: U256) -> bool {
    (0..8u32).any(|k| result == U256::from(m >> k))
        || result == U256::from(255 - m)
        || result == U256::ONE << (m >> 1) as usize
}

fn immediate(func: &mut Function, value: u64) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::I256(U256::from(value))))
}

/// Emits the `CLZ` form of `shape` and returns its last operation.
fn clz_form(
    func: &mut Function,
    shape: Shape,
    odd: bool,
    input: ValueId,
    emit: &mut dyn FnMut(&mut Function, InstKind) -> ValueId,
) -> InstKind {
    let one = immediate(func, 1);
    let all = immediate(func, 255);
    // %word = or %input, 1
    let word = if odd { emit(func, InstKind::Or(input, one)) } else { input };
    if shape == Shape::LeadingZeros {
        // %zeros = clz %word
        return InstKind::Clz(word);
    }
    // %zeros = clz %word
    // %index = sub 255, %zeros
    let zeros = emit(func, InstKind::Clz(word));
    let index = InstKind::Sub(all, zeros);
    match shape {
        Shape::Index(0) => index,
        Shape::Index(k) => {
            // %result = shr %index, k
            let index = emit(func, index);
            let k = immediate(func, u64::from(k));
            InstKind::Shr(k, index)
        }
        _ => {
            // %half = shr %index, 1
            // %result = shl 1, %half
            let index = emit(func, index);
            let half = emit(func, InstKind::Shr(one, index));
            InstKind::Shl(half, one)
        }
    }
}

fn phi_count(func: &Function, block: BlockId) -> usize {
    func.blocks[block]
        .instructions
        .iter()
        .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        .count()
}

/// The values of `last` that every run computed, before the first instruction one stopped at.
fn computed(
    func: &Function,
    last: BlockId,
    first: usize,
    runs: &[(u32, Run)],
) -> Vec<(InstId, ValueId)> {
    let limit = runs.iter().map(|(_, run)| run.stop_at).min().unwrap_or(0);
    let instructions = &func.blocks[last].instructions;
    instructions[first.min(limit)..limit.min(instructions.len())]
        .iter()
        .filter_map(|&inst| {
            let value = func.inst_result_value(inst)?;
            (func.value_ty(value) == Some(MirType::I256)
                && runs.iter().all(|(_, run)| run.known.contains_key(&value)))
            .then_some((inst, value))
        })
        .collect()
}

/// Proves and rewrites the region of `input` that starts at `entry`.
fn rewrite_region(
    func: &mut Function,
    target: &Target,
    work: &mut u32,
    entry: BlockId,
    input: ValueId,
) -> bool {
    // The runs start after the input when the entry defines it, and after the phis otherwise.
    let start = match func.value(input) {
        Value::Inst(def) => func.blocks[entry]
            .instructions
            .iter()
            .position(|inst| inst == def)
            .map_or_else(|| phi_count(func, entry), |place| place + 1),
        _ => phi_count(func, entry),
    };
    let first = |last: BlockId| if last == entry { start } else { 0 };

    // The zero input and a few classes first; they reject most regions.
    let mut runs: Vec<(u32, Run)> = Vec::new();
    let Some(zero) = run(func, entry, start, input, U256::ZERO, work) else { return false };
    let last = zero.stop_block;
    for m in PROBES {
        let Some(run) = run(func, entry, start, input, U256::ONE << m as usize, work) else {
            return false;
        };
        if run.stop_block != last {
            return false;
        }
        runs.push((m, run));
    }
    let plausible = computed(func, last, first(last), &runs).into_iter().any(|(_, value)| {
        zero.known.contains_key(&value)
            && runs.iter().all(|(m, run)| may_fit(*m, run.known[&value]))
    });
    if !plausible {
        return false;
    }

    // Every class now.
    for m in 0..256u32 {
        if PROBES.contains(&m) {
            continue;
        }
        let Some(run) = run(func, entry, start, input, U256::ONE << m as usize, work) else {
            return false;
        };
        if run.stop_block != last {
            return false;
        }
        runs.push((m, run));
    }
    runs.sort_by_key(|&(m, _)| m);
    let mut proved = Vec::new();
    for (inst, value) in computed(func, last, first(last), &runs) {
        let Some(&at_zero) = zero.known.get(&value) else { continue };
        if zero.stop_at < func.blocks[last].instructions.len()
            && func.blocks[last].instructions[..zero.stop_at].iter().all(|&i| i != inst)
        {
            continue;
        }
        let table: Vec<U256> = runs.iter().map(|(_, run)| run.known[&value]).collect();
        if let Some((shape, odd)) = shape(&table, at_zero) {
            proved.push((inst, value, shape, odd));
        }
    }
    if proved.is_empty() {
        return false;
    }

    // The region: the blocks the runs passed, entered only through the entry.
    let mut region: FxHashSet<BlockId> = zero.blocks.iter().copied().collect();
    for (_, run) in &runs {
        region.extend(run.blocks.iter().copied());
    }
    for &block in &region {
        let preds = &func.blocks[block].predecessors;
        let closed = if block == entry {
            preds.iter().all(|pred| !region.contains(pred))
        } else {
            preds.iter().all(|pred| region.contains(pred))
        };
        if !closed {
            return false;
        }
    }
    let mut middle: Vec<BlockId> =
        region.iter().copied().filter(|&block| block != entry && block != last).collect();
    middle.sort_unstable();

    // What the rewrite removes: the middle blocks, the phis of the last block, and then every
    // pure instruction of the entry and the last block that nothing reads any more.
    let mut uses = super::egraph::use_counts(func);
    let replaced: FxHashSet<InstId> = proved.iter().map(|&(inst, ..)| inst).collect();
    let mut removed: FxHashSet<InstId> = FxHashSet::default();
    let mut branches = Cost::ZERO;
    let mut pending = Vec::new();
    if last != entry {
        for &block in middle.iter().chain([&entry]) {
            let terminator = func.blocks[block].terminator.as_ref().expect("a run passed it");
            terminator.visit_operands(|operand| uses[operand] -= 1);
            branches += match terminator {
                Terminator::Jump(_) => target.jump(),
                _ => target.branch() + target.branch(),
            };
        }
        for &block in &middle {
            pending.extend(func.blocks[block].instructions.iter().copied());
        }
        pending.extend(func.blocks[last].instructions.iter().copied().take(phi_count(func, last)));
    }
    for &inst in &replaced {
        func.inst(inst).kind.visit_operands(|operand| uses[operand] -= 1);
    }
    for &inst in &pending {
        if removed.insert(inst) && !replaced.contains(&inst) {
            func.inst(inst).kind.visit_operands(|operand| uses[operand] -= 1);
        }
    }
    let mut sweep: Vec<InstId> = func.blocks[entry]
        .instructions
        .iter()
        .chain(&func.blocks[last].instructions)
        .copied()
        .filter(|inst| !removed.contains(inst) && !replaced.contains(inst))
        .collect();
    loop {
        let mut progress = false;
        sweep.retain(|&inst| {
            let instruction = func.inst(inst);
            let dead = !matches!(instruction.kind, InstKind::Phi(_))
                && is_pure(instruction)
                && instruction.result().is_some_and(|value| uses[value] == 0);
            if dead {
                removed.insert(inst);
                instruction.kind.visit_operands(|operand| uses[operand] -= 1);
                progress = true;
            }
            !dead
        });
        if !progress {
            break;
        }
    }
    // Nothing that stays may read what the removed code computes. A replaced phi hands its
    // readers to its new value.
    for &inst in &removed {
        if let Some(value) = func.inst_result_value(inst)
            && uses[value] != 0
            && !replaced.contains(&inst)
        {
            return false;
        }
    }

    // Price the rewrite.
    let old = removed
        .iter()
        .chain(replaced.iter().filter(|inst| !removed.contains(inst)))
        .map(|&inst| target.instruction_estimate(func, inst))
        .sum::<Cost>()
        + branches;
    let mut new = if last != entry { target.jump() } else { Cost::ZERO };
    for &(_, _, shape, odd) in &proved {
        let last_kind = clz_form(func, shape, odd, input, &mut |func, kind| {
            new += target.kind_estimate(func, &kind);
            input
        });
        new += target.kind_estimate(func, &last_kind);
    }
    if !target.cmp(new, old).is_lt() {
        return false;
    }

    // Rewrite.
    let mut replacements = FxHashMap::default();
    let mut front = Vec::new();
    let mut before: FxHashMap<InstId, Vec<InstId>> = FxHashMap::default();
    for &(inst, value, shape, odd) in &proved {
        let metadata = func.inst(inst).metadata.debug_context();
        let mut inserted = Vec::new();
        let last_kind = clz_form(func, shape, odd, input, &mut |func, kind| {
            let mut new = Instruction::new(kind, Some(MirType::I256));
            new.metadata = metadata.clone();
            let (new, value) = func.alloc_value_inst(new);
            inserted.push(new);
            value
        });
        if matches!(func.inst(inst).kind, InstKind::Phi(_)) {
            let mut new = Instruction::new(last_kind, Some(MirType::I256));
            new.metadata = metadata.clone();
            let (new, new_value) = func.alloc_value_inst(new);
            inserted.push(new);
            front.extend(inserted);
            replacements.insert(value, new_value);
        } else {
            func.replace_inst_kind(inst, last_kind);
            before.insert(inst, inserted);
        }
    }
    let old_instructions = std::mem::take(&mut func.blocks[last].instructions);
    let mut instructions = front;
    for inst in old_instructions {
        let is_phi = matches!(func.inst(inst).kind, InstKind::Phi(_));
        if (removed.contains(&inst) && !replaced.contains(&inst)) || (is_phi && last != entry) {
            continue;
        }
        instructions.extend(before.remove(&inst).unwrap_or_default());
        instructions.push(inst);
    }
    func.blocks[last].instructions = instructions;
    if last != entry {
        func.blocks[entry].instructions.retain(|inst| !removed.contains(inst));
        for &block in &middle {
            let _ = invalidate_unreachable_block(func, block);
        }
        replace_terminator(func, entry, Terminator::Jump(last));
    }
    if !replacements.is_empty() {
        func.replace_uses(&replacements);
    }
    true
}
