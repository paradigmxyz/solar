//! Word operations for counted loops that move one byte of a `bytes` buffer per iteration
//! (`byte-loops`).
//!
//! Checked Solidity has no word access into `bytes memory`, so it spells one as a loop over the
//! word's bytes, and a copy as a loop over the copied bytes:
//!
//! ```solidity
//! for (uint256 k; k < 32; ++k) word |= bytes32(b[offset + k]) >> (8 * k); // gather
//! for (uint256 k; k < 32; ++k) b[offset + k] = word[k];                   // scatter
//! for (uint256 k; k < count; ++k) out[k] = b[offset + k];                 // copy into `out`
//! ```
//!
//! Each iteration checks its index, reads a whole word to extract one byte, or writes one byte,
//! so a 32-byte word costs about two thousand gas. This pass replaces such a loop with one range
//! check and the word operation it spells: `mload` and a mask for a gather of `N <= 32` bytes,
//! `mstore` (or a read-modify-write of the leading `N` bytes) for a scatter, and `mcopy` for a
//! copy into a buffer allocated in the same function.
//!
//! # Recognition
//!
//! A loop qualifies when it is a canonical counted loop: a preheader, one exit, a header holding
//! only phis and `k < bound`, a counter `k` that starts at zero and steps by one, and a body of
//! straight-line blocks. The body must consist of exactly the pattern's instructions plus pure
//! instructions whose results nothing outside the loop reads. Every instruction that may fail or
//! write is part of the pattern:
//!
//! - the index `offset + k` (`checked_add`, or `k` itself), and the bounds check
//!   `panic_if_zero<0x32> (lt index, length)` with `length = mload object`;
//! - a gather's `byte 0` of the word at `object + 32 + index`, shifted into place by `8 * k` and
//!   ORed into an accumulator phi that starts at zero, with optional masks that keep every gathered
//!   byte;
//! - a scatter's `mstore8` of `byte k, value` for a loop-invariant `value`, with an optional bounds
//!   check on `k` that the header test already implies;
//! - a copy's `mstore8` of a gathered byte into a second object, whose index check may be the
//!   header test itself when the destination's length is the bound.
//!
//! # Safety
//!
//! The original loop panics with `Panic(0x32)` exactly when some `k < bound` has
//! `offset + k >= length`, and before any other failure: an index `offset + k` can only overflow
//! after the earlier index `length - offset`, or `offset` itself, already failed its bounds check,
//! because a length is below `2^256`. The replacement therefore raises the same panic exactly
//! when `bound > 0` and `bound > length` or `offset > length - bound`, and so for every object a
//! copy touches. A panic reverts the call frame, so the bytes a failing loop wrote before it
//! panicked were never observable.
//!
//! The word read starts at the lowest byte the loop read and ends inside the last word the loop
//! read, so it touches no memory the loop did not. A scatter below 32 bytes reads and rewrites
//! the word after its last byte, and a copy's `mcopy` reads and writes exactly the copied
//! ranges; memory expansion can differ, which only `msize` sees. A
//! loop's own stores can neither reach the length word of the object they write, which lies
//! below the checked range, nor feed a gather, which stores nothing. A copy is only rewritten
//! when its destination is a fresh allocation and its source a function argument: the
//! allocation lies above every object that existed on entry, so the two ranges are disjoint
//! and the byte order of the original loop cannot matter.
//!
//! # Place in the pipeline
//!
//! The pass runs on semantic MIR right after `loop-canonicalize`, where checks are still single
//! instructions and object identities are explicit, and before the inliner, so a helper that
//! spells a word access shrinks to a few instructions before callers decide to inline it. Both
//! optimization objectives run it: the result is smaller and cheaper.
//!
//! Not handled yet: copies between two arguments, which need a runtime overlap check; copies
//! that run downwards; and loops that fill, compare or gather into an integer with left shifts.
//!
//! NOTE: The replacement summarizes every iteration of the loop, so its instructions carry no
//! source location rather than an arbitrary iteration's checkpoint.

use crate::mir::{
    ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, Function, FunctionBuilder, InstId,
    InstKind, Module, PanicCode, PtrLocation, RevertKind, Terminator, Value, ValueId,
    analysis::{Loop, LoopAnalyzer},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
    utils::{fold_terminator_to_jump, invalidate_unreachable_block},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Replaces counted byte loops with word loads, word stores and copies.
pub(crate) struct ByteLoops;

impl MirPass for ByteLoops {
    fn name(&self) -> &'static str {
        "byte-loops"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| run_function(func))
    }
}

