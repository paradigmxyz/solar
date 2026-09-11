//! Structural lowering for profitable byte-loop idioms.
//!
//! This pass recognizes complete canonical loop shapes after semantic memory
//! objects have been lowered to physical addresses. It currently replaces a
//! read-only byte-by-byte 7-bit ASCII predicate with a 32-byte reduction and an
//! exact memory or calldata zero-byte counter with a per-lane word reduction.
//! The matchers require complete canonical loop shapes, standard byte-address
//! calculations, exact bounds and exits, and no additional effects or uses.
//!
//! Full chunks stay within the source payload. A final load may include only
//! the rounded allocation padding; the transform shifts or forces those bytes
//! to nonzero before reducing them, so uninitialized padding cannot affect the
//! result. The original byte loop remains unreachable and later CFG cleanup
//! removes it. This pass runs after memory-object lowering and before stack
//! scheduling. It is enabled only for gas optimization because the word setup
//! grows code.

use crate::mir::{
    BlockId, Function, FunctionBuilder, InstKind, Module, Terminator, ValueId,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
    utils::repair_reachability_phis,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

/// Rewrites canonical memory loops to target-efficient word operations.
pub(crate) struct LoopIdioms;

impl MirPass for LoopIdioms {
    fn name(&self) -> &'static str {
        "loop-idioms"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run_function(func))
    }
}

#[derive(Clone, Copy)]
struct AsciiLoop {
    preheader: BlockId,
    accept: BlockId,
    reject: BlockId,
    object: ValueId,
}

fn run_function(func: &mut Function) -> bool {
    let mut changed = false;
    loop {
        if let Some(candidate) =
            func.blocks.indices().find_map(|header| match_ascii_loop(func, header))
        {
            rewrite_ascii_loop(func, candidate);
        } else if let Some(candidate) =
            func.blocks.indices().find_map(|header| match_zero_count_loop(func, header))
        {
            rewrite_zero_count_loop(func, candidate);
        } else if let Some(candidate) =
            func.blocks.indices().find_map(|header| match_calldata_zero_count_loop(func, header))
        {
            rewrite_zero_count_loop(func, candidate);
        } else {
            break;
        }
        changed = true;
        let _ = repair_reachability_phis(func);
    }
    changed
}

