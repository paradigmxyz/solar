//! Word operations for counted loops that move one byte of a `bytes` buffer per iteration
//! (`byte-loops`).
//!
//! Checked Solidity has no word access into `bytes memory`, so it spells one as a loop over the
//! word's bytes, and a copy as a loop over the copied bytes:
//!
//! ```solidity
//! for (uint256 k; k < 32; ++k) word |= bytes32(b[offset + k]) >> (8 * k); // gather
//! for (uint256 k; k < n; ++k) b[offset + k] = word[k];                    // scatter
//! for (uint256 k; k < n; ++k) out[outOffset + k] = b[offset + k];         // copy
//! ```
//!
//! Each iteration checks its indices, reads a whole word to extract one byte, or writes one
//! byte, so a 32-byte word costs about two thousand gas. This pass replaces such a loop with one
//! range check and the word operation it spells: `mload` (or `calldataload`) and a mask for a
//! gather, `mstore` or a masked read-modify-write for a scatter, and `mcopy` (or
//! `calldatacopy`) for a copy.
//!
//! # Recognition
//!
//! A loop qualifies when it is a canonical counted loop: a preheader, one exit, a header holding
//! only phis and `k < bound` for a loop-invariant bound, a counter `k` that starts at zero and
//! steps by one, and a body of straight-line blocks. The body must consist of exactly the
//! pattern's instructions plus pure instructions whose results nothing outside the loop reads.
//! Every instruction that may fail or write is part of the pattern:
//!
//! - each access's index `offset + k` (`checked_add`, or `k` itself) and its bounds check
//!   `panic_if_zero<0x32> (lt index, length)`, where the length is `mload object` for a memory
//!   object, or the length a fresh allocation stored once load forwarding has replaced that load,
//!   and the slice's length field for a calldata slice;
//! - a gather's `byte 0` of the word at the indexed byte, shifted into place by `8 * k` and ORed
//!   into an accumulator phi that starts at zero, with optional masks that keep the leading bytes
//!   of a `bytesN` result;
//! - a scatter's `mstore8` of `byte k, value` for a loop-invariant `value`, whose index check on
//!   `k` against the width of `value` is either a literal or the header test, or a fill's `mstore8`
//!   of a byte the loop does not vary, such as the constant `value[k]` folds to for a constant
//!   `value`, over at most a word;
//! - a copy's `mstore8` of a gathered byte into a memory object, whose index check may be the
//!   header test itself when the destination's length is the bound.
//!
//! # Safety
//!
//! Every check a matched loop runs raises `Panic(0x32)`, and the loop fails exactly when some
//! `k < bound` has an index at or past its buffer's length, or `k` at or past the width of a
//! scattered value. Within an iteration an index `offset + k` is computed with a checked
//! addition before its bounds check, but that addition can only overflow at a `k` beyond
//! `length - offset`, where an earlier iteration's bounds check already failed: a length is
//! below `2^256`. A gather's shift `8 * k` can only overflow after `2^253` iterations, which the
//! bounds check or gas ends first. So the first failure is always `Panic(0x32)`, and the
//! replacement raises it exactly when the bound is not zero and the bound exceeds the length or
//! the offset exceeds the length minus the bound, for every buffer, or a scatter's bound
//! exceeds its value's width. A length the checks read from before the loop is the value the
//! replacement compares with, so the two fail on the same inputs whatever that value is. A panic
//! reverts the call frame, so the bytes a failing loop wrote before it panicked were never
//! observable.
//!
//! The word read starts at the lowest byte the loop read and ends inside the last word the loop
//! read, so it touches no memory the loop did not. A scatter below 32 bytes reads and rewrites
//! the bytes after the ones it writes, and a copy's `mcopy` reads and writes exactly the copied
//! ranges; memory expansion can differ, which only `msize` sees. A
//! loop's own stores never reach a length word, which lies below every checked range, or feed
//! a gather, which stores nothing.
//!
//! A forward byte copy equals `mcopy` unless its destination starts strictly inside its source
//! range, where the loop copies bytes it already wrote. A calldata source, or a source and a
//! destination that are distinct allocations or an argument and a fresh allocation, cannot
//! overlap. Other copies test the addresses at run time and fall back to the original loop
//! when the destination leads into the source.
//!
//! # Place in the pipeline
//!
//! The pass runs on semantic MIR right after `loop-canonicalize`, where checks are still single
//! instructions and object identities are explicit. `inline-tagged` has already cloned the
//! `@custom:solar-inline` helpers, so their loops see the callers' objects: a copy into a buffer
//! the caller just allocated needs no overlap guard. The pass runs before the cost-based
//! inliner, so any other helper that spells a word access shrinks to a few instructions before
//! callers decide to inline it. Both optimization objectives run it: the result is smaller and
//! cheaper.
//!
//! A copy may also run downwards, from `offset + bound - 1` to `offset`, as a memmove toward
//! higher addresses is written. Its first iteration touches the highest indices, so it alone
//! decides whether and how the loop fails: the addition `offset + bound` panics with
//! `Panic(0x11)` when it wraps and the index with `Panic(0x32)` when it is past the end, the
//! source's checks before the destination's. A downward loop equals `mcopy` unless its source
//! starts strictly inside the destination range, which the run-time test checks instead.
//!
//! Not handled yet: fills longer than a word, and loops that compare or gather into an integer
//! with left shifts.
//!
//! NOTE: The replacement summarizes every iteration of the loop, so its instructions carry no
//! source location rather than an arbitrary iteration's checkpoint.