fn run_function(func: &mut Function) -> bool {
    let mut changed = false;
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(plan) = loops.all_loops().find_map(|l| plan(func, l)) else { break };
        rewrite(func, plan);
        let _ = super::cfg_simplify::remove_unreachable_blocks(func);
        changed = true;
    }
    changed
}

/// A canonical counted loop from zero by one to `bound`.
struct Counted {
    header: BlockId,
    preheader: BlockId,
    exit: BlockId,
    body: Vec<BlockId>,
    /// The counter phi's value.
    counter: ValueId,
    /// The header test `counter < bound`.
    test: ValueId,
    bound: ValueId,
    /// The header's other phis, by result value.
    phis: Vec<(ValueId, InstId)>,
    /// The latch's counter increment.
    step: InstId,
    /// Every body instruction, in program order.
    insts: Vec<InstId>,
}

enum Shape {
    /// `bound` bytes of `object` from `offset`, left-aligned into the accumulator `result`.
    Gather { object: ValueId, offset: Option<ValueId>, width: u64, result: ValueId },
    /// The leading `bound` bytes of `value` written into `object` from `offset`.
    Scatter { object: ValueId, offset: Option<ValueId>, width: u64, value: ValueId },
    /// `bound` bytes of `src` from `src_offset` written into the fresh `dst` from `dst_offset`.
    Copy {
        src: ValueId,
        src_offset: Option<ValueId>,
        dst: ValueId,
        dst_offset: Option<ValueId>,
        /// Whether the destination index needs its own range check: it does not when the header
        /// test is that check.
        dst_checked: bool,
    },
}

struct Plan {
    lp: Counted,
    shape: Shape,
}

fn plan(func: &Function, l: &Loop) -> Option<Plan> {
    let lp = counted(func, l)?;
    let shape =
        gather(func, l, &lp).or_else(|| scatter(func, l, &lp)).or_else(|| copy(func, l, &lp))?;
    // Values computed in the body die with the loop, so nothing after it may read them.
    for &inst in &lp.insts {
        if let Some(value) = func.inst_result_value(inst)
            && escapes(func, l, value)
        {
            return None;
        }
    }
    if escapes(func, l, lp.test) {
        return None;
    }
    Some(Plan { lp, shape })
}

fn counted(func: &Function, l: &Loop) -> Option<Counted> {
    let header = l.header;
    let preheader = l.preheader?;
    let [latch] = l.back_edges.as_slice() else { return None };
    let [exit] = l.exit_blocks.as_slice() else { return None };
    if !matches!(func.blocks[preheader].terminator, Some(Terminator::Jump(b)) if b == header) {
        return None;
    }
    let [phis @ .., test_inst] = func.blocks[header].instructions.as_slice() else { return None };
    let InstKind::Lt(counter, bound) = func.inst(*test_inst).kind else { return None };
    let test = func.inst_result_value(*test_inst)?;
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[header].terminator
    else {
        return None;
    };
    if condition != test || else_block != *exit || !l.blocks.contains(then_block) {
        return None;
    }
    if !defined_outside(func, l, bound) {
        return None;
    }
    // The body is a chain of single-predecessor blocks from the branch target to the latch.
    let mut body = Vec::new();
    let mut block = then_block;
    loop {
        if block == header || body.contains(&block) || func.blocks[block].predecessors.len() != 1 {
            return None;
        }
        body.push(block);
        let Some(Terminator::Jump(next)) = func.blocks[block].terminator else { return None };
        if block == *latch {
            if next != header {
                return None;
            }
            break;
        }
        block = next;
    }
    if body.len() + 1 != l.blocks.count() {
        return None;
    }
    let mut counter_phi = None;
    let mut other = Vec::new();
    for &phi in phis {
        let InstKind::Phi(incoming) = &func.inst(phi).kind else { return None };
        if incoming.len() != 2 {
            return None;
        }
        let value = func.inst_result_value(phi)?;
        if value == counter {
            counter_phi = Some((phi, incoming.clone()));
        } else {
            other.push((value, phi));
        }
    }
    let (_, incoming) = counter_phi?;
    let init = incoming.iter().find(|(b, _)| *b == preheader)?.1;
    let next = incoming.iter().find(|(b, _)| b == latch)?.1;
    if func.value_u64(init) != Some(0) {
        return None;
    }
    let Value::Inst(step) = *func.value(next) else { return None };
    match func.inst(step).kind {
        InstKind::Add(a, b)
        | InstKind::CheckedBinary { op: CheckedOp::Add, lhs: a, rhs: b, .. }
            if (a == counter && func.value_u64(b) == Some(1))
                || (b == counter && func.value_u64(a) == Some(1)) => {}
        _ => return None,
    }
    let insts: Vec<_> =
        body.iter().flat_map(|&block| func.blocks[block].instructions.iter().copied()).collect();
    if !insts.contains(&step) {
        return None;
    }
    Some(Counted {
        header,
        preheader,
        exit: *exit,
        body,
        counter,
        test,
        bound,
        phis: other,
        step,
        insts,
    })
}