fn match_ascii_loop(func: &Function, header: BlockId) -> Option<AsciiLoop> {
    macro_rules! reject {
        ($reason:literal) => {{ return None }};
    }
    let [phi_inst, len_inst, less_inst] = func.blocks[header].instructions.as_slice() else {
        reject!("header shape");
    };
    let InstKind::Phi(incoming) = &func.inst(*phi_inst).kind else { reject!("phi") };
    let index = func.inst_result_value(*phi_inst)?;
    let [(preheader, initial), (latch, next)] = incoming.as_slice() else { reject!("incoming") };
    if func.value_u64(*initial) != Some(0)
        || !func.blocks[*preheader].instructions.is_empty()
        || !matches!(func.blocks[*preheader].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        reject!("preheader");
    }

    let InstKind::MLoad(object) = func.inst(*len_inst).kind else { reject!("length") };
    let length = func.inst_result_value(*len_inst)?;
    let InstKind::Lt(lhs, rhs) = func.inst(*less_inst).kind else { reject!("less") };
    if lhs != index || rhs != length {
        reject!("less operands");
    }
    let condition = func.inst_result_value(*less_inst)?;
    let Terminator::Branch { condition: branch_condition, then_block: body, else_block: accept } =
        func.blocks[header].terminator.as_ref()?
    else {
        reject!("header terminator");
    };
    if *branch_condition != condition || !returns_bool(func, *accept, true) {
        reject!("accept");
    }

    let [base_inst, ptr_inst, load_inst, byte_inst, rest @ ..] =
        func.blocks[*body].instructions.as_slice()
    else {
        reject!("body shape");
    };
    let InstKind::Add(base_object, data_offset) = func.inst(*base_inst).kind else {
        reject!("base")
    };
    if base_object != object || func.value_u64(data_offset) != Some(32) {
        reject!("base operands");
    }
    let base = func.inst_result_value(*base_inst)?;
    let InstKind::Add(ptr_base, ptr_index) = func.inst(*ptr_inst).kind else { reject!("pointer") };
    if ptr_base != base || ptr_index != index {
        reject!("pointer operands");
    }
    let ptr = func.inst_result_value(*ptr_inst)?;
    let InstKind::MLoad(load_ptr) = func.inst(*load_inst).kind else { reject!("load") };
    if load_ptr != ptr {
        reject!("load operand");
    }
    let word = func.inst_result_value(*load_inst)?;
    let InstKind::Byte(byte_index, byte_word) = func.inst(*byte_inst).kind else { reject!("byte") };
    if func.value_u64(byte_index) != Some(0) || byte_word != word {
        reject!("byte operands");
    }
    let byte = func.inst_result_value(*byte_inst)?;
    let rejected = match rest {
        [reject_inst] => {
            let InstKind::Gt(reject_byte, limit) = func.inst(*reject_inst).kind else {
                reject!("direct gt");
            };
            if reject_byte != byte || func.value_u64(limit) != Some(127) {
                reject!("direct gt operands");
            }
            func.inst_result_value(*reject_inst)?
        }
        [align_inst, reject_inst] => {
            let InstKind::Shl(shift, align_byte) = func.inst(*align_inst).kind else {
                reject!("align");
            };
            let aligned = func.inst_result_value(*align_inst)?;
            let InstKind::Gt(reject_byte, limit) = func.inst(*reject_inst).kind else {
                reject!("aligned gt");
            };
            let direct = reject_byte == byte && func.value_u64(limit) == Some(127);
            let aligned_compare =
                reject_byte == aligned && func.value_u256(limit) == Some(U256::from(127) << 248);
            if func.value_u64(shift) != Some(248)
                || align_byte != byte
                || (!direct && !aligned_compare)
            {
                reject!("aligned gt operands");
            }
            func.inst_result_value(*reject_inst)?
        }
        _ => reject!("body tail"),
    };
    let Terminator::Branch {
        condition: reject_condition,
        then_block: reject,
        else_block: branch_latch,
    } = func.blocks[*body].terminator.as_ref()?
    else {
        reject!("body terminator");
    };
    if *reject_condition != rejected
        || *branch_latch != *latch
        || !returns_bool(func, *reject, false)
    {
        reject!("body targets");
    }

    let [next_inst] = func.blocks[*latch].instructions.as_slice() else { reject!("latch shape") };
    let InstKind::Add(next_index, step) = func.inst(*next_inst).kind else { reject!("latch add") };
    if next_index != index
        || func.value_u64(step) != Some(1)
        || func.inst_result_value(*next_inst)? != *next
        || !matches!(func.blocks[*latch].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        reject!("latch operands");
    }

    Some(AsciiLoop { preheader: *preheader, accept: *accept, reject: *reject, object })
}

fn returns_bool(func: &Function, block: BlockId, expected: bool) -> bool {
    let Some(Terminator::Return { values }) = &func.blocks[block].terminator else { return false };
    let [value] = values.as_slice() else { return false };
    func.value_u64(*value) == Some(u64::from(expected))
}

#[derive(Clone, Copy)]
struct ZeroCountLoop {
    preheader: BlockId,
    source: ByteSource,
}

#[derive(Clone, Copy)]
enum ByteSource {
    Memory { object: ValueId },
    Calldata { data: ValueId, length: ValueId },
}