use crate::mir::{
    ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, Function, FunctionBuilder, InstId,
    InstKind, MirType, Module, PanicCode, PtrLocation, RevertKind, Terminator, Value, ValueId,
    analysis::{Loop, LoopAnalyzer},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
    utils::{fold_terminator_to_jump, invalidate_unreachable_block, rebuild_predecessors},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// One in the low bit of every byte, which spreads a byte over a word.
const LANES_01: U256 = U256::from_limbs([0x0101_0101_0101_0101; 4]);

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

/// A canonical counted loop from zero by one to `bound`, or from `bound` down by one to zero.
struct Counted {
    header: BlockId,
    preheader: BlockId,
    exit: BlockId,
    body: Vec<BlockId>,
    /// The counter phi's value.
    counter: ValueId,
    /// The header test: `counter < bound`, or `counter > 0` counting down.
    test: ValueId,
    bound: ValueId,
    /// Whether the counter runs from `bound` down to one, the iteration
    /// indexing `offset + counter - 1`.
    descending: bool,
    /// The header's other phis, by result value.
    phis: Vec<(ValueId, InstId)>,
    /// The latch's counter increment.
    step: InstId,
    /// Every body instruction, in program order.
    insts: Vec<InstId>,
}

impl Counted {
    /// The bound, when it is a literal.
    fn literal_bound(&self, func: &Function) -> Option<u64> {
        func.value_u64(self.bound)
    }
}

/// A `bytes` buffer that a loop reads or writes one byte of per iteration.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Buffer {
    /// A `bytes memory` object: its length word, then its data.
    Memory(ValueId),
    /// A `bytes calldata` slice.
    Calldata(ValueId),
}

/// The byte at `offset + k` of a buffer.
struct Access {
    buffer: Buffer,
    offset: Option<ValueId>,
    /// The length the loop's check compares with, when it is a value from before the loop
    /// rather than a load of the length word.
    len: Option<ValueId>,
}

/// How a copy's ranges relate.
enum Overlap {
    /// The ranges cannot overlap.
    Disjoint,
    /// The ranges are tested at run time, keeping the loop for a destination that leads into
    /// its source.
    Guarded,
}

enum Shape {
    /// The leading bytes of a buffer from an offset, left-aligned into the accumulator `result`;
    /// `width` is the most bytes the result type holds.
    Gather { access: Access, width: u64, result: ValueId },
    /// Bytes written into a memory object; `width` is the most bytes the value's index check
    /// admits.
    Scatter { access: Access, stored: Scattered, width: u64 },
    /// Bytes of a buffer written into a memory object.
    Copy { src: Access, dst: Access, dst_checked: bool, overlap: Overlap },
}

/// What a scatter writes.
#[derive(Clone, Copy)]
enum Scattered {
    /// `byte k, value`: the leading bytes of `value`, in order.
    Word(ValueId),
    /// `byte position, value` for a `position` from before the loop, or without a position the
    /// low byte of `value`: one byte at every index.
    Fill(Option<ValueId>, ValueId),
}