/// The instruction defining `value` inside the loop body.
fn body_def<'f>(
    func: &'f Function,
    lp: &Counted,
    value: ValueId,
) -> Option<(InstId, &'f InstKind)> {
    let Value::Inst(inst) = *func.value(value) else { return None };
    lp.insts.contains(&inst).then(|| (inst, &func.inst(inst).kind))
}

/// Matches the byte index `offset + k`, or `k`. Returns the offset and the defining instruction.
fn index(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    value: ValueId,
) -> Option<(Option<ValueId>, Option<InstId>)> {
    if value == lp.counter {
        return Some((None, None));
    }
    let (inst, kind) = body_def(func, lp, value)?;
    let (a, b) = match *kind {
        InstKind::Add(a, b) => (a, b),
        InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        } => (lhs, rhs),
        _ => return None,
    };
    let offset = if a == lp.counter {
        b
    } else if b == lp.counter {
        a
    } else {
        return None;
    };
    defined_outside(func, l, offset).then_some((Some(offset), Some(inst)))
}

/// Matches the byte address `object + idx + 32` in either association. Returns the object, the
/// index value, and the address instructions.
fn byte_address(
    func: &Function,
    lp: &Counted,
    ptr: ValueId,
) -> Option<(ValueId, ValueId, [InstId; 2])> {
    let (outer, &InstKind::PtrAdd(inner_ptr, last)) = body_def(func, lp, ptr)? else { return None };
    let (inner, &InstKind::PtrAdd(object, first)) = body_def(func, lp, inner_ptr)? else {
        return None;
    };
    let idx = if func.value_u64(last) == Some(32) {
        first
    } else if func.value_u64(first) == Some(32) {
        last
    } else {
        return None;
    };
    Some((object, idx, [outer, inner]))
}

/// Finds the body's range check of `idx` against `object`'s length.
fn bounds_check(
    func: &Function,
    lp: &Counted,
    object: ValueId,
    idx: ValueId,
) -> Option<[InstId; 3]> {
    lp.insts.iter().find_map(|&inst| {
        let condition = array_check(func, inst)?;
        let (cmp, &InstKind::Lt(a, len)) = body_def(func, lp, condition)? else { return None };
        if a != idx {
            return None;
        }
        let Value::Inst(len_inst) = *func.value(len) else { return None };
        if func.inst(len_inst).kind != InstKind::Load(object, PtrLocation::Memory) {
            return None;
        }
        Some([inst, cmp, len_inst])
    })
}

/// The condition of a `panic_if_zero<0x32>` check.
fn array_check(func: &Function, inst: InstId) -> Option<ValueId> {
    let InstKind::ICall {
        function:
            Callee::Builtin(Builtin::Check {
                is_zero: true,
                failure: RevertKind::Panic(PanicCode::ArrayOutOfBounds),
            }),
        args,
    } = &func.inst(inst).kind
    else {
        return None;
    };
    let [condition] = **args else { return None };
    Some(condition)
}