fn match_zero_count_loop(func: &Function, header: BlockId) -> Option<ZeroCountLoop> {
    let [first_phi, second_phi, len_inst, less_inst] = func.blocks[header].instructions.as_slice()
    else {
        return None;
    };
    let phis = [*first_phi, *second_phi];
    if !phis.iter().all(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_))) {
        return None;
    }
    let InstKind::MLoad(object) = func.inst(*len_inst).kind else { return None };
    let length = func.inst_result_value(*len_inst)?;
    let InstKind::Lt(index, bound) = func.inst(*less_inst).kind else { return None };
    if bound != length {
        return None;
    }
    let condition = func.inst_result_value(*less_inst)?;
    let Terminator::Branch { condition: branch, then_block: body, else_block: exit } =
        func.blocks[header].terminator.as_ref()?
    else {
        return None;
    };
    if *branch != condition {
        return None;
    }
    let Some(Terminator::Return { values }) = &func.blocks[*exit].terminator else { return None };
    let [count] = values.as_slice() else { return None };
    let count_phi =
        phis.iter().copied().find(|&inst| func.inst_result_value(inst) == Some(*count))?;
    let index_phi =
        phis.iter().copied().find(|&inst| func.inst_result_value(inst) == Some(index))?;
    if count_phi == index_phi {
        return None;
    }
    let count = *count;

    let [base_inst, ptr_inst, load_inst, byte_inst, align_inst] =
        func.blocks[*body].instructions.as_slice()
    else {
        return None;
    };
    let InstKind::Add(base_object, offset) = func.inst(*base_inst).kind else { return None };
    if base_object != object || func.value_u64(offset) != Some(32) {
        return None;
    }
    let base = func.inst_result_value(*base_inst)?;
    let InstKind::Add(ptr_base, ptr_index) = func.inst(*ptr_inst).kind else { return None };
    if ptr_base != base || ptr_index != index {
        return None;
    }
    let pointer = func.inst_result_value(*ptr_inst)?;
    let InstKind::MLoad(load_pointer) = func.inst(*load_inst).kind else { return None };
    if load_pointer != pointer {
        return None;
    }
    let word = func.inst_result_value(*load_inst)?;
    let InstKind::Byte(byte_index, byte_word) = func.inst(*byte_inst).kind else { return None };
    if func.value_u64(byte_index) != Some(0) || byte_word != word {
        return None;
    }
    let byte = func.inst_result_value(*byte_inst)?;
    let InstKind::Shl(shift, align_byte) = func.inst(*align_inst).kind else { return None };
    if func.value_u64(shift) != Some(248) || align_byte != byte {
        return None;
    }
    let aligned = func.inst_result_value(*align_inst)?;
    let Terminator::Branch { condition: nonzero, then_block: latch, else_block: increment } =
        func.blocks[*body].terminator.as_ref()?
    else {
        return None;
    };
    if *nonzero != aligned {
        return None;
    }

    let [increment_inst] = func.blocks[*increment].instructions.as_slice() else { return None };
    let InstKind::Add(increment_count, one) = func.inst(*increment_inst).kind else { return None };
    if increment_count != count || func.value_u64(one) != Some(1) {
        return None;
    }
    let incremented = func.inst_result_value(*increment_inst)?;
    let Terminator::Branch {
        condition: overflow_result,
        then_block: increment_latch,
        else_block: _,
    } = func.blocks[*increment].terminator.as_ref()?
    else {
        return None;
    };
    if *overflow_result != incremented || *increment_latch != *latch {
        return None;
    }

    let [merged_inst, next_inst] = func.blocks[*latch].instructions.as_slice() else { return None };
    let InstKind::Phi(merged_incoming) = &func.inst(*merged_inst).kind else { return None };
    if !merged_incoming.contains(&(*increment, incremented))
        || !merged_incoming.contains(&(*body, count))
    {
        return None;
    }
    let merged_count = func.inst_result_value(*merged_inst)?;
    let InstKind::Add(next_index, one) = func.inst(*next_inst).kind else { return None };
    if next_index != index || func.value_u64(one) != Some(1) {
        return None;
    }
    let next = func.inst_result_value(*next_inst)?;
    if !matches!(func.blocks[*latch].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        return None;
    }

    let InstKind::Phi(count_incoming) = &func.inst(count_phi).kind else { unreachable!() };
    let InstKind::Phi(index_incoming) = &func.inst(index_phi).kind else { unreachable!() };
    let [(preheader, count_initial), (count_latch, count_next)] = count_incoming.as_slice() else {
        return None;
    };
    let [(index_preheader, index_initial), (index_latch, index_next)] = index_incoming.as_slice()
    else {
        return None;
    };
    if preheader != index_preheader
        || count_latch != latch
        || index_latch != latch
        || *count_next != merged_count
        || *index_next != next
        || func.value_u64(*count_initial) != Some(0)
        || func.value_u64(*index_initial) != Some(0)
        || !func.blocks[*preheader].instructions.is_empty()
        || !matches!(func.blocks[*preheader].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        return None;
    }

    Some(ZeroCountLoop { preheader: *preheader, source: ByteSource::Memory { object } })
}