struct Plan {
    lp: Counted,
    shape: Shape,
}

fn plan(func: &Function, l: &Loop) -> Option<Plan> {
    let lp = counted(func, l)?;
    let shape = if lp.descending {
        copy(func, l, &lp)?
    } else {
        gather(func, l, &lp).or_else(|| scatter(func, l, &lp)).or_else(|| copy(func, l, &lp))?
    };
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
    if let Shape::Copy { overlap: Overlap::Guarded, .. } = shape {
        // The fast path joins the loop's exit, which must not distinguish its predecessors.
        if func.blocks[lp.exit]
            .instructions
            .iter()
            .any(|&i| matches!(func.inst(i).kind, InstKind::Phi(_)))
            || escapes(func, l, lp.counter)
        {
            return None;
        }
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
    let (counter, test_bound, descending) = match func.inst(*test_inst).kind {
        InstKind::Lt(counter, bound) => (counter, Some(bound), false),
        InstKind::Gt(counter, zero) if func.value_u64(zero) == Some(0) => (counter, None, true),
        _ => return None,
    };
    let test = func.inst_result_value(*test_inst)?;
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[header].terminator
    else {
        return None;
    };
    if condition != test || else_block != *exit || !l.blocks.contains(then_block) {
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
            counter_phi = Some(incoming.clone());
        } else {
            other.push((value, phi));
        }
    }
    let incoming = counter_phi?;
    let init = incoming.iter().find(|(b, _)| *b == preheader)?.1;
    let next = incoming.iter().find(|(b, _)| b == latch)?.1;
    let Value::Inst(step) = *func.value(next) else { return None };
    let bound = if descending {
        // k = phi [bound, k - 1]; k > 0
        match func.inst(step).kind {
            InstKind::Sub(a, b)
            | InstKind::CheckedBinary { op: CheckedOp::Sub, lhs: a, rhs: b, .. }
                if a == counter && func.value_u64(b) == Some(1) => {}
            _ => return None,
        }
        init
    } else {
        // k = phi [0, k + 1]; k < bound
        if func.value_u64(init) != Some(0) {
            return None;
        }
        match func.inst(step).kind {
            InstKind::Add(a, b)
            | InstKind::CheckedBinary { op: CheckedOp::Add, lhs: a, rhs: b, .. }
                if (a == counter && func.value_u64(b) == Some(1))
                    || (b == counter && func.value_u64(a) == Some(1)) => {}
            _ => return None,
        }
        test_bound?
    };
    if !defined_outside(func, l, bound) {
        return None;
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
        descending,
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

/// Matches the byte index `offset + k`, or `k`, counting up, and `offset + k - 1`, or `k - 1`,
/// counting down. Returns the offset and the defining instructions.
fn index(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    value: ValueId,
) -> Option<(Option<ValueId>, Vec<InstId>)> {
    let mut insts = Vec::new();
    let mut value = value;
    if lp.descending {
        // index = sub (offset + k), 1
        let (inst, kind) = body_def(func, lp, value)?;
        let (InstKind::Sub(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Sub,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs: a,
            rhs: b,
        }) = *kind
        else {
            return None;
        };
        if func.value_u64(b) != Some(1) {
            return None;
        }
        insts.push(inst);
        value = a;
    }
    if value == lp.counter {
        return Some((None, insts));
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
    insts.push(inst);
    defined_outside(func, l, offset).then_some((Some(offset), insts))
}

/// Matches the memory byte address `object + idx + 32` in either association. Returns the
/// object, the index value, and the address instructions.
fn memory_address(
    func: &Function,
    lp: &Counted,
    ptr: ValueId,
) -> Option<(ValueId, ValueId, [InstId; 2])> {
    let (outer, &InstKind::PtrAdd(inner_ptr, last)) = body_def(func, lp, ptr)? else {
        return None;
    };
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

/// Matches the calldata byte address `ptrtoint(slice.ptr) + idx`. Returns the slice, the index
/// value, and the address instructions.
fn calldata_address(
    func: &Function,
    lp: &Counted,
    address: ValueId,
) -> Option<(ValueId, ValueId, Vec<InstId>)> {
    let (add, &InstKind::Add(a, b)) = body_def(func, lp, address)? else { return None };
    for (base, idx) in [(a, b), (b, a)] {
        let Some((cast, &InstKind::PtrToInt(ptr, 256))) = body_def(func, lp, base) else {
            continue;
        };
        let Some((field, &InstKind::ExtractValue { aggregate, index, .. })) =
            body_def(func, lp, ptr)
        else {
            continue;
        };
        if index == MirType::SLICE_PTR_FIELD && is_calldata_slice(func, aggregate) {
            return Some((aggregate, idx, vec![add, cast, field]));
        }
    }
    None
}

fn is_calldata_slice(func: &Function, value: ValueId) -> bool {
    matches!(func.value_ty(value), Some(MirType::Slice(PtrLocation::Calldata)))
}

/// Finds the body's range check of `idx` against the buffer's length. Returns the check's
/// instructions and, for a length from before the loop, that length.
fn bounds_check(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    buffer: Buffer,
    idx: ValueId,
) -> Option<(Vec<InstId>, Option<ValueId>)> {
    lp.insts.iter().find_map(|&inst| {
        let condition = array_check(func, inst)?;
        let (cmp, &InstKind::Lt(a, len)) = body_def(func, lp, condition)? else { return None };
        if a != idx {
            return None;
        }
        if let Buffer::Memory(object) = buffer
            && stored_length(func, object) == Some(len)
            && defined_outside(func, l, len)
        {
            return Some((vec![inst, cmp], Some(len)));
        }
        let Value::Inst(len_inst) = *func.value(len) else { return None };
        let matches = match buffer {
            Buffer::Memory(object) => {
                func.inst(len_inst).kind == InstKind::Load(object, PtrLocation::Memory)
            }
            Buffer::Calldata(slice) => matches!(
                func.inst(len_inst).kind,
                InstKind::ExtractValue { aggregate, index, .. }
                    if aggregate == slice && index == MirType::SLICE_LEN_FIELD
            ),
        };
        matches.then(|| (vec![inst, cmp, len_inst], None))
    })
}

/// The length a fresh allocation's block first stores into its length word, as `new bytes(n)`
/// does. Load forwarding gives the object's checks that value in place of the length load.
fn stored_length(func: &Function, object: ValueId) -> Option<ValueId> {
    let Value::Inst(alloc) = *func.value(object) else { return None };
    if !matches!(func.inst(alloc).kind, InstKind::Alloc { .. }) {
        return None;
    }
    let block = func.blocks.iter().find(|block| block.instructions.contains(&alloc))?;
    block.instructions.iter().find_map(|&inst| match func.inst(inst).kind {
        InstKind::Store(ptr, len, PtrLocation::Memory) if ptr == object => Some(len),
        _ => None,
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

/// Matches `byte 0` of the word at one byte of a memory object or a calldata slice, indexed by
/// `offset + k` and range checked. Returns the access and its instructions.
fn byte_read(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    value: ValueId,
) -> Option<(Access, Vec<InstId>)> {
    let (byte_inst, &InstKind::Byte(position, word)) = body_def(func, lp, value)? else {
        return None;
    };
    if func.value_u64(position) != Some(0) {
        return None;
    }
    let mut used = vec![byte_inst];
    let (buffer, idx) = match body_def(func, lp, word)? {
        (load, &InstKind::Load(ptr, PtrLocation::Memory)) => {
            let (object, idx, address) = memory_address(func, lp, ptr)?;
            used.push(load);
            used.extend(address);
            (Buffer::Memory(object), idx)
        }
        (load, &InstKind::Load(address, PtrLocation::Calldata)) => {
            let (slice, idx, address) = calldata_address(func, lp, address)?;
            used.push(load);
            used.extend(address);
            (Buffer::Calldata(slice), idx)
        }
        _ => return None,
    };
    let access = checked_access(func, l, lp, buffer, idx, &mut used)?;
    Some((access, used))
}

/// Matches the index and bounds check of an access to `buffer` at `idx`.
fn checked_access(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    buffer: Buffer,
    idx: ValueId,
    used: &mut Vec<InstId>,
) -> Option<Access> {
    let (Buffer::Memory(object) | Buffer::Calldata(object)) = buffer;
    if !defined_outside(func, l, object) {
        return None;
    }
    let (offset, index_insts) = index(func, l, lp, idx)?;
    used.extend(index_insts);
    let (check, len) = bounds_check(func, l, lp, buffer, idx)?;
    used.extend(check);
    Some(Access { buffer, offset, len })
}

/// Accepts `and value, mask` when the mask keeps the leading bytes of a word. Returns the
/// operand and the number of leading bytes the mask keeps, or 32 without a mask.
fn strip_mask(
    func: &Function,
    lp: &Counted,
    value: ValueId,
    used: &mut Vec<InstId>,
) -> (ValueId, u64) {
    if let Some((inst, &InstKind::And(a, b))) = body_def(func, lp, value) {
        for (operand, mask) in [(a, b), (b, a)] {
            if let Some(width) = func.value_u256(mask).and_then(leading_bytes) {
                used.push(inst);
                return (operand, width);
            }
        }
    }
    (value, 32)
}

/// The number of leading bytes a mask keeps, when it keeps exactly a run of leading bytes.
fn leading_bytes(mask: U256) -> Option<u64> {
    (1..=32).find(|&width| mask == top_bytes(width))
}

/// The mask of the leading `width` bytes of a word.
fn top_bytes(width: u64) -> U256 {
    if width >= 32 { U256::MAX } else { !(U256::MAX >> (8 * width as usize)) }
}

fn gather(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    let [(result, phi)] = lp.phis.as_slice() else { return None };
    let InstKind::Phi(incoming) = &func.inst(*phi).kind else { return None };
    let init = incoming.iter().find(|(b, _)| *b == lp.preheader)?.1;
    let next = incoming.iter().find(|(b, _)| *b != lp.preheader)?.1;
    if func.value_u64(init) != Some(0) {
        return None;
    }
    let mut used = vec![lp.step];
    // next = [and] (or result, part)
    let (combined, outer_width) = strip_mask(func, lp, next, &mut used);
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
    let (part, inner_width) = strip_mask(func, lp, part, &mut used);
    let width = outer_width.min(inner_width);
    if lp.literal_bound(func).is_some_and(|bound| bound == 0 || bound > width) {
        return None;
    }
    let (shr_inst, &InstKind::Shr(shift, high)) = body_def(func, lp, part)? else { return None };
    used.push(shr_inst);
    used.extend(byte_scale(func, lp, shift)?);
    let (shl_inst, &InstKind::Shl(by, byte)) = body_def(func, lp, high)? else { return None };
    if func.value_u64(by) != Some(248) {
        return None;
    }
    used.push(shl_inst);
    let (access, read) = byte_read(func, l, lp, byte)?;
    used.extend(read);
    only_pattern_effects(func, lp, &used).then_some(Shape::Gather {
        access,
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
    if !lp.phis.is_empty() {
        return None;
    }
    let mut used = vec![lp.step];
    let store = single_store(func, lp)?;
    let InstKind::Store8(ptr, byte) = func.inst(store).kind else { return None };
    used.push(store);
    // `byte k, value` writes the bytes of `value` in order. A byte the loop does not vary, as
    // `value[k]` folds to for a constant `value` or `b[k] = c` stores for a `bytes1 c`, fills
    // every index.
    let stored = match body_def(func, lp, byte) {
        Some((byte_inst, &InstKind::Byte(position, value)))
            if position == lp.counter && defined_outside(func, l, value) =>
        {
            used.push(byte_inst);
            Scattered::Word(value)
        }
        Some((byte_inst, &InstKind::Byte(position, value)))
            if defined_outside(func, l, position) && defined_outside(func, l, value) =>
        {
            used.push(byte_inst);
            Scattered::Fill(Some(position), value)
        }
        None if defined_outside(func, l, byte) => Scattered::Fill(None, byte),
        _ => return None,
    };
    let (object, idx, address) = memory_address(func, lp, ptr)?;
    used.extend(address);
    let access = checked_access(func, l, lp, Buffer::Memory(object), idx, &mut used)?;
    // `value[k]` checks `k` against the width of `value`: a literal, or the header test when the
    // bound is that width. A loop without that check writes `byte k`, zero past the word, for every
    // `k`, so only a literal bound up to a word can do without it.
    let mut width = None;
    for inst in lp.insts.iter().copied() {
        let Some(condition) = array_check(func, inst) else { continue };
        if used.contains(&inst) {
            continue;
        }
        let checked = if condition == lp.test {
            lp.literal_bound(func)?
        } else {
            let Some((cmp, &InstKind::Lt(k, limit))) = body_def(func, lp, condition) else {
                return None;
            };
            used.push(cmp);
            if k != lp.counter {
                return None;
            }
            func.value_u64(limit)?
        };
        width = Some(width.map_or(checked, |w: u64| w.min(checked)));
        used.push(inst);
    }
    let width = match (width, lp.literal_bound(func)) {
        (Some(width), _) => width.min(32),
        (None, Some(_)) => 32,
        (None, None) => return None,
    };
    if width == 0 || lp.literal_bound(func).is_some_and(|bound| bound == 0 || bound > width) {
        return None;
    }
    only_pattern_effects(func, lp, &used).then_some(Shape::Scatter { access, stored, width })
}

/// The body's only `mstore8`.
fn single_store(func: &Function, lp: &Counted) -> Option<InstId> {
    let mut stores =
        lp.insts.iter().filter(|&&inst| matches!(func.inst(inst).kind, InstKind::Store8(..)));
    let (&store, None) = (stores.next()?, stores.next()) else { return None };
    Some(store)
}

fn copy(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    if !lp.phis.is_empty() {
        return None;
    }
    let mut used = vec![lp.step];
    let store = single_store(func, lp)?;
    let InstKind::Store8(dst_ptr, byte) = func.inst(store).kind else { return None };
    used.push(store);
    let (src, read) = byte_read(func, l, lp, byte)?;
    used.extend(read);
    let (dst, dst_idx, address) = memory_address(func, lp, dst_ptr)?;
    used.extend(address);
    if !defined_outside(func, l, dst) {
        return None;
    }
    let (dst_offset, dst_index) = index(func, l, lp, dst_idx)?;
    used.extend(dst_index);
    let mut dst_len = None;
    let dst_checked = match bounds_check(func, l, lp, Buffer::Memory(dst), dst_idx) {
        Some((check, len)) => {
            used.extend(check);
            dst_len = len;
            true
        }
        None if dst_offset.is_none() && !lp.descending => {
            // `out[k]` checked against `out.length`, already known to equal the bound.
            let check =
                lp.insts.iter().copied().find(|&inst| {
                    !used.contains(&inst) && array_check(func, inst) == Some(lp.test)
                })?;
            used.push(check);
            false
        }
        None => return None,
    };
    let overlap = match src.buffer {
        Buffer::Calldata(_) => Overlap::Disjoint,
        Buffer::Memory(object) if disjoint_objects(func, object, dst) => Overlap::Disjoint,
        Buffer::Memory(_) => Overlap::Guarded,
    };
    only_pattern_effects(func, lp, &used).then_some(Shape::Copy {
        src,
        dst: Access { buffer: Buffer::Memory(dst), offset: dst_offset, len: dst_len },
        dst_checked,
        overlap,
    })
}

/// Whether two memory objects are provably distinct allocations, or an argument and an
/// allocation made in this function, which lies above every object that existed on entry.
fn disjoint_objects(func: &Function, a: ValueId, b: ValueId) -> bool {
    let fresh = |v| matches!(*func.value(v), Value::Inst(i) if matches!(func.inst(i).kind, InstKind::Alloc { .. }));
    let argument = |v| matches!(func.value(v), Value::Arg(..));
    a != b && ((fresh(a) && (fresh(b) || argument(b))) || (fresh(b) && argument(a)))
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
    let literal = lp.literal_bound(func);
    let mut builder = FunctionBuilder::new_semantic(func);
    builder.switch_to_block(lp.preheader);
    match shape {
        Shape::Gather { access, width, result } => {
            // panic_if (bound != 0 && range does not fit), 0x32
            // word = load (buffer data + offset); value = and word, top(min(bound, width))
            let fails = range_fails(&mut builder, &access, lp.bound, literal.is_none());
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let word = load_word(&mut builder, &access);
            let mask = leading_mask(&mut builder, lp.bound, literal, width);
            let value = match mask {
                Some(mask) => builder.and(word, mask),
                None => word,
            };
            replacements.insert(result, value);
        }
        Shape::Scatter { access, stored, width } => {
            // fill: value = mul (byte position, value | and value, 0xff), 0x0101...01
            // panic_if (bound != 0 && (range does not fit || bound > width)), 0x32
            // mstore address, value
            // or, below a word: mstore address, or (and (mload address), ~mask), (and value, mask)
            let value = match stored {
                Scattered::Word(value) => value,
                Scattered::Fill(None, value)
                    if let Some(byte) = builder.func().value_u256(value) =>
                {
                    builder.imm((byte & U256::from(0xff)) * LANES_01)
                }
                Scattered::Fill(position, value) => {
                    let byte = match position {
                        Some(position) => builder.byte(position, value),
                        None => {
                            let low = builder.imm(0xff);
                            builder.and(value, low)
                        }
                    };
                    let lanes = builder.imm(LANES_01);
                    builder.mul(byte, lanes)
                }
            };
            let mut fails = range_fails(&mut builder, &access, lp.bound, literal.is_none());
            if literal.is_none() {
                // A dynamic bound always comes with the value's index check.
                let limit = builder.imm(width);
                let wide = builder.gt(lp.bound, limit);
                fails = builder.or(fails, wide);
            }
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let address = memory_data_address(&mut builder, &access);
            let stored = match leading_mask(&mut builder, lp.bound, literal, 32) {
                Some(mask) => {
                    let keep = builder.not(mask);
                    let old = builder.load(address, PtrLocation::Memory);
                    let old = builder.and(old, keep);
                    let new = builder.and(value, mask);
                    builder.or(old, new)
                }
                None => value,
            };
            builder.store(address, stored);
        }
        Shape::Copy { src, dst, dst_checked, overlap } => {
            let fast = match overlap {
                Overlap::Disjoint => None,
                Overlap::Guarded => {
                    // from = src data + offset; to = dst data + offset
                    // ahead = from < to && to < from + bound
                    // branch ahead, header, fast
                    let from = memory_data_address(&mut builder, &src);
                    let to = memory_data_address(&mut builder, &dst);
                    let from = builder.cast(from, MirType::I256);
                    let to = builder.cast(to, MirType::I256);
                    // A downward copy goes wrong the other way round, when its source starts
                    // inside the destination range:
                    // behind = to < from && from < to + bound
                    let (lead, trail) = if lp.descending { (to, from) } else { (from, to) };
                    let end = builder.add(lead, lp.bound);
                    let after = builder.lt(lead, trail);
                    let inside = builder.lt(trail, end);
                    let ahead = builder.and(after, inside);
                    let fast = builder.create_block();
                    builder.branch(ahead, lp.header, fast);
                    builder.switch_to_block(fast);
                    Some(fast)
                }
            };
            if lp.descending {
                // The first iteration indexes `offset + bound - 1`, the highest of all, so it
                // fails exactly when the loop does, with its own order of panics: the source
                // index's addition, then its bounds check, then the destination's.
                // panic_if (bound != 0 && offset + bound wraps), 0x11
                // panic_if (bound != 0 && offset + bound > len), 0x32
                let nonzero = builder.ne_zero(lp.bound);
                for access in [Some(&src), dst_checked.then_some(&dst)].into_iter().flatten() {
                    let len = buffer_len(&mut builder, access);
                    let end = match access.offset {
                        Some(offset) => {
                            let end = builder.add(offset, lp.bound);
                            let wraps = builder.lt(end, offset);
                            let wraps = builder.and(wraps, nonzero);
                            builder.panic_if(wraps, PanicCode::ArithmeticOverflowUnderflow);
                            end
                        }
                        None => lp.bound,
                    };
                    let past = builder.gt(end, len);
                    let past = builder.and(past, nonzero);
                    builder.panic_if(past, PanicCode::ArrayOutOfBounds);
                }
            } else {
                // panic_if (bound != 0 && (src range does not fit || dst range does not fit)), 0x32
                let mut fails = range_fails(&mut builder, &src, lp.bound, false);
                if dst_checked {
                    let dst_fails = range_fails(&mut builder, &dst, lp.bound, false);
                    fails = builder.or(fails, dst_fails);
                }
                if literal.is_none_or(|bound| bound == 0) {
                    let nonzero = builder.ne_zero(lp.bound);
                    fails = builder.and(fails, nonzero);
                }
                builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            }
            // mcopy|calldatacopy dst data + offset, src data + offset, bound
            let to = memory_data_address(&mut builder, &dst);
            let (from, location) = match src.buffer {
                Buffer::Memory(_) => (memory_data_address(&mut builder, &src), PtrLocation::Memory),
                Buffer::Calldata(slice) => {
                    let from = calldata_data_address(&mut builder, slice, src.offset);
                    (from, PtrLocation::Calldata)
                }
            };
            builder.copy(to, from, lp.bound, location);
            if fast.is_some() {
                // fast: ...; jump exit
                // The loop stays for a destination that leads into its source.
                builder.jump(lp.exit);
                rebuild_predecessors(func);
                return;
            }
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

/// Emits whether `count` bytes from the access's offset do not fit in its buffer:
/// `count > len || offset > len - count`, and with `nonempty` only when `count != 0`.
fn range_fails(
    builder: &mut FunctionBuilder<'_>,
    access: &Access,
    count: ValueId,
    nonempty: bool,
) -> ValueId {
    // len = mload object | slice.len
    // fails = lt len, count
    let len = buffer_len(builder, access);
    let mut fails = builder.lt(len, count);
    if let Some(offset) = access.offset {
        // room = sub len, count; past = gt offset, room; fails = or fails, past
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

/// Emits the length of the access's buffer, or returns the one the loop compared with.
fn buffer_len(builder: &mut FunctionBuilder<'_>, access: &Access) -> ValueId {
    if let Some(len) = access.len {
        return len;
    }
    match access.buffer {
        // len = mload object
        Buffer::Memory(object) => builder.load(object, PtrLocation::Memory),
        // len = slice.len
        Buffer::Calldata(slice) => builder.slice_len(slice),
    }
}

/// Emits the word at the access's first byte.
fn load_word(builder: &mut FunctionBuilder<'_>, access: &Access) -> ValueId {
    match access.buffer {
        Buffer::Memory(_) => {
            // word = mload (object + offset + 32)
            let address = memory_data_address(builder, access);
            builder.load(address, PtrLocation::Memory)
        }
        Buffer::Calldata(slice) => {
            // word = calldataload (ptrtoint slice.ptr + offset)
            let address = calldata_data_address(builder, slice, access.offset);
            builder.load(address, PtrLocation::Calldata)
        }
    }
}

/// Emits `object + offset + 32`, the address of a memory access's first byte.
fn memory_data_address(builder: &mut FunctionBuilder<'_>, access: &Access) -> ValueId {
    let (Buffer::Memory(object) | Buffer::Calldata(object)) = access.buffer;
    // base = ptradd object, offset; address = ptradd base, 32
    let base = match access.offset {
        Some(offset) => builder.ptradd(object, offset),
        None => object,
    };
    let header = builder.imm(32);
    builder.ptradd(base, header)
}

/// Emits `ptrtoint slice.ptr + offset`, the calldata address of a slice access's first byte.
fn calldata_data_address(
    builder: &mut FunctionBuilder<'_>,
    slice: ValueId,
    offset: Option<ValueId>,
) -> ValueId {
    // base = ptrtoint (slice.ptr); address = add base, offset
    let ptr = builder.slice_ptr(slice);
    let base = builder.cast(ptr, MirType::I256);
    match offset {
        Some(offset) => builder.add(base, offset),
        None => base,
    }
}

/// Emits the mask of the leading `min(count, width)` bytes, or `None` for a whole word.
fn leading_mask(
    builder: &mut FunctionBuilder<'_>,
    count: ValueId,
    literal: Option<u64>,
    width: u64,
) -> Option<ValueId> {
    if let Some(count) = literal {
        let bytes = count.min(width);
        return (bytes < 32).then(|| builder.imm(top_bytes(bytes)));
    }
    // bytes = select (lt count, width), count, width
    // mask = not (shr (not 0), (shl bytes, 3))
    let limit = builder.imm(width);
    let short = builder.lt(count, limit);
    let bytes = builder.select(short, count, limit);
    let three = builder.imm(3);
    let bits = builder.shl(bytes, three);
    let all = builder.imm(U256::MAX);
    let low = builder.shr(all, bits);
    Some(builder.not(low))
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