/// Matches `byte 0, mload(object + idx + 32)`: one byte of a `bytes` object, read through its
/// own word. Returns the object, the index and the instructions.
fn byte_read(
    func: &Function,
    lp: &Counted,
    value: ValueId,
) -> Option<(ValueId, ValueId, Vec<InstId>)> {
    let (byte_inst, &InstKind::Byte(position, word)) = body_def(func, lp, value)? else {
        return None;
    };
    if func.value_u64(position) != Some(0) {
        return None;
    }
    let (load, &InstKind::Load(ptr, PtrLocation::Memory)) = body_def(func, lp, word)? else {
        return None;
    };
    let (object, idx, address) = byte_address(func, lp, ptr)?;
    Some((object, idx, vec![byte_inst, load, address[0], address[1]]))
}

/// Accepts `and value, mask` when the mask keeps every one of the `width` leading bytes.
fn strip_mask(
    func: &Function,
    lp: &Counted,
    value: ValueId,
    width: u64,
    used: &mut Vec<InstId>,
) -> ValueId {
    if let Some((inst, &InstKind::And(a, b))) = body_def(func, lp, value) {
        let keep = top_bytes(width);
        for (operand, mask) in [(a, b), (b, a)] {
            if func.value_u256(mask).is_some_and(|mask| mask & keep == keep) {
                used.push(inst);
                return operand;
            }
        }
    }
    value
}

/// The mask of the leading `width` bytes of a word.
fn top_bytes(width: u64) -> U256 {
    if width >= 32 { U256::MAX } else { !(U256::MAX >> (8 * width as usize)) }
}

fn gather(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    let width = func.value_u64(lp.bound).filter(|w| (1..=32).contains(w))?;
    let [(result, phi)] = lp.phis.as_slice() else { return None };
    let InstKind::Phi(incoming) = &func.inst(*phi).kind else { return None };
    let init = incoming.iter().find(|(b, _)| *b == lp.preheader)?.1;
    let next = incoming.iter().find(|(b, _)| *b != lp.preheader)?.1;
    if func.value_u64(init) != Some(0) {
        return None;
    }
    let mut used = vec![lp.step];
    // next = [and] (or result, part)
    let combined = strip_mask(func, lp, next, width, &mut used);
    let (or_inst, &InstKind::Or(a, b)) = body_def(func, lp, combined)? else { return None };
    used.push(or_inst);
    let part = if a == *result {
        b
    } else if b == *result {
        a
    } else {
        return None;
    };
    // part = [and] shr (shl byte, 248), 8 * k
    let part = strip_mask(func, lp, part, width, &mut used);
    let (shr_inst, &InstKind::Shr(shift, high)) = body_def(func, lp, part)? else { return None };
    used.push(shr_inst);
    used.extend(byte_scale(func, lp, shift)?);
    let (shl_inst, &InstKind::Shl(by, byte)) = body_def(func, lp, high)? else { return None };
    if func.value_u64(by) != Some(248) {
        return None;
    }
    used.push(shl_inst);
    let (object, idx, read) = byte_read(func, lp, byte)?;
    used.extend(read);
    if !defined_outside(func, l, object) {
        return None;
    }
    let (offset, index_inst) = index(func, l, lp, idx)?;
    used.extend(index_inst);
    used.extend(bounds_check(func, lp, object, idx)?);
    only_pattern_effects(func, lp, &used).then_some(Shape::Gather {
        object,
        offset,
        width,
        result: *result,
    })
}

/// Matches `8 * k` in the spellings checked Solidity lowers it to.
fn byte_scale(func: &Function, lp: &Counted, shift: ValueId) -> Option<Option<InstId>> {
    let (inst, kind) = body_def(func, lp, shift)?;
    let ok = match *kind {
        InstKind::Mul(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Mul,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs: a,
            rhs: b,
        } => {
            (a == lp.counter && func.value_u64(b) == Some(8))
                || (b == lp.counter && func.value_u64(a) == Some(8))
        }
        InstKind::Shl(by, value) => value == lp.counter && func.value_u64(by) == Some(3),
        _ => false,
    };
    ok.then_some(Some(inst))
}