fn match_calldata_zero_count_loop(func: &Function, header: BlockId) -> Option<ZeroCountLoop> {
    let [first_phi, second_phi, less_inst] = func.blocks[header].instructions.as_slice() else {
        return None;
    };
    let phis = [*first_phi, *second_phi];
    if !phis.iter().all(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_))) {
        return None;
    }
    let InstKind::Lt(index, length) = func.inst(*less_inst).kind else { return None };
    let condition = func.inst_result_value(*less_inst)?;
    let Terminator::Branch { condition: branch, then_block: body, else_block: exit } =
        func.blocks[header].terminator.as_ref()?
    else {
        return None;
    };
    if *branch != condition {
        return None;
    }
    let Some(Terminator::Return { values }) = &func.blocks[*exit].terminator else { return None };
    let [count] = values.as_slice() else { return None };
    let count_phi =
        phis.iter().copied().find(|&inst| func.inst_result_value(inst) == Some(*count))?;
    let index_phi =
        phis.iter().copied().find(|&inst| func.inst_result_value(inst) == Some(index))?;
    if count_phi == index_phi {
        return None;
    }
    let count = *count;

    let [ptr_inst, load_inst, byte_inst] = func.blocks[*body].instructions.as_slice() else {
        return None;
    };
    let InstKind::Add(data, ptr_index) = func.inst(*ptr_inst).kind else { return None };
    if ptr_index != index {
        return None;
    }
    let pointer = func.inst_result_value(*ptr_inst)?;
    let InstKind::CalldataLoad(load_pointer) = func.inst(*load_inst).kind else { return None };
    if load_pointer != pointer {
        return None;
    }
    let word = func.inst_result_value(*load_inst)?;
    let InstKind::Byte(byte_index, byte_word) = func.inst(*byte_inst).kind else { return None };
    if func.value_u64(byte_index) != Some(0) || byte_word != word {
        return None;
    }
    let byte = func.inst_result_value(*byte_inst)?;
    let Terminator::Branch { condition: nonzero, then_block: latch, else_block: increment } =
        func.blocks[*body].terminator.as_ref()?
    else {
        return None;
    };
    if *nonzero != byte {
        return None;
    }

    let [increment_inst] = func.blocks[*increment].instructions.as_slice() else { return None };
    let InstKind::Add(increment_count, one) = func.inst(*increment_inst).kind else { return None };
    if increment_count != count || func.value_u64(one) != Some(1) {
        return None;
    }
    let incremented = func.inst_result_value(*increment_inst)?;
    let Terminator::Branch {
        condition: overflow_result,
        then_block: increment_latch,
        else_block: _,
    } = func.blocks[*increment].terminator.as_ref()?
    else {
        return None;
    };
    if *overflow_result != incremented || *increment_latch != *latch {
        return None;
    }

    let [merged_inst, next_inst] = func.blocks[*latch].instructions.as_slice() else { return None };
    let InstKind::Phi(merged_incoming) = &func.inst(*merged_inst).kind else { return None };
    if !merged_incoming.contains(&(*increment, incremented))
        || !merged_incoming.contains(&(*body, count))
    {
        return None;
    }
    let merged_count = func.inst_result_value(*merged_inst)?;
    let InstKind::Add(next_index, one) = func.inst(*next_inst).kind else { return None };
    if next_index != index || func.value_u64(one) != Some(1) {
        return None;
    }
    let next = func.inst_result_value(*next_inst)?;
    if !matches!(func.blocks[*latch].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        return None;
    }

    let InstKind::Phi(count_incoming) = &func.inst(count_phi).kind else { unreachable!() };
    let InstKind::Phi(index_incoming) = &func.inst(index_phi).kind else { unreachable!() };
    let [(preheader, count_initial), (count_latch, count_next)] = count_incoming.as_slice() else {
        return None;
    };
    let [(index_preheader, index_initial), (index_latch, index_next)] = index_incoming.as_slice()
    else {
        return None;
    };
    if preheader != index_preheader
        || count_latch != latch
        || index_latch != latch
        || *count_next != merged_count
        || *index_next != next
        || func.value_u64(*count_initial) != Some(0)
        || func.value_u64(*index_initial) != Some(0)
        || !func.blocks[*preheader].instructions.is_empty()
        || !matches!(func.blocks[*preheader].terminator, Some(Terminator::Jump(target)) if target == header)
    {
        return None;
    }

    Some(ZeroCountLoop { preheader: *preheader, source: ByteSource::Calldata { data, length } })
}

