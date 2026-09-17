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
        let intrinsic = solar_sema::core::intrinsic_of(self.cx.gcx, function_id)?;
        // Without the instruction the shipped body is the implementation.
        let needs_clz = matches!(
            intrinsic,
            CoreIntrinsic::LeadingZeros
                | CoreIntrinsic::HighestSetBit
                | CoreIntrinsic::TrailingZeros
        );
        if needs_clz && !self.cx.gcx.sess.opts.evm_version.has_clz() {
            return None;
        }
        Some(intrinsic)
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
            CoreIntrinsic::RevertRaw => self.lower_core_revert_raw(&operands),
            CoreIntrinsic::Keccak256Range => self.lower_core_keccak256_range(&operands),
            CoreIntrinsic::Deploy | CoreIntrinsic::Deploy2 => {
                self.lower_core_deploy(intrinsic, &operands)
            }
            CoreIntrinsic::TryDeploy | CoreIntrinsic::TryDeploy2 => {
                self.lower_core_try_deploy(intrinsic, function_id, &operands)
            }
            CoreIntrinsic::TryDeployInto => self.lower_core_try_deploy_into(function_id, &operands),
            CoreIntrinsic::TryReadBytes(width) => {
                self.lower_core_try_read(function_id, &operands, width)
            }
            CoreIntrinsic::TryReadUint256Be => self.lower_core_try_read(function_id, &operands, 32),
            CoreIntrinsic::CalldataReadBytes(width) => {
                self.lower_core_calldata_read(&operands, width)
            }
            CoreIntrinsic::CalldataReadUint256Be => self.lower_core_calldata_read(&operands, 32),
            CoreIntrinsic::CalldataCopyInto => self.lower_core_calldata_copy(&operands),
            CoreIntrinsic::CalldataTryReadBytes(width) => {
                self.lower_core_calldata_try_read(function_id, &operands, width)
            }
            CoreIntrinsic::CalldataTryReadUint256Be => {
                self.lower_core_calldata_try_read(function_id, &operands, 32)
            }
            CoreIntrinsic::CodeCopyInto => self.lower_core_code_copy(&operands),
            CoreIntrinsic::LeadingZeros => {
                let [value] = *operands.as_slice() else { return None };
                Some(self.builder.clz(value))
            }
            CoreIntrinsic::HighestSetBit => {
                let [value] = *operands.as_slice() else { return None };
                Some(self.core_highest_set_bit(value))
            }
            CoreIntrinsic::TrailingZeros => {
                let [value] = *operands.as_slice() else { return None };
                // lowest = value & (0 - value)
                let zero = self.builder.imm(U256::ZERO);
                let negated = self.builder.sub(zero, value);
                let lowest = self.builder.and(value, negated);
                Some(self.core_highest_set_bit(lowest))
            }
            CoreIntrinsic::CallInto
            | CoreIntrinsic::StaticCallInto
            | CoreIntrinsic::DelegateCallInto => {
                self.lower_core_call_into(intrinsic, function_id, &operands)
            }
            CoreIntrinsic::Mul512 => self.lower_core_mul512(function_id, &operands),
            CoreIntrinsic::WrappingAdd
            | CoreIntrinsic::WrappingSub
            | CoreIntrinsic::WrappingMul => {
                let [x, y] = *operands.as_slice() else { return None };
                // result = add|sub|mul(x, y)
                Some(match intrinsic {
                    CoreIntrinsic::WrappingAdd => self.builder.add(x, y),
                    CoreIntrinsic::WrappingSub => self.builder.sub(x, y),
                    _ => self.builder.mul(x, y),
                })
            }
        }
    }

    /// The index of the highest set bit of `value`, 256 for zero. A count of
    /// leading zeros is at most 255 for a non-zero word, where exclusive-or
    /// with 255 subtracts it; zero counts 256, which that turns into 511, and
    /// the second term brings back to 256.
    fn core_highest_set_bit(&mut self, value: ValueId) -> ValueId {
        // index = (255 ^ clz(value)) ^ (255 * iszero(value))
        let count = self.builder.clz(value);
        let top = self.builder.imm(U256::from(255));
        let index = self.builder.xor(top, count);
        let is_zero = self.builder.iszero(value);
        let fix = self.builder.mul(top, is_zero);
        self.builder.xor(index, fix)
    }

    /// Packs an intrinsic's results the way a call to `function_id` returns
    /// them, so the caller destructures either the same way.
    fn core_results(&mut self, function_id: hir::FunctionId, values: Vec<ValueId>) -> ValueId {
        let function = self.cx.gcx.hir.function(function_id);
        let types = function
            .returns
            .iter()
            .map(|&variable| self.cx.gcx.type_of_item(variable.into()))
            .collect::<Vec<_>>();
        self.pack_return_values(values, &types)
    }

    /// `Calls.callInto`, `staticCallInto` and `delegateCallInto`: the output
    /// lands in the caller's buffer, never past it.
    fn lower_core_call_into(
        &mut self,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (target, value, gas, payload, output) = match (intrinsic, operands) {
            (CoreIntrinsic::CallInto, &[target, value, gas, payload, output]) => {
                (target, Some(value), gas, payload, output)
            }
            (
                CoreIntrinsic::StaticCallInto | CoreIntrinsic::DelegateCallInto,
                &[target, gas, payload, output],
            ) => (target, None, gas, payload, output),
            _ => return None,
        };
        let input_size = self.builder.memory_object_len(payload, MemoryObjectKind::Bytes);
        let input = self.builder.memory_object_data(payload, MemoryObjectKind::Bytes);
        let capacity = self.builder.memory_object_len(output, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);
        // success = call|staticcall|delegatecall(gas, target[, value], input, input_size,
        //                                        destination, capacity)
        let success = match (intrinsic, value) {
            (CoreIntrinsic::CallInto, Some(value)) => {
                self.builder.call(gas, target, value, input, input_size, destination, capacity)
            }
            (CoreIntrinsic::StaticCallInto, _) => {
                self.builder.staticcall(gas, target, input, input_size, destination, capacity)
            }
            _ => self.builder.delegatecall(gas, target, input, input_size, destination, capacity),
        };
        // total = returndatasize()
        // copied = total < capacity ? total : capacity
        let total = self.builder.returndatasize();
        let shorter = self.builder.lt(total, capacity);
        let copied = self.builder.select(shorter, total, capacity);
        Some(self.core_results(function_id, vec![success, copied, total]))
    }

    /// `Math.mul512(x, y)`: the product modulo `2**256 - 1` is `high + low`
    /// there, so the high word is its difference from the low one, less a
    /// borrow.
    fn lower_core_mul512(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [x, y] = *operands else { return None };
        // low = mul(x, y)
        // folded = mulmod(x, y, not(0))
        // high = sub(sub(folded, low), lt(folded, low))
        let low = self.builder.mul(x, y);
        let modulus = self.builder.imm(U256::MAX);
        let folded = self.builder.mulmod(x, y, modulus);
        let difference = self.builder.sub(folded, low);
        let borrow = self.builder.lt(folded, low);
        let high = self.builder.sub(difference, borrow);
        Some(self.core_results(function_id, vec![high, low]))
    }

    /// `Revert.raw(data)`: the call never returns.
    fn lower_core_revert_raw(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [data] = *operands else { return None };
        // revert(data(object), len(object))
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let pointer = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        self.builder.revert(pointer, length);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `Hash.keccak256Range(b, offset, count)`: the range is hashed where it
    /// lies instead of being copied out first.
    fn lower_core_keccak256_range(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [object, offset, count] = *operands else { return None };
        let start = self.core_checked_range(object, offset, Width::Dynamic(count));
        // hash = keccak256(start, count)
        Some(self.builder.keccak256(start, count))
    }

    /// The `create` or `create2` both deployment families share.
    fn core_create(&mut self, initcode: ValueId, salt: Option<ValueId>, value: ValueId) -> ValueId {
        let length = self.builder.memory_object_len(initcode, MemoryObjectKind::Bytes);
        let pointer = self.builder.memory_object_data(initcode, MemoryObjectKind::Bytes);
        // deployed = create|create2(value, data, len[, salt])
        match salt {
            Some(salt) => self.builder.create2(value, pointer, length, salt),
            None => self.builder.create(value, pointer, length),
        }
    }

    /// `Create.tryDeploy(initcode, value)` and `tryDeploy2(initcode, salt, value)`:
    /// a creation that returns no address is `false`, not a revert.
    fn lower_core_try_deploy(
        &mut self,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (initcode, salt, value) = match (intrinsic, operands) {
            (CoreIntrinsic::TryDeploy, &[initcode, value]) => (initcode, None, value),
            (CoreIntrinsic::TryDeploy2, &[initcode, salt, value]) => (initcode, Some(salt), value),
            _ => return None,
        };
        let deployed = self.core_create(initcode, salt, value);
        // success = deployed != 0
        let failed = self.builder.iszero(deployed);
        let success = self.builder.iszero(failed);
        Some(self.core_results(function_id, vec![success, deployed]))
    }

    /// `Create.tryDeployInto(initcode, value, diagnostics)`. A creation that
    /// succeeds leaves no return data, so the copy needs no branch: it moves
    /// nothing then.
    fn lower_core_try_deploy_into(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [initcode, value, diagnostics] = *operands else { return None };
        let capacity = self.builder.memory_object_len(diagnostics, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(diagnostics, MemoryObjectKind::Bytes);
        let deployed = self.core_create(initcode, None, value);
        // total = returndatasize()
        // copied = total < capacity ? total : capacity
        // returndatacopy(data(diagnostics), 0, copied)
        let total = self.builder.returndatasize();
        let shorter = self.builder.lt(total, capacity);
        let copied = self.builder.select(shorter, total, capacity);
        let zero = self.builder.imm(U256::ZERO);
        self.builder.returndatacopy_heap(destination, zero, copied);
        // success = deployed != 0
        let failed = self.builder.iszero(deployed);
        let success = self.builder.iszero(failed);
        Some(self.core_results(function_id, vec![success, deployed, copied, total]))
    }

    /// `Create.deploy(initcode, value)` and `Create.deploy2(initcode, salt, value)`.
    fn lower_core_deploy(
        &mut self,
        intrinsic: CoreIntrinsic,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (initcode, salt, value) = match (intrinsic, operands) {
            (CoreIntrinsic::Deploy, &[initcode, value]) => (initcode, None, value),
            (CoreIntrinsic::Deploy2, &[initcode, salt, value]) => (initcode, Some(salt), value),
            _ => return None,
        };
        let deployed = self.core_create(initcode, salt, value);
        // if deployed == 0 { mstore(0, DeploymentFailed.selector); revert(0, 4) }
        let failed = self.builder.iszero(deployed);
        let failure = self.builder.create_block();
        let success = self.builder.create_block();
        self.builder.branch(failed, failure, success);
        self.builder.switch_to_block(failure);
        let zero = self.builder.imm(U256::ZERO);
        let selector = self.builder.imm(DEPLOYMENT_FAILED_SELECTOR << 224);
        self.builder.mstore(zero, selector);
        let four = self.builder.imm(4);
        self.builder.revert(zero, four);
        self.builder.switch_to_block(success);
        Some(deployed)
    }

    /// `Code.copyInto(dst, dstOffset, target, start, count)`: checked against
    /// both the buffer and the code's size, so nothing is zero-padded.
    fn lower_core_code_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, target, start, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        // size = extcodesize(target)
        // end = start + count
        // panic(0x32) if end < start || end > size
        let size = self.builder.extcodesize(target);
        let end = self.builder.add(start, count);
        let wrapped = self.builder.lt(end, start);
        let over = self.builder.gt(end, size);
        let bad = self.builder.or(wrapped, over);
        self.builder.panic_if(bad, PanicCode::ArrayOutOfBounds);
        // extcodecopy(target, destination, start, count)
        self.builder.extcodecopy_heap(target, destination, start, count);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `tryReadBytesN(b, offset)` and `tryReadUint256BE(b, offset)`: the range
    /// test becomes the flag instead of a panic. A read that fails is aimed at
    /// the start of the buffer, so it never reaches far past it, and its
    /// result is discarded.
    fn lower_core_try_read(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
        width: u8,
    ) -> Option<ValueId> {
        let [object, offset] = *operands else { return None };
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        let misses = self.core_range_misses(length, offset, Width::Const(u64::from(width)));
        // ok = !misses
        // word = mload(data(object) + (ok ? offset : 0))
        // value = (ok ? word : 0) & leading(width)
        let ok = self.builder.iszero(misses);
        let zero = self.builder.imm(U256::ZERO);
        let aimed = self.builder.select(ok, offset, zero);
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let address = self.builder.add(data, aimed);
        let word = self.builder.mload(address);
        let gated = self.builder.select(ok, word, zero);
        // The mask comes last so that a cleanup of the typed result folds
        // into it.
        let value = match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(gated, mask)
            }
            None => gated,
        };
        Some(self.core_results(function_id, vec![ok, value]))
    }

    /// `CalldataBytes.readBytesN(b, offset)` and `readUint256BE(b, offset)`.
    fn lower_core_calldata_read(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [slice, offset] = *operands else { return None };
        let address =
            self.core_checked_calldata_range(slice, offset, Width::Const(u64::from(width)));
        // word = calldataload(ptr(slice) + offset)
        // result = width < 32 ? word & leading(width) : word
        let word = self.builder.calldataload(address);
        Some(match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(word, mask)
            }
            None => word,
        })
    }

    /// `CalldataBytes.tryReadBytesN(b, offset)` and `tryReadUint256BE(b, offset)`.
    /// A load from calldata cannot fault, so a read that fails is aimed at the
    /// start of the slice only to keep its address small, and is discarded.
    fn lower_core_calldata_try_read(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
        width: u8,
    ) -> Option<ValueId> {
        let [slice, offset] = *operands else { return None };
        let length = self.builder.slice_len(slice);
        let misses = self.core_range_misses(length, offset, Width::Const(u64::from(width)));
        // ok = !misses
        // word = calldataload(ptr(slice) + (ok ? offset : 0))
        // value = (ok ? word : 0) & leading(width)
        let ok = self.builder.iszero(misses);
        let zero = self.builder.imm(U256::ZERO);
        let aimed = self.builder.select(ok, offset, zero);
        let base = self.builder.slice_ptr(slice);
        let address = self.builder.add(base, aimed);
        let word = self.builder.calldataload(address);
        let gated = self.builder.select(ok, word, zero);
        let value = match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(gated, mask)
            }
            None => gated,
        };
        Some(self.core_results(function_id, vec![ok, value]))
    }

    /// `CalldataBytes.copyInto(dst, dstOffset, src, srcOffset, count)`.
    fn lower_core_calldata_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, src, src_offset, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        let source = self.core_checked_calldata_range(src, src_offset, Width::Dynamic(count));
        // calldatacopy(destination, source, count)
        self.builder.calldatacopy_heap(destination, source, count);
        Some(self.builder.imm(U256::ZERO))
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
        // panic(0x32) if misses(length, offset, width)
        let misses = self.core_range_misses(length, offset, width);
        self.builder.panic_if(misses, PanicCode::ArrayOutOfBounds);
        // address = data(object) + offset
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        self.builder.add(data, offset)
    }

    /// The calldata counterpart of [`Self::core_checked_range`]: the range is
    /// checked against the slice's length and the address is a calldata
    /// offset.
    fn core_checked_calldata_range(
        &mut self,
        slice: ValueId,
        offset: ValueId,
        width: Width,
    ) -> ValueId {
        let length = self.builder.slice_len(slice);
        // panic(0x32) if misses(length, offset, width)
        let misses = self.core_range_misses(length, offset, width);
        self.builder.panic_if(misses, PanicCode::ArrayOutOfBounds);
        // address = ptr(slice) + offset
        let base = self.builder.slice_ptr(slice);
        self.builder.add(base, offset)
    }

    /// Whether `[offset, offset + width)` fails to lie inside `length` bytes.
    fn core_range_misses(&mut self, length: ValueId, offset: ValueId, width: Width) -> ValueId {
        // end = offset + width
        // misses = end < offset || end > length
        let end = match width {
            Width::Dynamic(count) => self.builder.add(offset, count),
            Width::Const(width) => {
                let width = self.builder.imm(width);
                self.builder.add(offset, width)
            }
        };
        let wrapped = self.builder.lt(end, offset);
        let over = self.builder.gt(end, length);
        self.builder.or(wrapped, over)
    }
}

/// The selector of `Create`'s `DeploymentFailed()` error.
const DEPLOYMENT_FAILED_SELECTOR: U256 = U256::from_limbs([0x3011_6425, 0, 0, 0]);

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
