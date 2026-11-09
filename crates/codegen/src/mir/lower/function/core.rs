//! Compiler-owned scalar and checked-memory operations.
//!
//! Only declarations loaded from the verified `solar:core/` sources select these rewrites.
//! Ordinary argument binding preserves evaluation order, named arguments and attached receivers.
//! Range checks reject overflowing ends before access; partial writes merge untouched bytes.
//! Pointer offsets remain `ptradd`, without claiming a non-wrapping provenance proof.
//! `-Zno-core-intrinsics` uses the shipped Solidity bodies for differential testing. Targets
//! without CLZ retain the portable scans; the remaining algorithms stay ordinary library calls.
//! Unoptimized wide memory reads retain their byte loops and exact memory expansion. Optimized
//! builds use the word read; they already diagnose `msize` as inexact.

use super::*;
use solar_sema::core::CoreIntrinsic::{self, *};

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn core_intrinsic(&self, id: hir::FunctionId) -> Option<CoreIntrinsic> {
        if self.cx.gcx.sess.opts.unstable.no_core_intrinsics {
            return None;
        }
        let intrinsic = solar_sema::core::intrinsic_of(self.cx.gcx, id)?;
        if matches!(intrinsic, CoreIntrinsic::Read { width, calldata: false } if width > 1)
            && self.cx.module.objective_of(self.builder.func()).is_none()
        {
            // Each byte read issues an unaligned MLOAD. The loop's final load can expand
            // memory beyond the single word read, observable through MSIZE at -O none.
            return None;
        }
        if matches!(intrinsic, CoreIntrinsic::LeadingZeros | CoreIntrinsic::HighestSetBit)
            && !self.cx.gcx.sess.opts.evm_version.has_clz()
        {
            return None;
        }
        Some(intrinsic)
    }

    /// Keeps the portable loop when forged aliases can change its later reads or checks.
    pub(super) fn lower_guarded_core_intrinsic(
        &mut self,
        intrinsic: CoreIntrinsic,
        function: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let Some(guard) = self.core_loop_alias_guard(intrinsic, operands) else {
            return self.lower_core_intrinsic(intrinsic, operands);
        };
        let fallback = *self.cx.function_ids.get(&function)?;
        let returns_value = matches!(intrinsic, CoreIntrinsic::Read { .. });
        // branch guard, portable, native
        // portable: slow = icall original_body, operands; jump done
        // native: fast = intrinsic operands; jump done
        // done: phi [portable: slow], [native: fast] (reads only)
        let portable = self.builder.create_block();
        let native = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(guard, portable, native);
        self.builder.switch_to_block(portable);
        let slow = if returns_value {
            Some(self.builder.icall(fallback, operands.to_vec(), MirType::I256))
        } else {
            self.builder.icall_void(fallback, operands.to_vec());
            None
        };
        let slow_block = self.builder.current_block();
        self.builder.jump(done);
        self.builder.switch_to_block(native);
        let fast = self.lower_core_intrinsic(intrinsic, operands)?;
        let fast_block = self.builder.current_block();
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(match slow {
            Some(slow) => self.builder.phi(vec![(slow_block, slow), (fast_block, fast)]),
            None => self.builder.imm(0),
        })
    }

    fn core_loop_alias_guard(
        &mut self,
        intrinsic: CoreIntrinsic,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        if let (CoreIntrinsic::Read { width, calldata: true }, &[object, offset]) =
            (intrinsic, operands)
            && width > 1
        {
            // start = slice_ptr object + offset
            // last = start + (width - 1)
            // wrapped = last < start
            let data = self.builder.memory_data(object);
            let start = self.builder.ptradd(data, offset);
            let start = self.builder.cast(start, MirType::I256);
            let tail = self.builder.imm(width - 1);
            let last = self.builder.add(start, tail);
            return Some(self.builder.lt(last, start));
        }
        let (object, offset, count, source) = match (intrinsic, operands) {
            (CoreIntrinsic::Write(width), &[object, offset, _]) if width > 1 => {
                let count = self.builder.imm(width);
                (object, offset, count, None)
            }
            (CoreIntrinsic::Copy { calldata }, &[object, offset, src, src_offset, count]) => {
                (object, offset, count, Some((src, src_offset, calldata)))
            }
            _ => return None,
        };
        // start = ptrtoint (ptradd (data object), offset)
        // wrapped = start < ptrtoint (data object)
        let data = self.builder.memory_data(object);
        let pointer = self.builder.ptradd(data, offset);
        let start = self.builder.cast(pointer, MirType::I256);
        let data = self.builder.cast(data, MirType::I256);
        let mut guard = self.builder.lt(start, data);
        if let Some((source, offset, calldata)) = source {
            // end = start + count
            // from = ptrtoint (ptradd (data source), offset)
            // last = from + count
            // overlap = object != source && start < last && from < end
            // header = start < (ptrtoint source + 32) && ptrtoint source < end
            // guard = wrapped || end < start || last < from || overlap || header
            let from = self.builder.memory_data(source);
            let from = self.builder.ptradd(from, offset);
            let from = self.builder.cast(from, MirType::I256);
            let end = self.builder.add(start, count);
            let last = self.builder.add(from, count);
            if !calldata {
                let different = self.builder.ne(object, source);
                let below = self.builder.lt(start, last);
                let above = self.builder.lt(from, end);
                let overlap = self.builder.and(below, above);
                let overlap = self.builder.and(different, overlap);
                let source = self.builder.cast(source, MirType::I256);
                let word = self.builder.imm(EvmMemoryLayout::WORD_SIZE);
                let header_end = self.builder.add(source, word);
                let below = self.builder.lt(start, header_end);
                let above = self.builder.lt(source, end);
                let header = self.builder.and(below, above);
                guard = self.builder.or(guard, overlap);
                guard = self.builder.or(guard, header);
            }
            let wraps_destination = self.builder.lt(end, start);
            let wraps_source = self.builder.lt(last, from);
            for condition in [wraps_destination, wraps_source] {
                guard = self.builder.or(guard, condition);
            }
        }
        Some(guard)
    }

    pub(super) fn lower_core_intrinsic(
        &mut self,
        intrinsic: CoreIntrinsic,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        Some(match (intrinsic, operands) {
            (LeadingZeros, &[x]) => self.builder.clz(x),
            (HighestSetBit | TrailingZeros, &[x]) => {
                // bit = x & (0 - x)  (trailing only)
                // index = select bit == 0, 256, 255 - clz bit
                let x = if matches!(intrinsic, TrailingZeros) {
                    let zero = self.builder.imm(0);
                    let negated = self.builder.sub(zero, x);
                    self.builder.and(x, negated)
                } else {
                    x
                };
                if matches!(intrinsic, TrailingZeros)
                    && !self.cx.gcx.sess.opts.evm_version.has_clz()
                {
                    return Some(self.core_lowest_bit_index(x));
                }
                let count = self.builder.clz(x);
                let top = self.builder.imm(255);
                let index = self.builder.sub(top, count);
                let empty = self.builder.eq_zero(x);
                let missing = self.builder.imm(256);
                self.builder.select(empty, missing, index)
            }
            (WrappingAdd, &[x, y]) => self.builder.add(x, y),
            (WrappingSub, &[x, y]) => self.builder.sub(x, y),
            (WrappingMul, &[x, y]) => self.builder.mul(x, y),
            (Mul512, &[x, y]) => {
                // folded = mulmod x, y, MAX
                // low = mul x, y
                // high = sub (sub folded, low), (zext (lt folded, low))
                // product = struct high, low
                let max = self.builder.imm(U256::MAX);
                let folded = self.builder.mulmod(x, y, max);
                let low = self.builder.mul(x, y);
                let high = self.builder.sub(folded, low);
                let borrow = self.builder.lt(folded, low);
                let borrow = self.builder.cast(borrow, MirType::I256);
                let high = self.builder.sub(high, borrow);
                let MirType::Struct(ty) =
                    self.cx.module.intern_struct(vec![MirType::I256, MirType::I256])
                else {
                    unreachable!()
                };
                self.builder.make_struct(ty, vec![high, low])
            }
            (Read { width, calldata }, &[object, offset]) => {
                // address = checked_range object, offset, width
                // word = mload|calldataload address
                // result = and word, leading_mask(width)
                let count = self.builder.imm(width);
                let address = self.core_checked_range(object, offset, count, calldata);
                let location = if calldata { PtrLocation::Calldata } else { PtrLocation::Memory };
                let value = self.builder.load(address, location);
                let mask = self.builder.imm(U256::MAX << (256 - usize::from(width) * 8));
                self.builder.and(value, mask)
            }
            (Write(width), &[object, offset, value]) => {
                // address = checked_range object, offset, width
                // stored = (mload address & ~leading_mask(width)) | (value & leading_mask(width))
                // mstore address, stored
                let count = self.builder.imm(width);
                let address = self.core_checked_range(object, offset, count, false);
                let mask = U256::MAX << (256 - usize::from(width) * 8);
                let stored = if width == 32 {
                    value
                } else {
                    let old = self.builder.load(address, PtrLocation::Memory);
                    let tail = self.builder.imm(!mask);
                    let keep = self.builder.and(old, tail);
                    let mask = self.builder.imm(mask);
                    let value = self.builder.and(value, mask);
                    self.builder.or(keep, value)
                };
                self.builder.store(address, stored);
                self.builder.imm(0)
            }
            (Copy { calldata }, &[dst, dst_offset, src, src_offset, count]) => {
                // destination = checked_range dst, dst_offset, count
                // source = checked_range src, src_offset, count
                // mcopy|calldatacopy destination, source, count
                let dst = self.core_checked_range(dst, dst_offset, count, false);
                let src = self.core_checked_range(src, src_offset, count, calldata);
                self.builder.copy(
                    dst,
                    src,
                    count,
                    if calldata { PtrLocation::Calldata } else { PtrLocation::Memory },
                );
                self.builder.imm(0)
            }
            (Truncate, &[object, length]) => {
                // panic_if length > mload object, ArrayOutOfBounds
                // mstore object, length
                let old = self.builder.memory_len(object);
                let grows = self.builder.gt(length, old);
                self.builder.panic_if(grows, PanicCode::ArrayOutOfBounds);
                self.builder.set_memory_len(object, length);
                self.builder.imm(0)
            }
            (KeccakRange, &[object, offset, count]) => {
                // address = checked_range object, offset, count
                // hash = keccak256 address, count
                let address = self.core_checked_range(object, offset, count, false);
                self.builder.keccak256(address, count)
            }
            _ => return None,
        })
    }

    fn core_checked_range(
        &mut self,
        object: ValueId,
        offset: ValueId,
        count: ValueId,
        _calldata: bool,
    ) -> ValueId {
        // end = add offset, count
        // panic_if (end < offset) | (end > len object), ArrayOutOfBounds
        // address = ptradd (data object), offset
        let length = self.builder.memory_len(object);
        let end = self.builder.add(offset, count);
        let wrapped = self.builder.lt(end, offset);
        let over = self.builder.gt(end, length);
        let fails = self.builder.or(wrapped, over);
        self.builder.panic_if(fails, PanicCode::ArrayOutOfBounds);
        let data = self.builder.memory_data(object);
        self.builder.ptradd(data, offset)
    }
    /// The index of the single set bit of `bit`, or 256 when it is zero,
    /// without `clz`. A De Bruijn-like product's top six bits select the
    /// packed nibble holding the index's top three bits (eight for zero); a
    /// De Bruijn division inside that bit's 32-bit window gives the low five.
    /// EVM division by zero yields zero, which the zero input relies on.
    fn core_lowest_bit_index(&mut self, bit: ValueId) -> ValueId {
        // high = ((NIBBLES << ((bit * SPREAD >> 250) << 2)) >> 252) << 5
        // low = byte((0xd76453e0 / (bit >> high)) & 31, LOW)
        // index = high | low
        let spread = self.builder.imm(
            U256::from_str_radix(
                "b6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff",
                16,
            )
            .unwrap(),
        );
        let product = self.builder.mul(bit, spread);
        let top_six = self.builder.imm(250);
        let slot = self.builder.shr(product, top_six);
        let two = self.builder.imm(2);
        let slot_bits = self.builder.shl(slot, two);
        let nibbles = self.builder.imm(
            U256::from_str_radix(
                "8040405543005266443200005020610674053026020000107506200176117077",
                16,
            )
            .unwrap(),
        );
        let selected = self.builder.shl(nibbles, slot_bits);
        let top_nibble = self.builder.imm(252);
        let high = self.builder.shr(selected, top_nibble);
        let five = self.builder.imm(5);
        let high = self.builder.shl(high, five);
        let window = self.builder.shr(bit, high);
        let de_bruijn = self.builder.imm(0xd764_53e0_u64);
        let quotient = self.builder.div(de_bruijn, window);
        let thirty_one = self.builder.imm(31);
        let slot = self.builder.and(quotient, thirty_one);
        let low_table = self.builder.imm(
            U256::from_str_radix(
                "001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160405",
                16,
            )
            .unwrap(),
        );
        let low = self.builder.byte(slot, low_table);
        self.builder.or(high, low)
    }
}
