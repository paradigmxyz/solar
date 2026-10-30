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
//! `calldatacopy`) for a copy. A copy of word elements into a memory array,
//! `out[outOffset + k] = a[offset + k]`, is the same `mcopy` (or `calldatacopy` from a calldata
//! array) of 32 bytes per element.
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
//!   object, the same load hoisted before the loop (behind trivial phis of enclosing loops), or the
//!   length a fresh allocation's block last stored, as a trusted `mstore(b, n)` after `new bytes`
//!   sets it, once load forwarding has replaced that load, and the slice's length field for a
//!   calldata slice; an index `k` may instead be checked by the header test itself, when the bound
//!   is the buffer's length from before the loop;
//! - a gather's `byte 0` of the word at the indexed byte, shifted into place by `8 * k` and ORed
//!   into an accumulator phi that starts at zero, with optional masks that keep the leading bytes
//!   of a `bytesN` result;
//! - a scatter's `mstore8` of `byte k, value` for a loop-invariant `value`, whose index check on
//!   `k` against the width of `value` is either a literal or the header test, or a fill's `mstore8`
//!   of a byte the loop does not vary, such as the constant `value[k]` folds to for a constant
//!   `value`, over at most a word; any other check of `k` bounds the loop too, but one above a word
//!   admits only a literal bound up to a word;
//! - a copy's `mstore8` of a gathered byte into a memory object, whose index check may be the
//!   header test itself when the destination's length is the bound;
//! - a word copy's `mstore` of an `mload`, both at `object + (index << 5) + 32` of word arrays, or
//!   of a `calldataload` at `slice.ptr + (index << 5)` with the check that the shift keeps the
//!   index, counting up, with the same checks as a byte copy. The ranges, offsets and lengths are
//!   then counted in elements, and the copied bytes are 32 per element.
//!
//! # Safety
//!
//! A matched loop's bounds checks raise `Panic(0x32)` when some `k < bound` has an index at or
//! past its buffer's length, or `k` at or past the width of a scattered value. Within an
//! iteration an index `offset + k` is computed with a checked addition before its bounds
//! check, but that addition can only overflow at a `k` beyond
//! `length - offset`, where an earlier iteration's bounds check already failed: a length is
//! below `2^256`. A gather's shift `8 * k` can only overflow after `2^253` iterations, which the
//! bounds check or gas ends first. A calldata word's shift `index << 5` fails at an index of
//! `2^251` or more, past a decoded slice's length. Assembly can forge a longer slice, so a
//! guarded word copy keeps its loop if scaling its index range could overflow, preserving the
//! original `Panic(0x41)` and its order relative to other checks. Memory word indices scale
//! unchecked, but the bulk byte count must not truncate: a guarded memory copy keeps its loop
//! when the count exceeds `2^251 - 1`. Unlike an offset, which may legitimately wrap to a low
//! address, such a count describes more bytes than a single `mcopy` can represent. Otherwise the
//! first failure is `Panic(0x32)`, and the replacement raises it when the bound is not zero and the
//! bound exceeds the length or the offset exceeds the length minus the bound, for every buffer, or
//! a scatter's bound exceeds its value's width. The replacement compares against the length read
//! before the loop, so the two fail on the same inputs whatever that value is. A panic
//! reverts the call frame, so the bytes a failing loop wrote before it panicked were never
//! observable.
//!
//! The word read starts at the lowest byte the loop read and ends inside the last word the loop
//! read, so it touches no memory the loop did not. A loop of a dynamic count may run no iterations,
//! though, and then touches no memory at all, so a gather or scatter of such a count over memory
//! runs behind a test that skips it for a count of zero: the buffer may then lie anywhere, even
//! where one word would expand memory until the call runs out of gas. A loop over a calldata slice
//! adds each index to the slice's offset unchecked, so past `2^256` it reads the start of calldata
//! while a bulk copy continues reading zeros. Only a slice whose offset inline assembly
//! set wraps, never an external function's decoded argument; a gather or copy of any other slice
//! tests for it at run time and keeps the loop then, joining the loop's result with the gathered
//! word after it for a gather. A scatter below 32 bytes reads and rewrites the bytes after the ones
//! it writes, and a copy's `mcopy` reads and writes exactly the copied ranges; memory expansion can
//! differ, which only `msize` sees. A gather stores nothing. Scatters, fills and copies keep their
//! original loop when an effective destination address wraps below its object's data start: a
//! store can then change the length checked by the next iteration. Copies also keep the loop when
//! the source address wraps, since its header need not precede its data. Other copies' stores may
//! reach their source's length word, which the next paragraph covers.
//!
//! A forward byte copy equals `mcopy` unless its destination starts strictly inside its source
//! range, where the loop copies bytes it already wrote. A source and a destination at distinct
//! allocation sites that the alias analysis keeps apart cannot overlap while their ranges stay
//! inside their allocations: from offset zero, an allocation sized for the bound holds its range,
//! and otherwise the length the checks compare with bounds it when that is the length the
//! allocation stored and nothing wrote the length word since. Inline assembly can grow a length
//! past its allocation, and the range it admits then runs into the next object. A memory argument
//! may overlap a fresh allocation, so other copies test the addresses at run time and fall back to
//! the original loop when the destination leads into the source. Such a destination may also cover
//! the source's length word, which the loop's checks reload after every store unless they compare
//! with a length from before the loop, while the copy reads it once. For a source other than the
//! destination, the test therefore keeps the loop whenever the destination range meets the span
//! from the source's length word to the end of its range, which also catches every overlap the
//! plain test does, in either direction, when neither address wraps below its data start.
//!
//! # Fresh destinations
//!
//! A copy of the whole of a buffer that `new bytes(bound)` allocated, zeroed, right before the
//! loop overwrites what that zeroing wrote, but for the padding after the copied bytes. When a
//! zero word stored at the end of the copied bytes follows the loop, as a port pads its result for
//! legacy solc, or the source's length read before the allocation proves the copy's range, the
//! allocation stops zeroing: that word, or else a zero store to the allocation's last word before
//! the copy, clears the padding. Nothing between the allocation and the loop may touch memory but
//! stores of its length, so nothing reads the bytes unzeroed. Each such store must keep the
//! bound as the length, and the allocation's size must be `(bound + 63) & ~31`, so the padding
//! fits in one word and the loop cannot fail a destination bounds check.
//!
//! An argument source may overlap the allocation, and the loop would then read its zeroed bytes.
//! So a guarded copy takes its fast path only when the source the loop reads, its length word and
//! its bytes up to the copied range's end, stays off the allocation. A fast path that checks the
//! source's range against its length from before the allocation proves it in advance and drops
//! the check; its test also keeps the source off the free memory pointer, below the zero slot, so
//! that length is still the source's. The slow path zeroes the buffer as the allocation would
//! have, then copies as the guarded copy above does. A wrapped source start takes this slow path
//! too, so the loop cannot read bytes whose zeroing was skipped. Its loop is entered through that
//! test, not a preheader, so no loop pass takes it up again.
//!
//! The loop's header test skips its body for a zero bound. A copy must keep that test whenever
//! its setup loads a memory buffer's length: even an unused load can expand memory or run out of
//! gas. When the setup needs no such load, [`crate::target::Target::tests_empty_copies`] prices
//! the optional test. Gas builds keep it to skip the overlap test, checks and copy for an empty
//! range, while size builds mask the panic conditions with `bound != 0` instead.
//!
//! # Place in the pipeline
//!
//! The pass runs on semantic MIR right after `loop-canonicalize`, where checks are still single
//! instructions and object identities are explicit. `inline-tagged` has already cloned the
//! `@custom:solar-inline` helpers, so their loops see the callers' objects: a copy between two
//! buffers the caller allocated needs no overlap guard. The pass runs before the cost-based
//! inliner, so any other helper that spells a word access shrinks to a few instructions before
//! callers decide to inline it. Both optimization objectives run it: the result is smaller and
//! cheaper.
//!
//! A copy may also run downwards, from `offset + bound - 1` to `offset`, as a memmove toward
//! higher addresses is written. Its first iteration touches the highest indices, so it alone
//! decides whether and how the loop fails: a checked addition `offset + bound` panics with
//! `Panic(0x11)` when it wraps and the index with `Panic(0x32)` when it is past the end, the
//! source's checks before the destination's, as one assignment evaluates them. A destination step
//! that may fail ahead of the source's check, as an index computed in an earlier statement, leaves
//! the loop alone. When an unchecked addition wraps, the falling index
//! wraps from zero to `2^256 - 1` in some iteration, whose bounds check fails, so the replacement
//! raises `Panic(0x32)` for it. That holds only if no `Panic(0x11)` can come first, so the pass
//! leaves alone a wrapping source index before a checked destination addition, which raises it
//! in the first iteration, and an index that adds unchecked but subtracts its one with a check,
//! which raises it where the sum reaches zero. A downward loop equals `mcopy` unless its source
//! starts strictly inside the destination range, which the run-time test checks instead.
//!
//! Not handled yet: fills longer than a word, and loops that compare or gather into an integer
//! with left shifts.
//!
//! NOTE: The replacement summarizes every iteration of the loop, so its instructions carry no
//! source location rather than an arbitrary iteration's checkpoint.