fn rewrite_zero_count_loop(func: &mut Function, candidate: ZeroCountLoop) {
    let short = func.alloc_block();
    let short_nonempty = func.alloc_block();
    let empty_result = func.alloc_block();
    let setup = func.alloc_block();
    let word_header = func.alloc_block();
    let word_exit = func.alloc_block();
    let word_body = func.alloc_block();
    let tail = func.alloc_block();
    let done = func.alloc_block();

    {
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(candidate.preheader);
        let (data, length) = match candidate.source {
            ByteSource::Memory { object } => {
                let length = builder.mload(object);
                let data = builder.add_u64_offset(object, 32);
                (data, length)
            }
            ByteSource::Calldata { data, length } => (data, length),
        };
        let short_limit = builder.imm(33);
        let is_short = builder.lt(length, short_limit);
        builder.branch(is_short, short, setup);

        // full = length & ~31
        // end = data + full
        builder.switch_to_block(setup);
        let full_mask = builder.imm(U256::MAX - U256::from(31));
        let full = builder.and(length, full_mask);
        let end = builder.add(data, full);
        builder.jump(word_header);

        // if length == 0: empty_result else short_nonempty
        builder.switch_to_block(short);
        let zero = builder.imm(0);
        builder.branch(length, short_nonempty, empty_result);

        // return 0
        builder.switch_to_block(empty_result);
        builder.ret([zero]);

        // padding_mask = max >> (length * 8)
        // word = load(data) | padding_mask
        // return count_zero_bytes(word)
        builder.switch_to_block(short_nonempty);
        let byte_bits = builder.imm(8);
        let data_bits = builder.mul(length, byte_bits);
        let all = builder.imm(U256::MAX);
        let padding_mask = builder.shr(data_bits, all);
        let word = match candidate.source {
            ByteSource::Memory { .. } => builder.mload(data),
            ByteSource::Calldata { .. } => builder.calldataload(data),
        };
        let word = builder.or(word, padding_mask);
        let result = emit_zero_byte_count(&mut builder, word);
        builder.ret([result]);

        // pointer = phi(setup: data, word_body: next_pointer)
        // word_count = phi(setup: 0, word_body: count_next)
        // if pointer != end: word_body else word_exit
        builder.switch_to_block(word_header);
        let zero = builder.imm(0);
        let pointer = builder.phi(vec![(setup, data)]);
        let word_count = builder.phi(vec![(setup, zero)]);
        let more = builder.xor(pointer, end);
        builder.branch(more, word_body, word_exit);

        // word = load(pointer)
        // count_next = word_count + count_zero_bytes(word)
        // next_pointer = pointer + 32
        builder.switch_to_block(word_body);
        let word = match candidate.source {
            ByteSource::Memory { .. } => builder.mload(pointer),
            ByteSource::Calldata { .. } => builder.calldataload(pointer),
        };
        let lane_count = emit_zero_byte_count(&mut builder, word);
        let count_next = builder.add(word_count, lane_count);
        let next_pointer = builder.add_u64_offset(pointer, 32);
        builder.jump(word_header);
        builder.add_phi_incoming(pointer, word_body, next_pointer);
        builder.add_phi_incoming(word_count, word_body, count_next);

        // if remainder != 0: tail else done
        builder.switch_to_block(word_exit);
        let remainder_mask = builder.imm(31);
        let has_remainder = builder.and(length, remainder_mask);
        builder.branch(has_remainder, tail, done);

        // data_bits = (length & 31) * 8
        // padding_mask = max >> data_bits
        // tail_word = load(pointer) | padding_mask
        // result = word_count + count_zero_bytes(tail_word)
        // return result
        builder.switch_to_block(tail);
        let remainder_mask = builder.imm(31);
        let remainder = builder.and(length, remainder_mask);
        let byte_bits = builder.imm(8);
        let data_bits = builder.mul(remainder, byte_bits);
        let all_bits = builder.imm(U256::MAX);
        let padding_mask = builder.shr(data_bits, all_bits);
        let word = match candidate.source {
            ByteSource::Memory { .. } => builder.mload(pointer),
            ByteSource::Calldata { .. } => builder.calldataload(pointer),
        };
        let word = builder.or(word, padding_mask);
        let tail_count = emit_zero_byte_count(&mut builder, word);
        let result = builder.add(word_count, tail_count);
        builder.ret([result]);

        // return word_count
        builder.switch_to_block(done);
        builder.ret([word_count]);
    }
}