fn scatter(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    let width = func.value_u64(lp.bound).filter(|w| (1..=32).contains(w))?;
    if !lp.phis.is_empty() {
        return None;
    }
    let mut used = vec![lp.step];
    let mut stores =
        lp.insts.iter().filter(|&&inst| matches!(func.inst(inst).kind, InstKind::Store8(..)));
    let (&store, None) = (stores.next()?, stores.next()) else { return None };
    let InstKind::Store8(ptr, byte) = func.inst(store).kind else { return None };
    used.push(store);
    let (byte_inst, &InstKind::Byte(position, value)) = body_def(func, lp, byte)? else {
        return None;
    };
    if position != lp.counter || !defined_outside(func, l, value) {
        return None;
    }
    used.push(byte_inst);
    let (object, idx, address) = byte_address(func, lp, ptr)?;
    used.extend(address);
    if !defined_outside(func, l, object) {
        return None;
    }
    let (offset, index_inst) = index(func, l, lp, idx)?;
    used.extend(index_inst);
    used.extend(bounds_check(func, lp, object, idx)?);
    // `value[k]` checks `k` against the width of `value`, which the header test implies.
    used.extend(implied_checks(func, lp, width));
    only_pattern_effects(func, lp, &used).then_some(Shape::Scatter { object, offset, width, value })
}

/// Range checks in the body that the header test `k < bound` implies: the test itself, or
/// `k < m` for a literal `m >= bound`.
fn implied_checks(func: &Function, lp: &Counted, bound: u64) -> Vec<InstId> {
    lp.insts
        .iter()
        .copied()
        .filter(|&inst| {
            array_check(func, inst).is_some_and(|condition| {
                condition == lp.test
                    || matches!(body_def(func, lp, condition),
                        Some((_, &InstKind::Lt(k, m)))
                            if k == lp.counter && func.value_u64(m).is_some_and(|m| m >= bound))
            })
        })
        .collect()
}

fn copy(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    if !lp.phis.is_empty() {
        return None;
    }
    let mut used = vec![lp.step];
    let mut stores =
        lp.insts.iter().filter(|&&inst| matches!(func.inst(inst).kind, InstKind::Store8(..)));
    let (&store, None) = (stores.next()?, stores.next()) else { return None };
    let InstKind::Store8(dst_ptr, byte) = func.inst(store).kind else { return None };
    used.push(store);
    let (src, src_idx, read) = byte_read(func, lp, byte)?;
    used.extend(read);
    let (dst, dst_idx, address) = byte_address(func, lp, dst_ptr)?;
    used.extend(address);
    if !defined_outside(func, l, src) || !defined_outside(func, l, dst) {
        return None;
    }
    // The destination is fresh and the source an argument, so the ranges cannot overlap.
    if !is_fresh_allocation(func, dst) || !matches!(func.value(src), Value::Arg(..)) {
        return None;
    }
    let (src_offset, src_index) = index(func, l, lp, src_idx)?;
    used.extend(src_index);
    used.extend(bounds_check(func, lp, src, src_idx)?);
    let (dst_offset, dst_index) = index(func, l, lp, dst_idx)?;
    used.extend(dst_index);
    let dst_checked = match bounds_check(func, lp, dst, dst_idx) {
        Some(check) => {
            used.extend(check);
            true
        }
        None if dst_offset.is_none() => {
            // `out[k]` checked against `out.length`, already known to equal the bound.
            let implied = implied_checks(func, lp, u64::MAX);
            if implied.is_empty() {
                return None;
            }
            used.extend(implied);
            false
        }
        None => return None,
    };
    only_pattern_effects(func, lp, &used).then_some(Shape::Copy {
        src,
        src_offset,
        dst,
        dst_offset,
        dst_checked,
    })
}

fn is_fresh_allocation(func: &Function, value: ValueId) -> bool {
    matches!(*func.value(value), Value::Inst(inst) if matches!(func.inst(inst).kind, InstKind::Alloc { .. }))
}

/// Whether every body instruction that may fail or write belongs to the pattern.
fn only_pattern_effects(func: &Function, lp: &Counted, used: &[InstId]) -> bool {
    let used: FxHashSet<_> = used.iter().copied().collect();
    lp.insts.iter().all(|inst| used.contains(inst) || !func.inst(*inst).kind.has_side_effects())
}