use crate::{
    mir::{
        AllocationInitialization, ArithmeticKind, BitWidthTest, BlockId, Builtin, Callee,
        CheckedOp, ConditionUse, Function, FunctionBuilder, InstId, InstKind, MirType, Module,
        PanicCode, PtrLocation, RevertKind, Terminator, Value, ValueId,
        analysis::{
            AddressSpace, AliasAnalysis, CfgInfo, Location, LocationSize, Loop, LoopAnalyzer,
            MemoryBase, MemoryLocation,
        },
        memory::EvmMemoryLayout,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
        utils::{
            fold_terminator_to_jump, invalidate_unreachable_block, rebuild_predecessors,
            replace_inst_uses, replace_terminator_uses,
        },
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

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
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            run_function(func, analyses.runtime_target(gcx))
        })
    }
}

fn run_function(func: &mut Function, target: Target) -> bool {
    let mut changed = false;
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(plan) = loops.all_loops().find_map(|l| plan(func, l)) else { break };
        rewrite(func, plan, target);
        rebuild_predecessors(func);
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
    /// The value the header test compares the counter with, which the counter equals when the
    /// loop exits: the bound counting up, zero counting down.
    limit: ValueId,
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

/// The element at `offset + k` of a buffer: a byte, or with a `shift` of five a word.
struct Access {
    buffer: Buffer,
    offset: Option<ValueId>,
    /// The length the loop's check compares with, when it is a value from before the loop
    /// rather than a load of the length word.
    len: Option<ValueId>,
    /// The element size as a power of two: zero for bytes, five for words.
    shift: u8,
    /// Whether the index adds its offset with unchecked arithmetic, which wraps silently.
    wrapping: bool,
}

/// How a copy's ranges relate.
enum Overlap {
    /// The ranges cannot overlap.
    Disjoint,
    /// The copy is tested at run time and keeps the loop where it would differ: a destination
    /// that leads into its memory source, or a calldata range that wraps.
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
    /// The copy's destination, when `new bytes(bound)` just allocated it for the copy to fill.
    fresh: Option<FreshDestination>,
}

/// A copy's destination that `new bytes(bound)` allocated right before the loop, so the copy
/// overwrites every byte of it but its length word and the padding after the copied bytes.
struct FreshDestination {
    /// The allocation, which stops zeroing the bytes the copy writes.
    alloc: InstId,
    /// The bytes it reserves: the length word, `bound` bytes, and the padding to a whole word.
    size: ValueId,
    /// Whether the loop's exit first stores a zero word at the end of the copied bytes, as a
    /// port that pads its result for legacy solc does, and so clears the padding itself.
    padded: bool,
    /// Whether the source's length as read before the allocation proves the copy's range: on the
    /// fast path, whose test keeps the source off the allocation and the free memory pointer, the
    /// length word still holds that value.
    source_fits: bool,
}

fn plan(func: &Function, l: &Loop) -> Option<Plan> {
    let lp = counted(func, l)?;
    let aa = AliasAnalysis::new(func);
    let shape = if lp.descending {
        copy(func, &aa, l, &lp)?
    } else {
        gather(func, l, &lp)
            .or_else(|| scatter(func, l, &lp))
            .or_else(|| copy(func, &aa, l, &lp))
            .or_else(|| word_copy(func, &aa, l, &lp))?
    };
    // Values computed in the body die with the loop, so nothing after it may read them.
    for &inst in &lp.insts {
        if let Some(value) = func.inst_result_value(inst)
            && l.escapes(func, value)
        {
            return None;
        }
    }
    if l.escapes(func, lp.test) {
        return None;
    }
    let keeps_loop = matches!(shape, Shape::Copy { overlap: Overlap::Guarded, .. })
        || matches!(&shape, Shape::Scatter { access, .. } if has_memory_offset(func, access));
    if keeps_loop {
        // The fast path joins the loop's exit, which must not distinguish its predecessors.
        if func.block_has_phi(lp.exit) {
            return None;
        }
    }
    let fresh = fresh_destination(func, &aa, l, &lp, &shape);
    Some(Plan { lp, shape, fresh })
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
    let (counter, limit, descending) = match func.inst(*test_inst).kind {
        InstKind::Lt(counter, bound) => (counter, bound, false),
        InstKind::Gt(counter, zero) if func.value_u64(zero) == Some(0) => (counter, zero, true),
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
        limit
    };
    if l.defines(func, bound) {
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
        limit,
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
/// counting down. Returns the offset and whether its addition is unchecked, and adds the checked
/// instructions that define it to `used`.
fn index(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    value: ValueId,
    used: &mut Vec<InstId>,
) -> Option<(Option<ValueId>, bool)> {
    let mut value = value;
    let mut checked_decrement = false;
    if lp.descending {
        // index = sub (offset + k), 1
        let (inst, kind) = body_def(func, lp, value)?;
        let (a, b, checked) = match *kind {
            InstKind::Sub(a, b) => (a, b, false),
            InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs,
                rhs,
            } => (lhs, rhs, true),
            _ => return None,
        };
        if func.value_u64(b) != Some(1) {
            return None;
        }
        if checked {
            used.push(inst);
        }
        value = a;
        checked_decrement = checked;
    }
    if value == lp.counter {
        return Some((None, false));
    }
    let (inst, kind) = body_def(func, lp, value)?;
    let (a, b, wrapping) = match *kind {
        InstKind::Add(a, b) => (a, b, true),
        InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        } => (lhs, rhs, false),
        _ => return None,
    };
    // Counting down, an unchecked sum that wraps reaches zero in some iteration, where a checked
    // decrement raises `Panic(0x11)` unless a bounds check failed first.
    if wrapping && checked_decrement {
        return None;
    }
    let offset = if a == lp.counter {
        b
    } else if b == lp.counter {
        a
    } else {
        return None;
    };
    if !wrapping {
        used.push(inst);
    }
    (!l.defines(func, offset)).then_some((Some(offset), wrapping))
}

/// Matches the memory byte address `object + idx + 32` in either association. Returns the
/// object and the index value.
fn memory_address(func: &Function, lp: &Counted, ptr: ValueId) -> Option<(ValueId, ValueId)> {
    let (_, &InstKind::PtrAdd(inner_ptr, last)) = body_def(func, lp, ptr)? else { return None };
    let (_, &InstKind::PtrAdd(object, first)) = body_def(func, lp, inner_ptr)? else {
        return None;
    };
    let idx = if func.value_u64(last) == Some(32) {
        first
    } else if func.value_u64(first) == Some(32) {
        last
    } else {
        return None;
    };
    Some((object, idx))
}

/// Matches the calldata byte address `ptrtoint(slice.ptr) + idx`. Returns the slice and the
/// index value.
fn calldata_address(func: &Function, lp: &Counted, address: ValueId) -> Option<(ValueId, ValueId)> {
    let (_, &InstKind::Add(a, b)) = body_def(func, lp, address)? else { return None };
    for (base, idx) in [(a, b), (b, a)] {
        let Some((_, &InstKind::PtrToInt(ptr, 256))) = body_def(func, lp, base) else {
            continue;
        };
        let Some((_, &InstKind::ExtractValue { aggregate, index, .. })) = body_def(func, lp, ptr)
        else {
            continue;
        };
        if index == MirType::SLICE_PTR_FIELD
            && func.value_slice_location(aggregate) == Some(PtrLocation::Calldata)
        {
            return Some((aggregate, idx));
        }
    }
    None
}

/// Finds the body's range check of `idx` against the buffer's length. Returns the check and,
/// for a length from before the loop, that length.
fn bounds_check(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    buffer: Buffer,
    idx: ValueId,
) -> Option<(InstId, Option<ValueId>)> {
    lp.insts.iter().find_map(|&inst| {
        let condition = array_check(func, inst)?;
        // The loop test `k < bound` itself, when the bound is the buffer's length from before
        // the loop: it holds in every iteration.
        if condition == lp.test
            && idx == lp.counter
            && !lp.descending
            && outside_length(func, l, buffer, lp.bound)
        {
            return Some((inst, Some(lp.bound)));
        }
        let (_, &InstKind::Lt(a, len)) = body_def(func, lp, condition)? else { return None };
        if a != idx {
            return None;
        }
        if outside_length(func, l, buffer, len) {
            return Some((inst, Some(len)));
        }
        let matches = match buffer {
            Buffer::Memory(object) => is_length_load(func, len, object),
            Buffer::Calldata(slice) => slice_length(func, slice, len),
        };
        matches.then_some((inst, None))
    })
}

/// Whether `len`, defined before the loop, is the buffer's length: for a memory object, the
/// length an allocation stored, or a load of the length word that the loop passes hoisted,
/// possibly behind trivial loop phis; for a calldata slice, its length field.
fn outside_length(func: &Function, l: &Loop, buffer: Buffer, len: ValueId) -> bool {
    if l.defines(func, len) {
        return false;
    }
    match buffer {
        Buffer::Memory(object) => {
            let source = through_trivial_phis(func, len);
            is_length_load(func, source, object) || stored_length(func, object) == Some(source)
        }
        Buffer::Calldata(slice) => slice_length(func, slice, len),
    }
}

/// Whether `len` is a load of the length word of the memory object `object`.
fn is_length_load(func: &Function, len: ValueId, object: ValueId) -> bool {
    func.value_inst_kind(len) == Some(&InstKind::Load(object, PtrLocation::Memory))
}

/// Whether `len` is the length field of the calldata slice `slice`.
fn slice_length(func: &Function, slice: ValueId, len: ValueId) -> bool {
    matches!(
        func.value_inst_kind(len),
        Some(&InstKind::ExtractValue { aggregate, index, .. })
            if aggregate == slice && index == MirType::SLICE_LEN_FIELD
    )
}

/// The value `value` stands for once phis whose inputs are only themselves and one other value
/// are looked through.
fn through_trivial_phis(func: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..8 {
        let Some(InstKind::Phi(incoming)) = func.value_inst_kind(value) else { break };
        let mut others = incoming.iter().map(|&(_, input)| input).filter(|&input| input != value);
        let Some(first) = others.next() else { break };
        if others.any(|other| other != first) {
            break;
        }
        value = first;
    }
    value
}

/// The length a fresh allocation's block leaves in its length word: the last value stored there,
/// as `new bytes(n)` does, or a trusted `mstore(b, n)` after it that sets the length of a buffer
/// allocated in whole words. Load forwarding gives the object's checks that value in place of the
/// length load.
fn stored_length(func: &Function, object: ValueId) -> Option<ValueId> {
    let Value::Inst(alloc) = *func.value(object) else { return None };
    if !matches!(func.inst(alloc).kind, InstKind::Alloc { .. }) {
        return None;
    }
    let block = func.blocks.iter().find(|block| block.instructions.contains(&alloc))?;
    block.instructions.iter().rev().find_map(|&inst| match func.memory_length_store(inst) {
        Some((ptr, len)) if ptr == object => Some(len),
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
/// `offset + k` and range checked. Returns the access and the instructions of its checks.
fn byte_read(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    value: ValueId,
) -> Option<(Access, Vec<InstId>)> {
    let (_, &InstKind::Byte(position, word)) = body_def(func, lp, value)? else { return None };
    if func.value_u64(position) != Some(0) {
        return None;
    }
    let (buffer, idx) = match *body_def(func, lp, word)?.1 {
        InstKind::Load(ptr, PtrLocation::Memory) => {
            let (object, idx) = memory_address(func, lp, ptr)?;
            (Buffer::Memory(object), idx)
        }
        InstKind::Load(address, PtrLocation::Calldata) => {
            let (slice, idx) = calldata_address(func, lp, address)?;
            (Buffer::Calldata(slice), idx)
        }
        _ => return None,
    };
    let mut used = Vec::new();
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
    if l.defines(func, object) {
        return None;
    }
    let (offset, wrapping) = index(func, l, lp, idx, used)?;
    let (check, len) = bounds_check(func, l, lp, buffer, idx)?;
    used.push(check);
    Some(Access { buffer, offset, len, shift: 0, wrapping })
}

/// Accepts `and value, mask` when the mask keeps the leading bytes of a word. Returns the
/// operand and the number of leading bytes the mask keeps, or 32 without a mask.
fn strip_mask(func: &Function, lp: &Counted, value: ValueId) -> (ValueId, u64) {
    if let Some((_, &InstKind::And(a, b))) = body_def(func, lp, value) {
        for (operand, mask) in [(a, b), (b, a)] {
            if let Some(width) = func.value_u256(mask).and_then(leading_bytes) {
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
    // next = [and] (or result, part)
    let (combined, outer_width) = strip_mask(func, lp, next);
    let (_, &InstKind::Or(a, b)) = body_def(func, lp, combined)? else { return None };
    let part = if a == *result {
        b
    } else if b == *result {
        a
    } else {
        return None;
    };
    // part = [and] shr (shl byte, 248), 8 * k
    let (part, inner_width) = strip_mask(func, lp, part);
    let width = outer_width.min(inner_width);
    if lp.literal_bound(func).is_some_and(|bound| bound == 0 || bound > width) {
        return None;
    }
    let (_, &InstKind::Shr(shift, high)) = body_def(func, lp, part)? else { return None };
    let mut used = vec![lp.step, byte_scale(func, lp, shift)?];
    let (_, &InstKind::Shl(by, byte)) = body_def(func, lp, high)? else { return None };
    if func.value_u64(by) != Some(248) {
        return None;
    }
    let (access, read) = byte_read(func, l, lp, byte)?;
    used.extend(read);
    only_pattern_effects(func, lp, &used).then_some(Shape::Gather {
        access,
        width,
        result: *result,
    })
}

/// Matches `8 * k` in the spellings checked Solidity lowers it to.
fn byte_scale(func: &Function, lp: &Counted, shift: ValueId) -> Option<InstId> {
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
    ok.then_some(inst)
}

fn scatter(func: &Function, l: &Loop, lp: &Counted) -> Option<Shape> {
    if !lp.phis.is_empty() {
        return None;
    }
    let store = single_store(func, lp, |kind| matches!(kind, InstKind::Store8(..)))?;
    let InstKind::Store8(ptr, byte) = func.inst(store).kind else { return None };
    let mut used = vec![lp.step, store];
    // `byte k, value` writes the bytes of `value` in order. A byte the loop does not vary, as
    // `value[k]` folds to for a constant `value` or `b[k] = c` stores for a `bytes1 c`, fills
    // every index.
    let stored = match body_def(func, lp, byte) {
        Some((_, &InstKind::Byte(position, value)))
            if position == lp.counter && !l.defines(func, value) =>
        {
            Scattered::Word(value)
        }
        Some((_, &InstKind::Byte(position, value)))
            if !l.defines(func, position) && !l.defines(func, value) =>
        {
            Scattered::Fill(Some(position), value)
        }
        None if !l.defines(func, byte) => Scattered::Fill(None, byte),
        _ => return None,
    };
    let (object, idx) = memory_address(func, lp, ptr)?;
    let access = checked_access(func, l, lp, Buffer::Memory(object), idx, &mut used)?;
    // `value[k]` checks `k` against the width of `value`: a literal, or the header test when the
    // bound is that width. A loop without that check writes `byte k`, zero past the word, for every
    // `k`, so only a literal bound up to a word can do without it. Any other check of `k`, such as
    // an unrelated array's, bounds the loop too, but one above a word lets a dynamic count write
    // past the word, which one word store cannot.
    let mut width = None;
    for inst in lp.insts.iter().copied() {
        let Some(condition) = array_check(func, inst) else { continue };
        if used.contains(&inst) {
            continue;
        }
        let checked = if condition == lp.test {
            lp.literal_bound(func)?
        } else {
            let Some((_, &InstKind::Lt(k, limit))) = body_def(func, lp, condition) else {
                return None;
            };
            if k != lp.counter {
                return None;
            }
            func.value_u64(limit)?
        };
        width = Some(width.map_or(checked, |w: u64| w.min(checked)));
        used.push(inst);
    }
    let width = match (width, lp.literal_bound(func)) {
        (Some(width), _) if width <= 32 => width,
        (_, Some(_)) => 32,
        (_, None) => return None,
    };
    if width == 0 || lp.literal_bound(func).is_some_and(|bound| bound == 0 || bound > width) {
        return None;
    }
    only_pattern_effects(func, lp, &used).then_some(Shape::Scatter { access, stored, width })
}

/// The body's only store matching `is_store`.
fn single_store(
    func: &Function,
    lp: &Counted,
    is_store: impl Fn(&InstKind) -> bool,
) -> Option<InstId> {
    let mut stores = lp.insts.iter().filter(|&&inst| is_store(&func.inst(inst).kind));
    let (&store, None) = (stores.next()?, stores.next()) else { return None };
    Some(store)
}

fn copy(func: &Function, aa: &AliasAnalysis, l: &Loop, lp: &Counted) -> Option<Shape> {
    if !lp.phis.is_empty() {
        return None;
    }
    let store = single_store(func, lp, |kind| matches!(kind, InstKind::Store8(..)))?;
    let InstKind::Store8(dst_ptr, byte) = func.inst(store).kind else { return None };
    let (src, read) = byte_read(func, l, lp, byte)?;
    let mut used = vec![lp.step, store];
    used.extend_from_slice(&read);
    let (dst, dst_idx) = memory_address(func, lp, dst_ptr)?;
    if l.defines(func, dst) {
        return None;
    }
    let steps = used.len();
    let (dst, dst_checked) = destination(func, l, lp, dst, dst_idx, 0, &mut used)?;
    // Counting down, a wrapping source index fails its bounds check only in some later
    // iteration, after a checked destination addition could raise `Panic(0x11)` in the first.
    if lp.descending && src.wrapping && dst.offset.is_some() && !dst.wrapping {
        return None;
    }
    // Counting down, the replacement raises the first iteration's panics in the source's order,
    // then the destination's, as one assignment evaluates them. A destination step that may fail
    // ahead of the source's check, as an index computed in an earlier statement, fails first in
    // the loop.
    if lp.descending && !source_fails_first(func, lp, &read, &used[steps..]) {
        return None;
    }
    let overlap = copy_overlap(func, aa, lp, &src, &dst);
    only_pattern_effects(func, lp, &used).then_some(Shape::Copy { src, dst, dst_checked, overlap })
}

/// Matches a copy's destination `dst[offset + k]` at index `idx`, with a bounds check or, for
/// `k` itself counting up, the check against the bound. Returns the access and whether its
/// bounds are checked against the destination's length, and adds the index and check
/// instructions to `used`.
fn destination(
    func: &Function,
    l: &Loop,
    lp: &Counted,
    dst: ValueId,
    idx: ValueId,
    shift: u8,
    used: &mut Vec<InstId>,
) -> Option<(Access, bool)> {
    let (offset, wrapping) = index(func, l, lp, idx, used)?;
    let (len, checked) = match bounds_check(func, l, lp, Buffer::Memory(dst), idx) {
        Some((check, len)) => {
            used.push(check);
            (len, true)
        }
        None if offset.is_none() && !lp.descending => {
            // `out[k]` checked against `out.length`, already known to equal the bound.
            let check =
                lp.insts.iter().copied().find(|&inst| {
                    !used.contains(&inst) && array_check(func, inst) == Some(lp.test)
                })?;
            used.push(check);
            (None, false)
        }
        None => return None,
    };
    Some((Access { buffer: Buffer::Memory(dst), offset, len, shift, wrapping }, checked))
}

/// Whether every step of a copy's source that may fail comes before every step of its
/// destination that may fail, in body order: `src` and `dst` are the steps of each access.
fn source_fails_first(func: &Function, lp: &Counted, src: &[InstId], dst: &[InstId]) -> bool {
    let position = |inst: &InstId| lp.insts.iter().position(|other| other == inst);
    let may_fail = |inst: &&InstId| func.inst(**inst).kind.effects().control.may_revert;
    let last_source = src.iter().filter(may_fail).filter_map(position).max();
    let first_destination =
        dst.iter().filter(|inst| !src.contains(*inst)).filter(may_fail).filter_map(position).min();
    match (last_source, first_destination) {
        (Some(source), Some(destination)) => source < destination,
        _ => true,
    }
}

/// Matches the memory word address `object + (idx << 5) + 32`. Returns the object and the index
/// value.
fn word_address(func: &Function, lp: &Counted, ptr: ValueId) -> Option<(ValueId, ValueId)> {
    let (_, &InstKind::PtrAdd(inner_ptr, header)) = body_def(func, lp, ptr)? else { return None };
    if func.value_u64(header) != Some(32) {
        return None;
    }
    let (_, &InstKind::PtrAdd(object, offset)) = body_def(func, lp, inner_ptr)? else {
        return None;
    };
    let (_, &InstKind::Shl(shift, idx)) = body_def(func, lp, offset)? else { return None };
    (func.value_u64(shift) == Some(5)).then_some((object, idx))
}

/// Matches the calldata word address `ptrtoint(slice.ptr) + (idx << 5)`, with the check that
/// the shift keeps `idx`. Returns the slice and the index value, and adds the check to `used`.
///
/// NOTE: A decoded slice's bounds rule out indices of `2^251` or more. For an assembly-forged
/// slice, the replacement's guard keeps the loop when scaling the index range could overflow,
/// so this check still raises `Panic(0x41)` in the original order.
fn calldata_word_address(
    func: &Function,
    lp: &Counted,
    address: ValueId,
    used: &mut Vec<InstId>,
) -> Option<(ValueId, ValueId)> {
    let (slice, offset) = calldata_address(func, lp, address)?;
    let (_, &InstKind::Shl(shift, idx)) = body_def(func, lp, offset)? else { return None };
    if func.value_u64(shift) != Some(5) {
        return None;
    }
    // panic_if (ne (shr 251, idx), 0), 0x41
    for &inst in &lp.insts {
        let InstKind::ICall {
            function:
                Callee::Builtin(Builtin::Check {
                    is_zero: false,
                    failure: RevertKind::Panic(PanicCode::MemoryAllocationOverflow),
                }),
            args,
        } = &func.inst(inst).kind
        else {
            continue;
        };
        let [condition] = **args else { continue };
        if scale_wraps(func, lp, condition, idx) {
            used.push(inst);
        }
    }
    Some((slice, idx))
}

/// Whether `condition` tests that scaling `idx` by 32 wraps: `idx` has a bit at or above 251, in
/// either spelling of the width test.
fn scale_wraps(func: &Function, lp: &Counted, condition: ValueId, idx: ValueId) -> bool {
    body_def(func, lp, condition)
        .and_then(|(_, kind)| BitWidthTest::of_kind(func, kind))
        .is_some_and(|test| test.word == idx && !test.fits && test.bits == 251)
}

/// Matches `dst[dstOffset + k] = src[srcOffset + k]` over arrays of words, counting up.
fn word_copy(func: &Function, aa: &AliasAnalysis, l: &Loop, lp: &Counted) -> Option<Shape> {
    if !lp.phis.is_empty() || lp.descending {
        return None;
    }
    let store = single_store(func, lp, |kind| {
        matches!(kind, InstKind::Store(.., PtrLocation::Memory) | InstKind::Store8(..))
    })?;
    let InstKind::Store(dst_ptr, value, PtrLocation::Memory) = func.inst(store).kind else {
        return None;
    };
    let mut used = vec![lp.step, store];
    let mut src = match *body_def(func, lp, value)?.1 {
        InstKind::Load(src_ptr, PtrLocation::Memory) => {
            let (src_object, src_idx) = word_address(func, lp, src_ptr)?;
            checked_access(func, l, lp, Buffer::Memory(src_object), src_idx, &mut used)?
        }
        InstKind::Load(address, PtrLocation::Calldata) => {
            let (slice, src_idx) = calldata_word_address(func, lp, address, &mut used)?;
            checked_access(func, l, lp, Buffer::Calldata(slice), src_idx, &mut used)?
        }
        _ => return None,
    };
    src.shift = 5;
    let (dst, dst_idx) = word_address(func, lp, dst_ptr)?;
    if l.defines(func, dst) {
        return None;
    }
    let (dst, dst_checked) = destination(func, l, lp, dst, dst_idx, 5, &mut used)?;
    let overlap = copy_overlap(func, aa, lp, &src, &dst);
    only_pattern_effects(func, lp, &used).then_some(Shape::Copy { src, dst, dst_checked, overlap })
}

/// How a copy's ranges relate. Ranges at distinct allocation sites cannot overlap while each stays
/// inside its allocation, which its allocation's size or the length its checks compare with
/// proves. Inline assembly can grow a length past its allocation, and the range it admits then
/// runs into the next object. A calldata range wraps only where inline assembly set its slice's
/// offset, never for an external function's decoded argument, but its destination must also stay
/// above its own length word. Every other copy tests at run time where the loop differs from the
/// copy.
fn copy_overlap(
    func: &Function,
    aa: &AliasAnalysis,
    lp: &Counted,
    src: &Access,
    dst: &Access,
) -> Overlap {
    let (Buffer::Memory(source), Buffer::Memory(destination)) = (src.buffer, dst.buffer) else {
        return match src.buffer {
            Buffer::Calldata(slice)
                if decoded_argument(func, slice)
                    && (dst.offset.is_none_or(|offset| func.value_u64(offset) == Some(0))
                        || within_allocation(func, aa, lp, dst)) =>
            {
                Overlap::Disjoint
            }
            _ => Overlap::Guarded,
        };
    };
    if disjoint_objects(func, aa, source, destination)
        && within_allocation(func, aa, lp, src)
        && within_allocation(func, aa, lp, dst)
    {
        Overlap::Disjoint
    } else {
        Overlap::Guarded
    }
}

/// Whether every element the loop reads or writes through the access lies inside its object's
/// allocation: from offset zero, by the allocation's size alone when it reserves the bound, as
/// `new bytes(bound)` does; otherwise when the allocation reserves the length its block stored,
/// and the loop's checks compare with that length, as a value or as the object's length word while
/// nothing between that store and the loop may write the word. The loop's own stores keep to the
/// copy's destination, inside its own allocation by the same argument, so they leave the word alone
/// too.
fn within_allocation(func: &Function, aa: &AliasAnalysis, lp: &Counted, access: &Access) -> bool {
    let Buffer::Memory(object) = access.buffer else { return false };
    let Value::Inst(alloc) = *func.value(object) else { return false };
    let InstKind::Alloc { size, .. } = func.inst(alloc).kind else { return false };
    let reserves = |count| {
        if access.shift == 0 {
            bytes_size(func, size, count)
        } else {
            word_array_size(func, size, count)
        }
    };
    if access.offset.is_none_or(|offset| func.value_u64(offset) == Some(0)) && reserves(lp.bound) {
        return true;
    }
    let Some(stored) = stored_length(func, object) else { return false };
    reserves(stored)
        && (access.len.is_some_and(|len| through_trivial_phis(func, len) == stored)
            || length_unchanged(func, aa, lp, alloc, object))
}

/// Whether nothing that may run from the last store of `object`'s length in the block of its
/// allocation `alloc` up to the loop may write that length word. The allocation must lie outside
/// any cycle, so the word is that of the one object the loop sees.
fn length_unchanged(
    func: &Function,
    aa: &AliasAnalysis,
    lp: &Counted,
    alloc: InstId,
    object: ValueId,
) -> bool {
    let Some(address) = aa.memory_address(func, object) else { return false };
    let word = Location::Memory(MemoryLocation::new(address, LocationSize::Const(32)));
    let writes = |inst| aa.instruction_mod_ref(func, inst).may_write(aa, word);
    let cfg = CfgInfo::new(func);
    let Some((home, block)) =
        func.blocks.iter_enumerated().find(|(_, block)| block.instructions.contains(&alloc))
    else {
        return false;
    };
    if cfg.cyclic_blocks().contains(home) {
        return false;
    }
    let Some(stored) = block.instructions.iter().rposition(|&inst| {
        func.memory_length_store(inst).is_some_and(|(stored, _)| stored == object)
    }) else {
        return false;
    };
    if block.instructions[stored + 1..].iter().any(|&inst| writes(inst)) {
        return false;
    }
    // The other blocks on a way from the allocation's to the loop's preheader.
    let mut after = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = cfg.successors(home).to_vec();
    while let Some(block) = stack.pop() {
        if block != home && block != lp.header && after.insert(block) {
            stack.extend_from_slice(cfg.successors(block));
        }
    }
    let mut before = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = vec![lp.preheader];
    while let Some(block) = stack.pop() {
        if block != home && before.insert(block) {
            stack.extend_from_slice(cfg.predecessors(block));
        }
    }
    after
        .iter()
        .filter(|&block| before.contains(block))
        .all(|block| !func.blocks[block].instructions.iter().any(|&inst| writes(inst)))
}

/// Whether `size` is `(len + 1) * 32`, the bytes a dynamic array of `len` words reserves: its
/// length word and its elements.
fn word_array_size(func: &Function, size: ValueId, len: ValueId) -> bool {
    let (a, b) = match func.value_inst_kind(size) {
        Some(&InstKind::Mul(a, b))
        | Some(&InstKind::CheckedBinary { op: CheckedOp::Mul, lhs: a, rhs: b, .. }) => (a, b),
        _ => return false,
    };
    let words = match (func.value_u64(a), func.value_u64(b)) {
        (_, Some(32)) => a,
        (Some(32), _) => b,
        _ => return false,
    };
    let (x, y) = match func.value_inst_kind(words) {
        Some(&InstKind::Add(x, y))
        | Some(&InstKind::CheckedBinary { op: CheckedOp::Add, lhs: x, rhs: y, .. }) => (x, y),
        _ => return false,
    };
    (func.value_u64(y) == Some(1) && same_value(func, x, len, 2))
        || (func.value_u64(x) == Some(1) && same_value(func, y, len, 2))
}

/// Whether two memory objects are distinct allocation sites, which never overlap. A memory
/// argument may overlap a fresh allocation, and an allocation after a free memory pointer reset
/// may overlap an earlier one, so only the bases the alias analysis keeps apart count.
fn disjoint_objects(func: &Function, aa: &AliasAnalysis, a: ValueId, b: ValueId) -> bool {
    let site = |value| match aa.memory_address(func, value)?.base {
        MemoryBase::Allocation(inst) | MemoryBase::DynamicAllocation(inst) => Some(inst),
        _ => None,
    };
    matches!((site(a), site(b)), (Some(a), Some(b)) if a != b)
}

/// Whether every body instruction that may fail or write belongs to the pattern.
fn only_pattern_effects(func: &Function, lp: &Counted, used: &[InstId]) -> bool {
    let used: FxHashSet<_> = used.iter().copied().collect();
    lp.insts.iter().all(|inst| used.contains(inst) || !func.inst(*inst).kind.has_side_effects())
}

/// Matches a byte copy, counting up, of the whole of a destination that `new bytes(bound)`
/// allocated, zeroed, on the only way into the loop: its size is `(bound + 63) & ~31`, its
/// block stores `bound` into its length word last, and nothing between the allocation and the
/// loop touches memory but stores of that same length.
fn fresh_destination(
    func: &Function,
    aa: &AliasAnalysis,
    l: &Loop,
    lp: &Counted,
    shape: &Shape,
) -> Option<FreshDestination> {
    let Shape::Copy { src, dst, overlap, .. } = shape else { return None };
    let Buffer::Memory(object) = dst.buffer else { return None };
    if lp.descending
        || dst.offset.is_some_and(|offset| func.value_u64(offset) != Some(0))
        || src.shift != 0
    {
        return None;
    }
    let Value::Inst(alloc) = *func.value(object) else { return None };
    let InstKind::Alloc { size, semantics } = func.inst(alloc).kind else { return None };
    if semantics.initialization != AllocationInitialization::Zeroed
        || !bytes_size(func, size, lp.bound)
        || !stored_length(func, object).is_some_and(|len| same_value(func, len, lp.bound, 2))
    {
        return None;
    }
    let touches_memory = |inst| {
        let effects = aa.instruction_mod_ref(func, inst);
        effects.reads_space(AddressSpace::Memory) || effects.writes_space(AddressSpace::Memory)
    };
    // The blocks from the allocation's to the preheader, each the only way into the next.
    let mut chain = vec![lp.preheader];
    while !func.blocks[*chain.last()?].instructions.contains(&alloc) {
        let block = *chain.last()?;
        let [pred] = func.blocks[block].predecessors.as_slice() else { return None };
        if chain.len() > 8
            || l.blocks.contains(*pred)
            || func.blocks[*pred].terminator != Some(Terminator::Jump(block))
        {
            return None;
        }
        chain.push(*pred);
    }
    let first = &func.blocks[*chain.last()?].instructions;
    let after = first.iter().position(|&inst| inst == alloc)? + 1;
    let quiet = chain[..chain.len() - 1]
        .iter()
        .flat_map(|&block| func.blocks[block].instructions.iter().copied())
        .chain(first[after..].iter().copied())
        .all(|inst| {
            func.memory_length_store(inst)
                .is_some_and(|(stored, len)| stored == object && same_value(func, len, lp.bound, 2))
                || !touches_memory(inst)
        });
    if !quiet {
        return None;
    }
    // A zero word stored at the end of the copied bytes before anything after the loop touches
    // memory but a read of the destination's length word, through the jumps that lead on from
    // the loop's exit.
    let header = |inst| func.inst(inst).kind == InstKind::Load(object, PtrLocation::Memory);
    let mut padded = false;
    let mut block = lp.exit;
    for _ in 0..8 {
        if let Some(inst) = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .find(|&inst| !header(inst) && touches_memory(inst))
        {
            padded = match func.inst(inst).kind {
                InstKind::Store(address, zero, PtrLocation::Memory) => {
                    func.value_u64(zero) == Some(0) && payload_end(func, address, object, lp.bound)
                }
                _ => false,
            };
            break;
        }
        let Some(Terminator::Jump(next)) = func.blocks[block].terminator else { break };
        block = next;
    }
    let source_fits = match (overlap, src.buffer) {
        (Overlap::Guarded, Buffer::Memory(source)) => length_before(func, aa, alloc, source)
            .is_some_and(|len| range_fits(func, src.offset, lp.bound, len)),
        _ => false,
    };
    // Without either, the copy only trades the allocation's zeroing for a store of its own.
    (padded || source_fits).then_some(FreshDestination { alloc, size, padded, source_fits })
}

/// Whether `bound` bytes from `offset` fit in a length `len`: `len` itself from offset zero, or
/// the checked `len - offset` from `offset`.
fn range_fits(func: &Function, offset: Option<ValueId>, bound: ValueId, len: ValueId) -> bool {
    match offset.filter(|&offset| func.value_u64(offset) != Some(0)) {
        None => same_value(func, bound, len, 2),
        Some(offset) => matches!(
            func.value_inst_kind(bound),
            Some(&InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs,
                rhs,
            }) if same_value(func, lhs, len, 2) && same_value(func, rhs, offset, 2)
        ),
    }
}

/// A load of `object`'s length word that runs before the allocation `alloc`, when nothing that
/// may run before the allocation writes memory: the length word then still holds that value at
/// the allocation.
fn length_before(
    func: &Function,
    aa: &AliasAnalysis,
    alloc: InstId,
    object: ValueId,
) -> Option<ValueId> {
    let cfg = CfgInfo::new(func);
    let home = func.blocks.iter_enumerated().find(|(_, b)| b.instructions.contains(&alloc))?.0;
    if cfg.cyclic_blocks().contains(home) {
        return None;
    }
    let writes = |inst| aa.instruction_mod_ref(func, inst).writes_space(AddressSpace::Memory);
    let before_alloc = &func.blocks[home].instructions;
    let before_alloc = &before_alloc[..before_alloc.iter().position(|&inst| inst == alloc)?];
    if before_alloc.iter().copied().any(writes) {
        return None;
    }
    // Every block that may run before the allocation's.
    let mut earlier = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = cfg.predecessors(home).to_vec();
    while let Some(block) = stack.pop() {
        if earlier.insert(block) {
            if func.blocks[block].instructions.iter().copied().any(writes) {
                return None;
            }
            stack.extend_from_slice(cfg.predecessors(block));
        }
    }
    let load = |inst| func.inst(inst).kind == InstKind::Load(object, PtrLocation::Memory);
    before_alloc
        .iter()
        .copied()
        .find(|&inst| load(inst))
        .or_else(|| {
            earlier
                .iter()
                .filter(|&block| cfg.dominators().dominates(block, home))
                .flat_map(|block| func.blocks[block].instructions.iter().copied())
                .find(|&inst| load(inst))
        })
        .and_then(|inst| func.inst_result_value(inst))
}

/// Whether `size` is `(len + 63) & ~31`, the bytes `new bytes(len)` reserves: the length word
/// and `len` bytes padded to whole words. `len` may itself come padded to whole words, as a
/// buffer allocated in words and cut to its length by a trusted `mstore(b, len)` allocates.
fn bytes_size(func: &Function, size: ValueId, len: ValueId) -> bool {
    let words = |value| {
        let Some(&InstKind::And(a, b)) = func.value_inst_kind(value) else { return None };
        let mask = |mask| func.value_u256(mask) == Some(!U256::from(31));
        if mask(b) { Some(a) } else { mask(a).then_some(b) }
    };
    let plus = |value, constant| {
        let Some(&InstKind::Add(x, y)) = func.value_inst_kind(value) else { return None };
        if func.value_u64(y) == Some(constant) {
            Some(x)
        } else {
            (func.value_u64(x) == Some(constant)).then_some(y)
        }
    };
    let Some(padded) = words(size).and_then(|sum| plus(sum, 63)) else { return false };
    same_value(func, padded, len, 2)
        || words(padded).and_then(|sum| plus(sum, 31)).is_some_and(|x| same_value(func, x, len, 2))
}

/// Whether `address` is `object + 32 + len` in either association, the end of the object's
/// first `len` bytes, with `len` itself or the object's length word.
fn payload_end(func: &Function, address: ValueId, object: ValueId, len: ValueId) -> bool {
    let add = |value| match func.value_inst_kind(value) {
        Some(&InstKind::Add(a, b)) => Some((a, b)),
        _ => None,
    };
    let base = |value| func.value_inst_kind(value) == Some(&InstKind::PtrToInt(object, 256));
    let length = |value| same_value(func, value, len, 2) || is_length_load(func, value, object);
    let Some((a, b)) = add(address) else { return false };
    [(a, b), (b, a)].into_iter().any(|(inner, outer)| {
        add(inner).is_some_and(|(c, d)| {
            [(c, d), (d, c)].into_iter().any(|(pointer, other)| {
                base(pointer)
                    && ((func.value_u64(other) == Some(32) && length(outer))
                        || (func.value_u64(outer) == Some(32) && length(other)))
            })
        })
    })
}

/// Whether `a` and `b` hold the same value wherever both are defined: one SSA value, one
/// constant, or the same arithmetic of operands that are, as the two spellings of `end - start`
/// that checked arithmetic leaves apart until it is lowered. Looks `depth` operations deep.
fn same_value(func: &Function, a: ValueId, b: ValueId, depth: u32) -> bool {
    if a == b || func.value_u256(a).is_some_and(|value| func.value_u256(b) == Some(value)) {
        return true;
    }
    let (Some(x), Some(y)) = (func.value_inst_kind(a), func.value_inst_kind(b)) else {
        return false;
    };
    let Some(depth) = depth.checked_sub(1) else { return false };
    let same = |a, b| same_value(func, a, b, depth);
    match (x, y) {
        (
            &InstKind::CheckedBinary { op, arithmetic, lhs, rhs },
            &InstKind::CheckedBinary { op: op2, arithmetic: arithmetic2, lhs: lhs2, rhs: rhs2 },
        ) => op == op2 && arithmetic == arithmetic2 && same(lhs, lhs2) && same(rhs, rhs2),
        (&InstKind::Add(a1, b1), &InstKind::Add(a2, b2))
        | (&InstKind::Sub(a1, b1), &InstKind::Sub(a2, b2)) => same(a1, a2) && same(b1, b2),
        _ => false,
    }
}

fn rewrite(func: &mut Function, plan: Plan, target: Target) {
    let Plan { lp, shape, fresh } = plan;
    if let Some(fresh) = &fresh
        && let InstKind::Alloc { semantics, .. } = &mut func.inst_mut(fresh.alloc).kind
    {
        // The copy writes the bytes the allocation would clear; each path below clears the rest.
        semantics.initialization = AllocationInitialization::Uninitialized;
    }
    let mut replacements = FxHashMap::default();
    // The counter leaves the loop equal to its limit: the bound counting up, zero counting down.
    replacements.insert(lp.counter, lp.limit);
    let literal = lp.literal_bound(func);
    // A gather that skips an empty count keeps its result phi, now joining the skip and the setup.
    let mut kept_phi = None;
    let mut builder = FunctionBuilder::new_semantic(func).with_bit_tests(target);
    builder.switch_to_block(lp.preheader);
    match shape {
        Shape::Gather { access, width, result } => {
            let calldata = matches!(access.buffer, Buffer::Calldata(_));
            let wraps = matches!(access.buffer, Buffer::Calldata(slice)
                if !decoded_argument(builder.func(), slice));
            let skips = literal.is_none() && !calldata;
            if skips {
                skip_when_empty(&mut builder, &lp);
            }
            if wraps {
                // branch (calldata range wraps), header, fast
                let wraps = calldata_wraps(&mut builder, &access, lp.bound);
                let fast = builder.create_block();
                builder.branch(wraps, lp.header, fast);
                builder.switch_to_block(fast);
            }
            // nonzero = ne bound, 0, for a dynamic bound the setup does not skip
            // panic_if (bound != 0 && range does not fit), 0x32
            // word = load (buffer data + offset); value = and word, top(min(bound, width))
            let nonzero = (literal.is_none() && !skips).then(|| builder.ne_zero(lp.bound));
            let fails = range_fails(&mut builder, &access, lp.bound, nonzero);
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let word = load_word(&mut builder, &access);
            let mask = leading_mask(&mut builder, lp.bound, literal, width);
            let value = match mask {
                Some(mask) => builder.and(word, mask),
                None => word,
            };
            if wraps {
                // fast: ...; jump join
                // join: result' = phi [header: result, fast: value]; jump exit
                let (join, gathered) = join_gather(&mut builder, &lp, result, value);
                replacements.insert(result, gathered);
                replace_after_loop(func, &lp, Some(join), &replacements);
                return;
            }
            if skips {
                // setup: ...; jump header
                // header: result = phi [preheader: 0, setup: value]; jump exit
                let setup = builder.current_block();
                builder.jump(lp.header);
                let (_, phi) = *lp
                    .phis
                    .iter()
                    .find(|&&(value, _)| value == result)
                    .expect("gather result phi");
                if let InstKind::Phi(incoming) = &mut builder.func_mut().inst_mut(phi).kind {
                    for (block, input) in incoming.iter_mut() {
                        if *block != lp.preheader {
                            *block = setup;
                            *input = value;
                        }
                    }
                }
                kept_phi = Some(phi);
            } else {
                replacements.insert(result, value);
            }
        }
        Shape::Scatter { access, stored, width } => {
            // fill: value = mul (byte position, value | and value, 0xff), 0x0101...01
            // [branch (eq bound, 0), exit|header, setup]
            // [branch (memory offset wraps), header, fast]
            // setup: panic_if (range does not fit || bound > width), 0x32
            // mstore address, value
            // or, below a word: mstore address, or (and (mload address), ~mask), (and value, mask)
            // [jump exit|header]
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
            let guarded = has_memory_offset(builder.func(), &access);
            if literal.is_none() {
                if guarded {
                    // empty = eq bound, 0; branch empty, exit, setup
                    let empty = builder.eq_zero(lp.bound);
                    let setup = builder.create_block();
                    builder.branch(empty, lp.exit, setup);
                    builder.switch_to_block(setup);
                    enter_from(builder.func_mut(), lp.header, lp.preheader, setup);
                } else {
                    skip_when_empty(&mut builder, &lp);
                }
            }
            let address = guarded.then(|| {
                // address = ptradd (ptradd object, offset), 32
                // branch (lt (ptrtoint address), (ptrtoint (ptradd object, 32))), header, fast
                let address = memory_data_address(&mut builder, &access);
                let word = builder.cast(address, MirType::I256);
                let wraps = memory_offset_wraps(&mut builder, &access, word)
                    .expect("scatter has a nonzero memory offset");
                let fast = builder.create_block();
                builder.branch(wraps, lp.header, fast);
                builder.switch_to_block(fast);
                address
            });
            let mut fails = range_fails(&mut builder, &access, lp.bound, None);
            if literal.is_none() {
                // A dynamic bound always comes with the value's index check.
                let wide = builder.gt_literal(lp.bound, width, ConditionUse::VALUE);
                fails = builder.or(fails, wide);
            }
            builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            let address = address.unwrap_or_else(|| memory_data_address(&mut builder, &access));
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
            if guarded {
                // fast: ...; jump exit
                builder.jump(lp.exit);
                replace_after_loop(func, &lp, None, &replacements);
                return;
            }
            if literal.is_none() {
                builder.jump(lp.header);
            }
        }
        Shape::Copy { src, dst, dst_checked, overlap } => {
            // A zero count must skip new length loads even when the target prefers no branch:
            // masking the panic does not prevent an otherwise unused load from exhausting gas.
            let loads_length = |access: &Access| {
                matches!(access.buffer, Buffer::Memory(_)) && access.len.is_none()
            };
            let needs_empty_test =
                loads_length(&src) || (dst_checked && fresh.is_none() && loads_length(&dst));
            let tested = literal.is_none_or(|count| count == 0)
                && (needs_empty_test || target.tests_empty_copies());
            if tested {
                // empty = eq bound, 0
                // branch empty, skip, setup
                let empty = builder.eq_zero(lp.bound);
                let setup = builder.create_block();
                // The loop's exit has no phis for a guarded copy, and the header of an
                // unguarded one becomes a jump to it.
                let skip = match overlap {
                    Overlap::Disjoint => lp.header,
                    Overlap::Guarded => lp.exit,
                };
                builder.branch(empty, skip, setup);
                builder.switch_to_block(setup);
                // A guarded copy still enters the loop, now from the setup.
                enter_from(builder.func_mut(), lp.header, lp.preheader, setup);
            }
            // size = shl bound, shift
            let size = scaled(&mut builder, lp.bound, src.shift);
            if let (Some(fresh), Overlap::Guarded, Buffer::Memory(_)) =
                (&fresh, &overlap, src.buffer)
            {
                fresh_guarded_copy(&mut builder, &lp, &src, &dst, fresh, tested);
                replace_after_loop(func, &lp, None, &replacements);
                return;
            }
            let fast = match overlap {
                Overlap::Disjoint => None,
                Overlap::Guarded => {
                    // branch (keeps loop), header, fast
                    let ahead = keeps_loop(&mut builder, &lp, &src, &dst, size);
                    let fast = builder.create_block();
                    builder.branch(ahead, lp.header, fast);
                    builder.switch_to_block(fast);
                    Some(fast)
                }
            };
            if lp.descending {
                // The first iteration indexes `offset + bound - 1`, the highest of all, so it
                // fails exactly when the loop does, with its own order of panics: the source
                // index's addition, then its bounds check, then the destination's. An unchecked
                // addition that wraps fails instead at the bounds check of the iteration whose
                // index wraps to `2^256 - 1`, which no `Panic(0x11)` precedes, as `copy` checked.
                // A tested bound is not zero here.
                let nonzero = (!tested).then(|| builder.ne_zero(lp.bound));
                for access in [Some(&src), dst_checked.then_some(&dst)].into_iter().flatten() {
                    let len = buffer_len(&mut builder, access);
                    let mut past = match access.offset {
                        Some(offset) => {
                            // end = add offset, bound; wraps = lt end, offset
                            let end = builder.add(offset, lp.bound);
                            let wraps = builder.lt(end, offset);
                            if access.wrapping {
                                // past = or wraps, (gt end, len)
                                let past = builder.gt(end, len);
                                builder.or(wraps, past)
                            } else {
                                // panic_if (bound != 0 && wraps), 0x11; past = gt end, len
                                let wraps = match nonzero {
                                    Some(nonzero) => builder.and(wraps, nonzero),
                                    None => wraps,
                                };
                                builder.panic_if(wraps, PanicCode::ArithmeticOverflowUnderflow);
                                builder.gt(end, len)
                            }
                        }
                        // past = gt bound, len
                        None => builder.gt(lp.bound, len),
                    };
                    // panic_if (bound != 0 && past), 0x32
                    if let Some(nonzero) = nonzero {
                        past = builder.and(past, nonzero);
                    }
                    builder.panic_if(past, PanicCode::ArrayOutOfBounds);
                }
            } else {
                // panic_if (bound != 0 && (src range does not fit || dst range does not fit)), 0x32
                // A fresh destination's length is the bound, so its range always fits.
                let mut fails = range_fails(&mut builder, &src, lp.bound, None);
                if dst_checked && fresh.is_none() {
                    let dst_fails = range_fails(&mut builder, &dst, lp.bound, None);
                    fails = builder.or(fails, dst_fails);
                }
                if !tested && literal.is_none_or(|bound| bound == 0) {
                    let nonzero = builder.ne_zero(lp.bound);
                    fails = builder.and(fails, nonzero);
                }
                builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
            }
            if let Some(fresh) = &fresh {
                clear_padding(&mut builder, &dst, fresh);
            }
            // mcopy|calldatacopy dst data + offset, src data + offset, bound
            let to = memory_data_address(&mut builder, &dst);
            let (from, location) = match src.buffer {
                Buffer::Memory(_) => (memory_data_address(&mut builder, &src), PtrLocation::Memory),
                Buffer::Calldata(slice) => {
                    let offset = src.offset.map(|offset| scaled(&mut builder, offset, src.shift));
                    let from = calldata_data_address(&mut builder, slice, offset);
                    (from, PtrLocation::Calldata)
                }
            };
            builder.copy(to, from, size, location);
            if fast.is_some() {
                // fast: ...; jump exit
                // The loop stays for a destination that leads into its source, or a calldata range
                // that wraps.
                builder.jump(lp.exit);
                replace_after_loop(func, &lp, None, &replacements);
                return;
            }
            if tested {
                // setup: ...; jump header
                builder.jump(lp.header);
            }
        }
    }
    func.replace_uses(&replacements);
    // header: jump exit
    fold_terminator_to_jump(func, lp.header, lp.exit);
    for block in lp.body {
        let _ = invalidate_unreachable_block(func, block);
    }
    func.blocks[lp.header].instructions.retain(|&inst| Some(inst) == kept_phi);
}

/// Emits a copy into a fresh destination whose source may overlap the allocation. The fast path
/// copies into the unzeroed buffer when the source the loop reads, its length word and its bytes
/// up to the copied range's end, stays off the whole allocation, whose bytes the loop would read
/// as zeros. When the source's length read before the allocation proves its range check, the
/// fast path drops the check and the test also keeps the source off the free memory pointer,
/// below the zero slot. Otherwise the zeroing runs late, and the copy goes as before: the loop
/// for a destination that leads into its source or meets its length word, and `mcopy` for any
/// other. A source offset that wraps below the source's data start takes the slow path too, so
/// the buffer is zeroed before the loop reads it.
///
/// ```text
/// start = ptrtoint src; base = ptrtoint dst; limit = ptrtoint (ptradd dst, size)
/// overlaps = (start < limit && base < from + bound) || source_offset_wraps [|| start < 0x60]
/// branch overlaps, slow, fast
/// fast:  [panic_if src range does not fit, 0x32]; [mstore (ptradd dst, size - 32), 0]
///        mcopy to, from, bound; jump exit
/// slow:  memory_zero to, size - 32; branch (keeps loop), header, again
/// again: panic_if src range does not fit, 0x32; mcopy to, from, bound; jump exit
/// ```
///
/// The loop's header keeps a branch for its entry, so no loop pass takes the slow path's loop for
/// a preheader-shaped one to version or rewrite again.
fn fresh_guarded_copy(
    builder: &mut FunctionBuilder<'_>,
    lp: &Counted,
    src: &Access,
    dst: &Access,
    fresh: &FreshDestination,
    tested: bool,
) {
    let (Buffer::Memory(source), Buffer::Memory(object)) = (src.buffer, dst.buffer) else {
        unreachable!("a guarded copy reads and writes memory")
    };
    let entry = builder.current_block();
    let from = memory_data_address(builder, src);
    let data = memory_data_address(builder, dst);
    let from = builder.cast(from, MirType::I256);
    let end = builder.add(from, lp.bound);
    let start = builder.cast(source, MirType::I256);
    let base = builder.cast(object, MirType::I256);
    let limit = builder.ptradd(object, fresh.size);
    let limit = builder.cast(limit, MirType::I256);
    let below = builder.lt(start, limit);
    let inside = builder.lt(base, end);
    let mut overlaps = builder.and(below, inside);
    if let Some(wraps) = memory_offset_wraps(builder, src, from) {
        // overlaps = or overlaps, wraps
        overlaps = builder.or(overlaps, wraps);
    }
    if fresh.source_fits {
        let zero_slot = builder.imm(EvmMemoryLayout::ZERO_SLOT);
        let reserved = builder.lt(start, zero_slot);
        overlaps = builder.or(overlaps, reserved);
    }
    let fast = builder.create_block();
    let slow = builder.create_block();
    builder.branch(overlaps, slow, fast);
    let source_check = |builder: &mut FunctionBuilder<'_>| {
        // panic_if (bound != 0 && src range does not fit), 0x32
        let nonzero = (!tested).then(|| builder.ne_zero(lp.bound));
        let fails = range_fails(builder, src, lp.bound, nonzero);
        builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
    };
    builder.switch_to_block(fast);
    if !fresh.source_fits {
        source_check(builder);
    }
    clear_padding(builder, dst, fresh);
    builder.copy(data, from, lp.bound, PtrLocation::Memory);
    builder.jump(lp.exit);
    builder.switch_to_block(slow);
    let header = builder.imm(EvmMemoryLayout::WORD_SIZE);
    let bytes = builder.sub(fresh.size, header);
    builder.memory_zero(data, bytes);
    // branch (keeps loop), header, again
    let ahead = keeps_loop(builder, lp, src, dst, lp.bound);
    // The fast path's block stays its own: entering it from here too costs its stack layout.
    let again = builder.create_block();
    builder.branch(ahead, lp.header, again);
    enter_from(builder.func_mut(), lp.header, entry, slow);
    builder.switch_to_block(again);
    source_check(builder);
    builder.copy(data, from, lp.bound, PtrLocation::Memory);
    builder.jump(lp.exit);
}

/// Emits whether a guarded copy runs its loop, where the copy would differ from it: a calldata
/// range that wraps, a word count whose byte size truncates, or a memory destination that leads
/// into its source. The loop reloads the source's length after every store, unless it compares
/// with a length from before the loop, while the copy reads it once. For a source other than
/// the destination, the loop also stays for a destination that meets the source's length word.
/// The span from that word to the end of the source range catches overlaps in either direction
/// only when the source's effective address stays above its data start. Wrapped source and
/// destination starts keep the loop, including copies within the same object: a wrapped store
/// can overwrite its own length word and change the next iteration's bounds check.
fn keeps_loop(
    builder: &mut FunctionBuilder<'_>,
    lp: &Counted,
    src: &Access,
    dst: &Access,
    size: ValueId,
) -> ValueId {
    if let Buffer::Calldata(slice) = src.buffer {
        // wraps = decoded ? false : calldata_wraps
        // to = ptrtoint (dst data + offset)
        // wraps = or wraps, destination_wraps
        let mut wraps = if decoded_argument(builder.func(), slice) {
            builder.imm_bool(false)
        } else {
            calldata_wraps(builder, src, lp.bound)
        };
        let to = memory_data_address(builder, dst);
        let to = builder.cast(to, MirType::I256);
        if let Some(destination_wraps) = memory_offset_wraps(builder, dst, to) {
            // wraps = or wraps, destination_wraps
            wraps = builder.or(wraps, destination_wraps);
        }
        return wraps;
    }
    // from = src data + offset; to = dst data + offset
    let from = memory_data_address(builder, src);
    let to = memory_data_address(builder, dst);
    let from = builder.cast(from, MirType::I256);
    let to = builder.cast(to, MirType::I256);
    let mut overlaps = if let Buffer::Memory(object) = src.buffer
        && src.len.is_none()
        && src.buffer != dst.buffer
    {
        // start = ptrtoint src
        // ahead = to < from + size && start < to + size
        let start = builder.cast(object, MirType::I256);
        let end = builder.add(from, size);
        let last = builder.add(to, size);
        let below = builder.lt(to, end);
        let above = builder.lt(start, last);
        builder.and(below, above)
    } else {
        // A downward copy goes wrong the other way round, when its source starts inside the
        // destination range:
        // behind = to < from && from < to + size
        let (lead, trail) = if lp.descending { (to, from) } else { (from, to) };
        let end = builder.add(lead, size);
        let after = builder.lt(lead, trail);
        let inside = builder.lt(trail, end);
        builder.and(after, inside)
    };
    for (access, address) in [(src, from), (dst, to)] {
        if let Some(wraps) = memory_offset_wraps(builder, access, address) {
            // overlaps = or overlaps, wraps
            overlaps = builder.or(overlaps, wraps);
        }
    }
    if src.shift == 0 {
        return overlaps;
    }
    // scale_wraps = ne (shr bound, 256 - shift), 0
    // keeps_loop = or overlaps, scale_wraps
    let scale_wraps =
        builder.exceeds_bits(lp.bound, 256 - u32::from(src.shift), ConditionUse::VALUE);
    builder.or(overlaps, scale_wraps)
}

/// Unless a zero word stored at the end of the copied bytes clears it, clears the padding of a
/// fresh destination, the rest of its allocation's last word, before a copy of its bytes writes
/// the bytes before it: `mstore (ptradd dst, size - 32), 0`.
fn clear_padding(builder: &mut FunctionBuilder<'_>, dst: &Access, fresh: &FreshDestination) {
    let Buffer::Memory(object) = dst.buffer else { return };
    if fresh.padded {
        return;
    }
    let word = builder.imm(EvmMemoryLayout::WORD_SIZE);
    let offset = builder.sub(fresh.size, word);
    let last = builder.ptradd(object, offset);
    let zero = builder.imm(0);
    builder.store(last, zero);
}

/// Emits whether `count` bytes from the access's offset do not fit in its buffer:
/// `count > len || offset > len - count`, and with `nonzero`, the test `count != 0`, only when
/// it holds.
fn range_fails(
    builder: &mut FunctionBuilder<'_>,
    access: &Access,
    count: ValueId,
    nonzero: Option<ValueId>,
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
    if let Some(nonzero) = nonzero {
        // fails = and fails, nonzero
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

/// Emits `object + (offset << shift) + 32`, the address of a memory access's first byte.
fn memory_data_address(builder: &mut FunctionBuilder<'_>, access: &Access) -> ValueId {
    let (Buffer::Memory(object) | Buffer::Calldata(object)) = access.buffer;
    // base = ptradd object, (shl offset, shift); address = ptradd base, 32
    let base = match access.offset {
        Some(offset) => {
            let offset = scaled(builder, offset, access.shift);
            builder.ptradd(object, offset)
        }
        None => object,
    };
    let header = builder.imm(32);
    builder.ptradd(base, header)
}

/// Emits `count << shift`, the bytes of `count` elements.
fn scaled(builder: &mut FunctionBuilder<'_>, count: ValueId, shift: u8) -> ValueId {
    if shift == 0 {
        return count;
    }
    // bytes = shl count, shift
    let shift = builder.imm(u64::from(shift));
    builder.shl(count, shift)
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
    let short = builder.lt_literal(count, width, ConditionUse::VALUE);
    let bytes = builder.select(short, count, limit);
    let three = builder.imm(3);
    let bits = builder.shl(bytes, three);
    let all = builder.imm(U256::MAX);
    let low = builder.shr(all, bits);
    Some(builder.not(low))
}

/// Branches past the replacement of a gather or scatter of a dynamic count over memory for a
/// count of zero, which runs no iteration of the loop and so touches no memory: the buffer may
/// then lie anywhere, even where one word would expand memory until the call runs out of gas. The
/// replacement continues in a block of its own, which jumps on to the loop's header as the skip
/// does.
fn skip_when_empty(builder: &mut FunctionBuilder<'_>, lp: &Counted) {
    // empty = eq bound, 0; branch empty, header, setup
    let empty = builder.eq_zero(lp.bound);
    let setup = builder.create_block();
    builder.branch(empty, lp.header, setup);
    builder.switch_to_block(setup);
}

/// Emits whether a calldata access needs its original loop: its addresses wrap past `2^256`,
/// or scaling its word indices could overflow. The loop adds each byte offset to the slice's
/// offset unchecked, but checks the index shift first. A slice forged by assembly can exercise
/// either case. Testing the exclusive index end conservatively keeps the loop when its final
/// index is exactly the largest scalable word index too.
fn calldata_wraps(builder: &mut FunctionBuilder<'_>, access: &Access, count: ValueId) -> ValueId {
    let Buffer::Calldata(slice) = access.buffer else { unreachable!("a calldata access") };
    // start = ptrtoint slice.ptr + (offset << shift); end = add start, (count << shift)
    // wraps = lt end, start
    let offset = access.offset.map(|offset| scaled(builder, offset, access.shift));
    let start = calldata_data_address(builder, slice, offset);
    let size = scaled(builder, count, access.shift);
    let end = builder.add(start, size);
    let mut wraps = builder.lt(end, start);
    if access.shift != 0 {
        // index_end = add offset, count
        // index_wraps = lt index_end, offset
        // scale_wraps = ne (shr index_end, 256 - shift), 0
        // wraps = or wraps, (or index_wraps, scale_wraps)
        let index_start = access.offset.unwrap_or_else(|| builder.imm(0));
        let index_end = builder.add(index_start, count);
        let index_wraps = builder.lt(index_end, index_start);
        let scale_wraps =
            builder.exceeds_bits(index_end, 256 - u32::from(access.shift), ConditionUse::VALUE);
        let index_fails = builder.or(index_wraps, scale_wraps);
        wraps = builder.or(wraps, index_fails);
    }
    wraps
}

/// Whether `slice` is an argument of an external function, which ABI decoding checked to end
/// inside calldata, so that its addresses cannot wrap. A public function also takes its arguments
/// from internal calls, which can pass a slice whose offset inline assembly set.
fn decoded_argument(func: &Function, slice: ValueId) -> bool {
    matches!(func.value(slice), Value::Arg(_))
        && func.selector.is_some()
        && func.attributes.visibility == solar_ast::Visibility::External
}

/// Joins the fast path of a calldata gather, ending in `value`, with the loop it keeps for a range
/// that wraps: both reach the loop's exit through a new block, returned with the gathered value it
/// picks, which every use of the loop's `result` after the loop reads instead.
fn join_gather(
    builder: &mut FunctionBuilder<'_>,
    lp: &Counted,
    result: ValueId,
    value: ValueId,
) -> (BlockId, ValueId) {
    let fast = builder.current_block();
    let join = builder.create_block();
    // fast: ...; jump join
    // join: gathered = phi [header: result, fast: value]; jump exit
    builder.jump(join);
    builder.switch_to_block(join);
    let gathered = builder.phi(vec![(lp.header, result), (fast, value)]);
    builder.jump(lp.exit);
    let func = builder.func_mut();
    // header: branch test, body, join
    if let Some(Terminator::Branch { else_block, .. }) = &mut func.blocks[lp.header].terminator {
        *else_block = join;
    }
    enter_from(func, lp.exit, lp.header, join);
    (join, gathered)
}

/// Replaces the uses of values after a loop that a fast path joins, other than in `skip`: the
/// counter leaves the loop equal to its limit on every way out, and a gather's result is the value
/// the join picks.
fn replace_after_loop(
    func: &mut Function,
    lp: &Counted,
    skip: Option<BlockId>,
    replacements: &FxHashMap<ValueId, ValueId>,
) {
    for block in func.blocks.indices() {
        if block == lp.header || lp.body.contains(&block) || Some(block) == skip {
            continue;
        }
        for inst in func.blocks[block].instructions.clone() {
            replace_inst_uses(func.inst_mut(inst), replacements);
        }
        if let Some(terminator) = &mut func.blocks[block].terminator {
            replace_terminator_uses(terminator, replacements);
        }
    }
}

/// Renames the entry edge `from` of the loop `header` to `to` in its phis, for a `to` that
/// enters the loop in place of `from`.
fn enter_from(func: &mut Function, header: BlockId, from: BlockId, to: BlockId) {
    for index in 0..func.blocks[header].instructions.len() {
        let inst = func.blocks[header].instructions[index];
        if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
            for (block, _) in incoming.iter_mut() {
                if *block == from {
                    *block = to;
                }
            }
        }
    }
}

/// Tests whether a memory access's offset moves its effective address below its object's data
/// start. The caller keeps the loop then: the header-to-data ordering and stable length words
/// that bulk stores and copies rely on no longer hold. An absent or zero offset needs no test.
fn memory_offset_wraps(
    builder: &mut FunctionBuilder<'_>,
    access: &Access,
    address: ValueId,
) -> Option<ValueId> {
    let Buffer::Memory(object) = access.buffer else { return None };
    if !has_memory_offset(builder.func(), access) {
        return None;
    }
    // data = ptrtoint (ptradd object, 32)
    // wraps = lt address, data
    let header = builder.imm(EvmMemoryLayout::WORD_SIZE);
    let data = builder.ptradd(object, header);
    let data = builder.cast(data, MirType::I256);
    Some(builder.lt(address, data))
}

/// Whether a memory access has an offset that may wrap below its object's data start.
fn has_memory_offset(func: &Function, access: &Access) -> bool {
    matches!(access.buffer, Buffer::Memory(_))
        && access.offset.is_some_and(|offset| func.value_u64(offset) != Some(0))
}
