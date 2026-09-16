//! Lowering for the compiler-owned `solar:core/` modules.
//!
//! A function declared in one of those modules is identified by the module it
//! comes from and its name, never by name alone, so an ordinary function
//! spelled `readBytes4` is still an ordinary function. Each entry point here
//! lowers to word operations directly instead of calling the body that ships
//! with the module; `-Zno-core-intrinsics` calls the body instead, which is
//! how the two are compared.
//!
//! Every operation checks that the full range fits before it writes anything
//! and raises the same `Panic(0x32)` the body raises. Fixed-width reads and
//! writes touch the word containing the range, which can reach up to
//! thirty-one bytes past it; a write puts those bytes back unchanged, so the
//! difference is visible only as memory expansion and `msize`, never as a
//! value. `copyInto` is a move by contract, which is what `mcopy` is, so its
//! two ranges need no disjointness proof; `truncate` is a store to the length
//! word, which the alias and value-numbering analyses already model.

use super::*;
use solar_sema::core::CoreIntrinsic;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Returns the intrinsic `function_id` names, when it is one and intrinsic
    /// lowering is enabled.
    pub(super) fn core_intrinsic(&self, function_id: hir::FunctionId) -> Option<CoreIntrinsic> {
        if self.cx.gcx.sess.opts.unstable.no_core_intrinsics {
            return None;
        }
        solar_sema::core::intrinsic_of(self.cx.gcx, function_id)
    }

    /// Lowers a call to a compiler-owned module function.
    pub(super) fn lower_core_intrinsic_call(
        &mut self,
        expr: &hir::Expr<'_>,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        receiver: Option<&hir::Expr<'_>>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let function = self.cx.gcx.hir.function(function_id);
        // The receiver of `using Bytes for bytes` is the first parameter, so
        // both spellings reach the same operand list and the same operation.
        let mut operands = Vec::with_capacity(function.parameters.len());
        let mut parameter_tys = Vec::with_capacity(function.parameters.len());
        let exprs = receiver.into_iter().chain(args.exprs());
        for (index, argument) in exprs.enumerate() {
            let parameter = *function.parameters.get(index)?;
            let parameter_ty = self.cx.gcx.type_of_item(parameter.into());
            let value = self.lower_typed_expr(argument, parameter_ty)?;
            let value = self.materialize_call_argument(parameter_ty, value, argument.span)?;
            operands.push(value);
            parameter_tys.push(parameter_ty);
        }
        if operands.len() != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "compiler module argument list");
        }

        match intrinsic {
            CoreIntrinsic::ReadBytes(width) => self.lower_core_read(&operands, width),
            CoreIntrinsic::ReadUint256Be => self.lower_core_read(&operands, 32),
            CoreIntrinsic::WriteBytes(width) => self.lower_core_write(&operands, width),
            CoreIntrinsic::WriteUint256Be => self.lower_core_write(&operands, 32),
            CoreIntrinsic::CopyInto => self.lower_core_copy(&operands),
            CoreIntrinsic::Fill => self.lower_core_fill(&operands),
            CoreIntrinsic::Truncate => self.lower_core_truncate(expr, &operands, &parameter_tys),
        }
    }

    /// `readBytesN(b, offset)` and `readUint256BE(b, offset)`.
    fn lower_core_read(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [object, offset] = *operands else { return None };
        let data = self.core_checked_range(object, offset, Width::Const(u64::from(width)));
        // word = mload(data + offset)
        // result = width < 32 ? word & leading(width) : word
        let word = self.builder.mload(data);
        Some(match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(word, mask)
            }
            None => word,
        })
    }

    /// `writeBytesN(b, offset, value)` and `writeUint256BE(b, offset, value)`.
    fn lower_core_write(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [object, offset, value] = *operands else { return None };
        let data = self.core_checked_range(object, offset, Width::Const(u64::from(width)));
        match leading_mask(width) {
            // stored = (mload(data) & ~leading) | (value & leading)
            // mstore(data, stored)
            Some(mask) => {
                let tail = self.builder.imm(!mask);
                let mask = self.builder.imm(mask);
                let old = self.builder.mload(data);
                let kept = self.builder.and(old, tail);
                let taken = self.builder.and(value, mask);
                let stored = self.builder.or(kept, taken);
                self.builder.mstore(data, stored);
            }
            // A whole word replaces everything at the address.
            None => self.builder.mstore(data, value),
        }
        Some(self.builder.imm(U256::ZERO))
    }

    /// `copyInto(dst, dstOffset, src, srcOffset, count)`.
    fn lower_core_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, src, src_offset, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        let source = self.core_checked_range(src, src_offset, Width::Dynamic(count));
        // mcopy(destination, source, count)
        self.builder.mcopy_heap(destination, source, count);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `fill(dst, offset, count, value)`.
    fn lower_core_fill(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, offset, count, value] = *operands else { return None };
        let base = self.core_checked_range(dst, offset, Width::Dynamic(count));
        // pattern = byte(0, value) * 0x0101..01
        let top = self.builder.imm(248);
        let byte = self.builder.shr(top, value);
        let ones = self.builder.imm(U256::MAX / U256::from(255));
        let pattern = self.builder.mul(byte, ones);
        // for word in 0..count / 32 { mstore(base + word * 32, pattern) }
        let five = self.builder.imm(5);
        let words = self.builder.shr(five, count);
        self.builder.counted_loop(words, |builder, word| {
            let five = builder.imm(5);
            let stride = builder.shl(five, word);
            let address = builder.add(base, stride);
            builder.mstore(address, pattern);
        });
        // rest = count & 31
        // if rest != 0 {
        //   keep = max >> (8 * rest)
        //   tail = base + (words << 5)
        //   mstore(tail, (mload(tail) & keep) | (pattern & ~keep))
        // }
        let low = self.builder.imm(31);
        let rest = self.builder.and(count, low);
        let partial = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(rest, partial, done);
        self.builder.switch_to_block(partial);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let keep = self.builder.shr(bits, all);
        let five = self.builder.imm(5);
        let stride = self.builder.shl(five, words);
        let tail = self.builder.add(base, stride);
        let old = self.builder.mload(tail);
        let kept = self.builder.and(old, keep);
        let drop = self.builder.not(keep);
        let taken = self.builder.and(pattern, drop);
        let stored = self.builder.or(kept, taken);
        self.builder.mstore(tail, stored);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `truncate(a, n)`: shortens a dynamic memory array in place.
    fn lower_core_truncate(
        &mut self,
        expr: &hir::Expr<'_>,
        operands: &[ValueId],
        parameter_tys: &[Ty<'gcx>],
    ) -> Option<ValueId> {
        let ([object, new_len], [array_ty, _]) = (operands, parameter_tys) else { return None };
        let Some(layout) = self.types.memory_layout(*array_ty) else {
            return self.cx.report_unsupported(expr.span, "truncated array type");
        };
        let kind = layout.kind();
        // panic(0x32) if new_len > len(a)
        // set_len(a, new_len)
        let length = self.builder.memory_object_len(*object, kind);
        let grows = self.builder.gt(*new_len, length);
        self.builder.panic_if(grows, PanicCode::ArrayOutOfBounds);
        self.builder.set_memory_object_len(*object, *new_len, kind);
        Some(self.builder.imm(U256::ZERO))
    }

    /// Checks that `[offset, offset + width)` lies inside the `bytes` object
    /// and returns the address of its first byte.
    ///
    /// The sum is tested for wrapping as well as for fit, so an offset near
    /// the top of the word cannot wrap into a range that looks valid.
    fn core_checked_range(&mut self, object: ValueId, offset: ValueId, width: Width) -> ValueId {
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        // end = offset + width
        // panic(0x32) if end < offset || end > length
        let end = match width {
            Width::Dynamic(count) => self.builder.add(offset, count),
            Width::Const(width) => {
                let width = self.builder.imm(width);
                self.builder.add(offset, width)
            }
        };
        let wrapped = self.builder.lt(end, offset);
        let over = self.builder.gt(end, length);
        let bad = self.builder.or(wrapped, over);
        self.builder.panic_if(bad, PanicCode::ArrayOutOfBounds);
        // address = data(object) + offset
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        self.builder.add(data, offset)
    }
}

/// How wide a checked range is.
#[derive(Clone, Copy)]
enum Width {
    /// A width fixed by the operation.
    Const(u64),
    /// A width the caller passed.
    Dynamic(ValueId),
}

/// The mask keeping the leading `width` bytes of a word, or `None` at a whole
/// word, where nothing is masked away. `width` is at most 32.
fn leading_mask(width: u8) -> Option<U256> {
    (width < 32).then(|| !(U256::MAX >> (8 * usize::from(width))))
}