fn rewrite(func: &mut Function, plan: Plan) {
    let Plan { lp, shape } = plan;
    let mut replacements = FxHashMap::default();
    // The counter leaves the loop equal to its bound.
    replacements.insert(lp.counter, lp.bound);
    let mut builder = FunctionBuilder::new_semantic(func);
    builder.switch_to_block(lp.preheader);
    match shape {
        Shape::Gather { object, offset, width, result } => {
            // bad = width > len || offset > len - width; panic_if bad, 0x32
            // word = mload (object + offset + 32); value = and word, top(width)
            let bound = builder.imm(width);
            let fails = range_fails(&mut builder, object, offset, bound, false);
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let address = data_address(&mut builder, object, offset);
            let mut value = builder.load(address, PtrLocation::Memory);
            if width < 32 {
                let mask = builder.imm(top_bytes(width));
                value = builder.and(value, mask);
            }
            replacements.insert(result, value);
        }
        Shape::Scatter { object, offset, width, value } => {
            // bad = width > len || offset > len - width; panic_if bad, 0x32
            // mstore (object + offset + 32), value
            // or, below a word: mstore address, or (and (mload address), ~top), (and value, top)
            let bound = builder.imm(width);
            let fails = range_fails(&mut builder, object, offset, bound, false);
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let address = data_address(&mut builder, object, offset);
            let stored = if width < 32 {
                let keep = builder.imm(!top_bytes(width));
                let take = builder.imm(top_bytes(width));
                let old = builder.load(address, PtrLocation::Memory);
                let old = builder.and(old, keep);
                let new = builder.and(value, take);
                builder.or(old, new)
            } else {
                value
            };
            builder.store(address, stored);
        }
        Shape::Copy { src, src_offset, dst, dst_offset, dst_checked } => {
            // bad = bound != 0 && (src range fails || dst range fails); panic_if bad, 0x32
            // mcopy (dst + dst_offset + 32), (src + src_offset + 32), bound
            let mut fails = range_fails(&mut builder, src, src_offset, lp.bound, true);
            if dst_checked {
                let dst_fails = range_fails(&mut builder, dst, dst_offset, lp.bound, true);
                fails = builder.or(fails, dst_fails);
            }
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let to = data_address(&mut builder, dst, dst_offset);
            let from = data_address(&mut builder, src, src_offset);
            builder.copy(to, from, lp.bound, PtrLocation::Memory);
        }
    }
    func.replace_uses(&replacements);
    // header: jump exit
    fold_terminator_to_jump(func, lp.header, lp.exit);
    for block in lp.body {
        let _ = invalidate_unreachable_block(func, block);
    }
    func.blocks[lp.header].instructions.clear();
}

/// Emits whether `count` bytes from `offset` do not fit in `object`: `count > len || offset >
/// len - count`, and with `nonempty` only when `count != 0`.
fn range_fails(
    builder: &mut FunctionBuilder<'_>,
    object: ValueId,
    offset: Option<ValueId>,
    count: ValueId,
    nonempty: bool,
) -> ValueId {
    // len = mload object
    // short = lt len, count
    let len = builder.load(object, PtrLocation::Memory);
    let mut fails = builder.lt(len, count);
    if let Some(offset) = offset {
        // room = sub len, count; past = gt offset, room; fails = or short, past
        let room = builder.sub(len, count);
        let past = builder.gt(offset, room);
        fails = builder.or(fails, past);
    }
    if nonempty {
        // fails = and fails, (ne count, 0)
        let nonzero = builder.ne_zero(count);
        fails = builder.and(fails, nonzero);
    }
    fails
}

/// Emits `object + offset + 32`, the address of the byte at `offset`.
fn data_address(
    builder: &mut FunctionBuilder<'_>,
    object: ValueId,
    offset: Option<ValueId>,
) -> ValueId {
    let base = match offset {
        Some(offset) => builder.ptradd(object, offset),
        None => object,
    };
    let header = builder.imm(32);
    builder.ptradd(base, header)
}

fn defined_outside(func: &Function, l: &Loop, value: ValueId) -> bool {
    let Value::Inst(inst) = *func.value(value) else { return true };
    !l.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst))
}

fn escapes(func: &Function, l: &Loop, value: ValueId) -> bool {
    func.blocks.iter_enumerated().any(|(block, contents)| {
        if l.blocks.contains(block) {
            return false;
        }
        let mut found = false;
        for &inst in &contents.instructions {
            func.inst(inst).kind.visit_operands(|operand| found |= operand == value);
        }
        if let Some(term) = &contents.terminator {
            term.visit_operands(|operand| found |= operand == value);
        }
        found
    })
}