fn emit_zero_byte_count(builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
    // lanes = ~(word | ((word & 0x7f7f..7f) + 0x7f7f..7f) | 0x7f7f..7f) >> 7
    // return byte(0, lanes * 0x0101..01)
    let lane_mask = builder.imm(U256::MAX / U256::from(255) * U256::from(127));
    let low = builder.and(word, lane_mask);
    let biased = builder.add(low, lane_mask);
    let occupied = builder.or(biased, word);
    let occupied = builder.or(occupied, lane_mask);
    let vacant = builder.not(occupied);
    let seven = builder.imm(7);
    let lanes = builder.shr(seven, vacant);
    let sum_factor = builder.imm(U256::MAX / U256::from(255));
    let product = builder.mul(lanes, sum_factor);
    let first = builder.imm(0);
    builder.byte(first, product)
}

fn rewrite_ascii_loop(func: &mut Function, candidate: AsciiLoop) {
    let small_check = func.alloc_block();
    let one_word = func.alloc_block();
    let two_check = func.alloc_block();
    let two_words = func.alloc_block();
    let setup = func.alloc_block();
    let pair_header = func.alloc_block();
    let pair_exit = func.alloc_block();
    let pair_body = func.alloc_block();
    let tail = func.alloc_block();
    let single_body = func.alloc_block();
    let finish = func.alloc_block();

    {
        let mut builder = FunctionBuilder::new(func);

        builder.switch_to_block(candidate.preheader);
        let length = builder.mload(candidate.object);
        let pair_size = builder.imm(64);
        let is_short = builder.lt(length, pair_size);
        builder.branch(is_short, small_check, setup);

        // if length != 0: two_check else accept
        builder.switch_to_block(small_check);
        builder.branch(length, two_check, candidate.accept);

        // if length <= 32: one_word else two_words
        builder.switch_to_block(two_check);
        let one_word_limit = builder.imm(33);
        let is_one_word = builder.lt(length, one_word_limit);
        builder.branch(is_one_word, one_word, two_words);

        // shift = (32 - length) * 8
        // word = mload(object + 32) >> shift
        // jump finish(iszero(word & 0x8080..80))
        builder.switch_to_block(one_word);
        let data = builder.add_u64_offset(candidate.object, 32);
        let word = builder.mload(data);
        let word_size = builder.imm(32);
        let padding = builder.sub(word_size, length);
        let byte_bits = builder.imm(8);
        let shift = builder.mul(padding, byte_bits);
        let word = builder.shr(shift, word);
        let one_result = emit_ascii_result(&mut builder, word);
        builder.jump(finish);

        // shift = (64 - length) * 8
        // words = mload(object + 32) | (mload(object + 64) >> shift)
        // jump finish(iszero(words & 0x8080..80))
        builder.switch_to_block(two_words);
        let data = builder.add_u64_offset(candidate.object, 32);
        let first = builder.mload(data);
        let second_pointer = builder.add_u64_offset(data, 32);
        let second = builder.mload(second_pointer);
        let pair_size = builder.imm(64);
        let padding = builder.sub(pair_size, length);
        let byte_bits = builder.imm(8);
        let shift = builder.mul(padding, byte_bits);
        let second = builder.shr(shift, second);
        let words = builder.or(first, second);
        let two_result = emit_ascii_result(&mut builder, words);
        builder.jump(finish);

        // data = object + 32
        // pair_end = data + (length & ~63)
        builder.switch_to_block(setup);
        let data = builder.add_u64_offset(candidate.object, 32);
        let pair_mask = builder.imm(U256::MAX - U256::from(63));
        let pair_bytes = builder.and(length, pair_mask);
        let pair_end = builder.add(data, pair_bytes);
        builder.jump(pair_header);

        // pointer = phi(setup: data, pair_body: next_pointer)
        // if pointer != pair_end: pair_body else pair_exit
        builder.switch_to_block(pair_header);
        let pointer = builder.phi(vec![(setup, data)]);
        let more = builder.xor(pointer, pair_end);
        builder.branch(more, pair_body, pair_exit);

        // first = mload(pointer)
        // second = mload(pointer + 32)
        // next_pointer = pointer + 64
        // if (first | second) & 0x8080..80: reject else pair_header
        builder.switch_to_block(pair_body);
        let first = builder.mload(pointer);
        let second_pointer = builder.add_u64_offset(pointer, 32);
        let second = builder.mload(second_pointer);
        let words = builder.or(first, second);
        let next_pointer = builder.add_u64_offset(second_pointer, 32);
        let high_bits = builder.imm(U256::from_be_bytes([0x80; 32]));
        let non_ascii = builder.and(words, high_bits);
        builder.branch(non_ascii, candidate.reject, pair_header);
        builder.add_phi_incoming(pointer, pair_body, next_pointer);

        // if length & 32: single_body else tail
        builder.switch_to_block(pair_exit);
        let word_bit = builder.imm(32);
        let has_single = builder.and(length, word_bit);
        builder.branch(has_single, single_body, tail);

        // tail_pointer = phi(pair_exit: pointer, single_body: pointer + 32)
        // tail_aggregate = phi(pair_exit: 0, single_body: mload(pointer))
        // padding_bits = (32 - (length & 31)) * 8
        // word = mload(tail_pointer) >> padding_bits
        // return iszero((tail_aggregate | word) & 0x8080..80)
        builder.switch_to_block(tail);
        let tail_pointer = builder.phi(vec![(pair_exit, pointer)]);
        let zero = builder.imm(0);
        let tail_aggregate = builder.phi(vec![(pair_exit, zero)]);
        let remainder_mask = builder.imm(31);
        let remainder = builder.and(length, remainder_mask);
        let word_size = builder.imm(32);
        let padding = builder.sub(word_size, remainder);
        let byte_bits = builder.imm(8);
        let padding_bits = builder.mul(padding, byte_bits);
        let word = builder.mload(tail_pointer);
        let word = builder.shr(padding_bits, word);
        let final_aggregate = builder.or(tail_aggregate, word);
        let tail_result = emit_ascii_result(&mut builder, final_aggregate);
        builder.jump(finish);

        // single = mload(pointer)
        // jump tail(pointer + 32, aggregate | single)
        builder.switch_to_block(single_body);
        let single = builder.mload(pointer);
        let single_pointer = builder.add_u64_offset(pointer, 32);
        builder.jump(tail);
        builder.add_phi_incoming(tail_pointer, single_body, single_pointer);
        builder.add_phi_incoming(tail_aggregate, single_body, single);

        // result = phi(one_word, two_words, tail)
        // return result
        builder.switch_to_block(finish);
        let result =
            builder.phi(vec![(one_word, one_result), (two_words, two_result), (tail, tail_result)]);
        builder.ret([result]);
    }

    // NOTE: The new word loop is compiler-generated and intentionally carries
    // no source checkpoint. Debug output must not alter executable code.
}

fn emit_ascii_result(builder: &mut FunctionBuilder<'_>, aggregate: ValueId) -> ValueId {
    // return iszero(aggregate & 0x8080..80)
    let high_bits = builder.imm(U256::from_be_bytes([0x80; 32]));
    let non_ascii = builder.and(aggregate, high_bits);
    builder.iszero(non_ascii)
}
