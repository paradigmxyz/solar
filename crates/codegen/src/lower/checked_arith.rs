//! Checked arithmetic and Solidity panic lowering helpers.

use super::Lowerer;
use crate::mir::{BlockId, FunctionBuilder, ValueId};
use alloy_primitives::U256;
use solar_interface::Span;
use solar_sema::{
    hir::{self, ElementaryType, ExprKind},
    ty::{Ty, TyKind},
};

#[derive(Clone, Copy)]
pub(super) struct IntegerInfo {
    pub(super) signed: bool,
    pub(super) bits: u16,
}

#[derive(Clone, Copy)]
pub(super) struct ArithmeticInfo {
    pub(super) integer: Option<IntegerInfo>,
    pub(super) is_signed: bool,
    pub(super) span: Span,
    pub(super) unsupported_udvt_operator: bool,
}

#[derive(Clone, Copy)]
pub(super) enum PanicCode {
    Assert,
    ArithmeticOverflowUnderflow,
    DivisionByZero,
    PopEmptyArray,
    ArrayOutOfBounds,
    MemoryAllocationOverflow,
}

impl PanicCode {
    fn as_u64(self) -> u64 {
        match self {
            Self::Assert => 0x01,
            Self::ArithmeticOverflowUnderflow => 0x11,
            Self::DivisionByZero => 0x12,
            Self::PopEmptyArray => 0x31,
            Self::ArrayOutOfBounds => 0x32,
            Self::MemoryAllocationOverflow => 0x41,
        }
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Checks if a HIR type is a signed integer type.
    fn is_hir_type_signed(&self, ty: &hir::Type<'_>) -> bool {
        matches!(ty.kind, hir::TypeKind::Elementary(ElementaryType::Int(_)))
    }

    /// Checks if an expression has a signed integer type.
    /// This is a best-effort check based on the expression structure.
    pub(super) fn is_expr_signed(&self, expr: &hir::Expr<'_>) -> bool {
        if let Some(ty) = self.get_expr_type(expr) {
            return ty.is_signed();
        }

        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() {
                    let var = self.gcx.hir.variable(*var_id);
                    self.is_hir_type_signed(&var.ty)
                } else {
                    false
                }
            }
            ExprKind::Unary(_, inner) => self.is_expr_signed(inner),
            ExprKind::Binary(lhs, _, _) => self.is_expr_signed(lhs),
            ExprKind::Tuple(elements) => {
                if let Some(Some(inner)) = elements.first() {
                    self.is_expr_signed(inner)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(super) fn integer_info_for_expr(&self, expr: &hir::Expr<'_>) -> Option<IntegerInfo> {
        self.get_expr_type(expr).and_then(Self::integer_info_for_ty)
    }

    fn integer_info_for_ty(ty: Ty<'_>) -> Option<IntegerInfo> {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Int(size)) => {
                Some(IntegerInfo { signed: true, bits: size.bits() })
            }
            TyKind::Elementary(ElementaryType::UInt(size)) => {
                Some(IntegerInfo { signed: false, bits: size.bits() })
            }
            TyKind::IntLiteral(signed, size, _) => Some(IntegerInfo { signed, bits: size.bits() }),
            _ => None,
        }
    }

    pub(super) fn expr_has_udvt_type(&self, expr: &hir::Expr<'_>) -> bool {
        matches!(self.get_expr_type(expr).map(|ty| ty.peel_refs().kind), Some(TyKind::Udvt(..)))
    }

    pub(super) fn emit_unsupported_udvt_operator(&self, span: Span) {
        self.gcx
            .dcx()
            .err("user-defined operators are not supported in codegen yet")
            .span(span)
            .help("unwrap the user-defined value type before using this operator")
            .emit();
    }

    fn require_checked_arithmetic_info(
        &self,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> Option<IntegerInfo> {
        if self.in_unchecked_block || int_info.is_some() {
            return int_info;
        }

        self.gcx
            .dcx()
            .err("cannot determine arithmetic type for checked operation")
            .span(span)
            .emit();
        None
    }

    pub(super) fn signed_binary_fold_is_unsafe(op: hir::BinOpKind, is_signed: bool) -> bool {
        is_signed
            && matches!(
                op,
                hir::BinOpKind::Div
                    | hir::BinOpKind::Rem
                    | hir::BinOpKind::Shr
                    | hir::BinOpKind::Sar
                    | hir::BinOpKind::Lt
                    | hir::BinOpKind::Le
                    | hir::BinOpKind::Gt
                    | hir::BinOpKind::Ge
            )
    }

    /// Lowers a binary operation.
    pub(super) fn lower_binary_op(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        op: hir::BinOp,
        rhs: ValueId,
        arithmetic: ArithmeticInfo,
    ) -> ValueId {
        use hir::BinOpKind;

        if arithmetic.unsupported_udvt_operator {
            self.emit_unsupported_udvt_operator(arithmetic.span);
            return builder.imm_u64(0);
        }

        match op.kind {
            BinOpKind::Add => self.lower_checked_or_wrapping_add(
                builder,
                lhs,
                rhs,
                arithmetic.integer,
                arithmetic.span,
            ),
            BinOpKind::Sub => self.lower_checked_or_wrapping_sub(
                builder,
                lhs,
                rhs,
                arithmetic.integer,
                arithmetic.span,
            ),
            BinOpKind::Mul => self.lower_checked_or_wrapping_mul(
                builder,
                lhs,
                rhs,
                arithmetic.integer,
                arithmetic.span,
            ),
            BinOpKind::Div => {
                let int_info =
                    self.require_checked_arithmetic_info(arithmetic.integer, arithmetic.span);
                let is_signed = int_info.map_or(arithmetic.is_signed, |info| info.signed);
                self.emit_panic_if_zero(builder, rhs, PanicCode::DivisionByZero);
                if is_signed {
                    if !self.in_unchecked_block
                        && let Some(info) = int_info
                        && info.signed
                    {
                        self.emit_signed_min_div_minus_one_check(builder, lhs, rhs, info);
                    }
                    builder.sdiv(lhs, rhs)
                } else {
                    builder.div(lhs, rhs)
                }
            }
            BinOpKind::Rem => {
                let int_info =
                    self.require_checked_arithmetic_info(arithmetic.integer, arithmetic.span);
                let is_signed = int_info.map_or(arithmetic.is_signed, |info| info.signed);
                self.emit_panic_if_zero(builder, rhs, PanicCode::DivisionByZero);
                if is_signed { builder.smod(lhs, rhs) } else { builder.mod_(lhs, rhs) }
            }
            BinOpKind::Pow => self.lower_checked_or_wrapping_pow(
                builder,
                lhs,
                rhs,
                arithmetic.integer,
                arithmetic.span,
            ),
            // Logical AND: for bool inputs (guaranteed by type checker), just use bitwise AND.
            // Bool values are already 0 or 1, so a && b == a & b.
            BinOpKind::And => builder.and(lhs, rhs),
            // Logical OR: for bool inputs (guaranteed by type checker), just use bitwise OR.
            // Bool values are already 0 or 1, so a || b == a | b.
            BinOpKind::Or => builder.or(lhs, rhs),
            BinOpKind::BitAnd => builder.and(lhs, rhs),
            BinOpKind::BitOr => builder.or(lhs, rhs),
            BinOpKind::BitXor => builder.xor(lhs, rhs),
            BinOpKind::Shl => builder.shl(rhs, lhs),
            BinOpKind::Shr => {
                // For signed types, >> is arithmetic shift (SAR)
                if arithmetic.is_signed { builder.sar(rhs, lhs) } else { builder.shr(rhs, lhs) }
            }
            BinOpKind::Sar => builder.sar(rhs, lhs),
            BinOpKind::Lt => {
                if arithmetic.is_signed {
                    builder.slt(lhs, rhs)
                } else {
                    builder.lt(lhs, rhs)
                }
            }
            BinOpKind::Gt => {
                if arithmetic.is_signed {
                    builder.sgt(lhs, rhs)
                } else {
                    builder.gt(lhs, rhs)
                }
            }
            BinOpKind::Le => {
                if arithmetic.is_signed {
                    let gt = builder.sgt(lhs, rhs);
                    builder.iszero(gt)
                } else {
                    let gt = builder.gt(lhs, rhs);
                    builder.iszero(gt)
                }
            }
            BinOpKind::Ge => {
                if arithmetic.is_signed {
                    let lt = builder.slt(lhs, rhs);
                    builder.iszero(lt)
                } else {
                    let lt = builder.lt(lhs, rhs);
                    builder.iszero(lt)
                }
            }
            BinOpKind::Eq => builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = builder.eq(lhs, rhs);
                builder.iszero(eq)
            }
        }
    }

    /// Truncates a wrapping result back into its sub-word type. The checked
    /// paths prove the result in range, but unchecked sub-word arithmetic can
    /// wrap past the type's width, and the checked shapes (and ABI encoding)
    /// rely on values of type `uintN`/`intN` being clean.
    fn truncate_wrapping_result(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        int_info: Option<IntegerInfo>,
    ) -> ValueId {
        let Some(info) = int_info else { return value };
        if info.bits >= 256 {
            return value;
        }
        if info.signed {
            self.sign_extend_to_bits(builder, value, u32::from(info.bits))
        } else {
            self.mask_to_bits(builder, value, u32::from(info.bits))
        }
    }

    pub(super) fn lower_checked_or_wrapping_add(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> ValueId {
        let result = builder.add(lhs, rhs);
        if !self.in_unchecked_block {
            let Some(info) = self.require_checked_arithmetic_info(int_info, span) else {
                return result;
            };
            let overflow = if info.signed {
                self.signed_add_overflow(builder, lhs, rhs, result, info)
            } else {
                self.unsigned_add_overflow(builder, lhs, result, info)
            };
            self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
            result
        } else {
            self.truncate_wrapping_result(builder, result, int_info)
        }
    }

    pub(super) fn lower_checked_or_wrapping_sub(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> ValueId {
        let result = builder.sub(lhs, rhs);
        if !self.in_unchecked_block {
            let Some(info) = self.require_checked_arithmetic_info(int_info, span) else {
                return result;
            };
            let overflow = if info.signed {
                self.signed_sub_overflow(builder, lhs, rhs, result, info)
            } else {
                builder.lt(lhs, rhs)
            };
            self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
            result
        } else {
            self.truncate_wrapping_result(builder, result, int_info)
        }
    }

    fn lower_checked_or_wrapping_mul(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> ValueId {
        let result = builder.mul(lhs, rhs);
        if !self.in_unchecked_block {
            let Some(info) = self.require_checked_arithmetic_info(int_info, span) else {
                return result;
            };
            let overflow = if info.signed {
                self.signed_mul_overflow(builder, lhs, rhs, result, info)
            } else {
                self.unsigned_mul_overflow(builder, lhs, rhs, result, info)
            };
            self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
            result
        } else {
            self.truncate_wrapping_result(builder, result, int_info)
        }
    }

    /// Lowers `base ** exponent`, porting solc's `checked_exp_*` Yul helper
    /// structure: trivial bases and small bases use native `EXP` guarded by
    /// precomputed bounds, and only the general case falls back to checked
    /// exponentiation by squaring.
    fn lower_checked_or_wrapping_pow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> ValueId {
        if self.in_unchecked_block {
            let result = builder.exp(lhs, rhs);
            return self.truncate_wrapping_result(builder, result, int_info);
        }

        let Some(info) = self.require_checked_arithmetic_info(int_info, span) else {
            return builder.exp(lhs, rhs);
        };

        // Constant-base fast path (solc's `checked_exp_<literal>` shape): the
        // largest exponent whose power still fits the type is known at compile
        // time, so a single bound check makes native `EXP` exact. Like solc,
        // only full-width types take this path.
        if info.bits == 256
            && let Some(imm) = builder.func().value(lhs).as_immediate()
            && let Some(base) = imm.as_u256()
        {
            return self.lower_checked_pow_const_base(builder, lhs, base, rhs, info);
        }

        if info.signed {
            self.lower_checked_pow_signed(builder, lhs, rhs, info)
        } else {
            self.lower_checked_pow_unsigned(builder, lhs, rhs, info)
        }
    }

    /// Ports solc's `checked_exp_t_<rational>_t_uint*` literal-base helper:
    /// `if gt(exponent, ub) panic; power := exp(base, exponent)` where `ub` is
    /// the largest exponent such that `base ** ub` stays in range.
    fn lower_checked_pow_const_base(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base_value: ValueId,
        base: U256,
        exponent: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        debug_assert_eq!(info.bits, 256);
        // Bases 0, 1, and -1 (signed) can never overflow: their powers stay in
        // {-1, 0, 1}, and native `EXP` on the two's-complement representation
        // is exact mod 2^256.
        let trivial = base == U256::ZERO || base == U256::ONE || (info.signed && base == U256::MAX);
        if !trivial {
            let bound = Self::const_base_max_exponent(base, info);
            let bound = builder.imm_u64(u64::from(bound));
            let too_large = builder.gt(exponent, bound);
            self.emit_panic_if(builder, too_large, PanicCode::ArithmeticOverflowUnderflow);
        }
        if base == U256::from(2) {
            // `exp(2, e) == shl(e, 1)` for `e <= 255`, and SHL is cheaper.
            let one = builder.imm_u64(1);
            return builder.shl(exponent, one);
        }
        builder.exp(base_value, exponent)
    }

    /// Computes the largest exponent `e` with `base ** e` in range for the
    /// type: `|base| ** e <= max` for non-negative bases and
    /// `|base| ** e <= |min|` for negative ones. Underflow is the only
    /// negative-base concern: an even power equal to `2^255` would need
    /// `255 / e` integral for even `e`, which is impossible (255 is odd), so
    /// bounding by `|min|` never admits a positive overflow (same argument as
    /// solc's `overflowCheckedIntLiteralExpFunction`).
    fn const_base_max_exponent(base: U256, info: IntegerInfo) -> u32 {
        debug_assert_eq!(info.bits, 256);
        let (abs_base, limit) = if info.signed && base.bit(255) {
            // `wrapping_neg` maps MIN to 2^255, which is exactly `|min|`.
            (base.wrapping_neg(), U256::from(1) << 255)
        } else if info.signed {
            (base, Self::signed_max(info.bits))
        } else {
            (base, U256::MAX)
        };
        debug_assert!(abs_base >= U256::from(2));
        let mut bound = 0u32;
        let mut power = U256::from(1);
        while let Some(next) = power.checked_mul(abs_base) {
            if next > limit {
                break;
            }
            power = next;
            bound += 1;
        }
        bound
    }

    /// Ports solc's `checked_exp_unsigned` Yul helper.
    fn lower_checked_pow_unsigned(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        exponent: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        let max = Self::unsigned_max(info.bits);
        let max_imm = builder.imm_u256(max);
        let zero = builder.imm_u64(0);
        let one = builder.imm_u64(1);

        let join = builder.create_block();
        let mut results: Vec<(BlockId, ValueId)> = Vec::new();

        // if iszero(exponent) { power := 1 } (note that 0**0 == 1)
        let base_zero_block = builder.create_block();
        let exp_zero = builder.iszero(exponent);
        results.push((builder.current_block(), one));
        builder.branch(exp_zero, join, base_zero_block);

        // if iszero(base) { power := 0 }
        builder.switch_to_block(base_zero_block);
        let dispatch_block = builder.create_block();
        let base_zero = builder.iszero(base);
        results.push((builder.current_block(), zero));
        builder.branch(base_zero, join, dispatch_block);

        // switch base: case 1 { power := 1 } case 2 { bounded shift }
        // (lowered as an eq-chain, like solc's optimized switch output)
        builder.switch_to_block(dispatch_block);
        let base_two_check = builder.create_block();
        let base_one = builder.eq(base, one);
        results.push((builder.current_block(), one));
        builder.branch(base_one, join, base_two_check);

        builder.switch_to_block(base_two_check);
        let base_two_block = builder.create_block();
        let small_base_check = builder.create_block();
        let two = builder.imm_u64(2);
        let base_is_two = builder.eq(base, two);
        builder.branch(base_is_two, base_two_block, small_base_check);

        // Base 2: a power fits in 256 bits iff `exponent <= 255`, making the
        // shift exact; sub-word types additionally range-check the result.
        builder.switch_to_block(base_two_block);
        let max_shift = builder.imm_u64(255);
        let shift_too_large = builder.gt(exponent, max_shift);
        self.emit_panic_if(builder, shift_too_large, PanicCode::ArithmeticOverflowUnderflow);
        let power = builder.shl(exponent, one);
        if info.bits < 256 {
            let out_of_range = builder.gt(power, max_imm);
            self.emit_panic_if(builder, out_of_range, PanicCode::ArithmeticOverflowUnderflow);
        }
        results.push((builder.current_block(), power));
        builder.jump(join);

        // Small-base specialization: within the bounds checked by
        // `small_base_exp_is_exact`, native `EXP` cannot wrap and is exact.
        builder.switch_to_block(small_base_check);
        let native_block = builder.create_block();
        let loop_block = builder.create_block();
        let use_native = Self::small_base_exp_is_exact(builder, base, exponent);
        builder.branch(use_native, native_block, loop_block);

        builder.switch_to_block(native_block);
        let power = builder.exp(base, exponent);
        if info.bits < 256 {
            let out_of_range = builder.gt(power, max_imm);
            self.emit_panic_if(builder, out_of_range, PanicCode::ArithmeticOverflowUnderflow);
        }
        results.push((builder.current_block(), power));
        builder.jump(join);

        // General case: checked exponentiation by squaring.
        builder.switch_to_block(loop_block);
        let (power, base) = self.emit_checked_exp_loop(builder, one, base, exponent, max_imm);
        // Final multiply: panic iff `power * base > max`.
        let quotient = builder.div(max_imm, base);
        let overflow = builder.gt(power, quotient);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
        let power = builder.mul(power, base);
        results.push((builder.current_block(), power));
        builder.jump(join);

        builder.switch_to_block(join);
        builder.phi(results)
    }

    /// Emits the condition under which native `EXP` of a non-negative base is
    /// exact (no mod-2^256 wrap): `10**77 < 2^256` and `306**31 < 2^256`, so
    /// `(base < 11 && exponent < 78) || (base < 307 && exponent < 32)` powers
    /// fit in a word. The result still needs a range check against the type's
    /// max.
    fn small_base_exp_is_exact(
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        exponent: ValueId,
    ) -> ValueId {
        let eleven = builder.imm_u64(11);
        let base_lt_11 = builder.lt(base, eleven);
        let seventy_eight = builder.imm_u64(78);
        let exp_lt_78 = builder.lt(exponent, seventy_eight);
        let small_arm = builder.and(base_lt_11, exp_lt_78);
        let three_oh_seven = builder.imm_u64(307);
        let base_lt_307 = builder.lt(base, three_oh_seven);
        let thirty_two = builder.imm_u64(32);
        let exp_lt_32 = builder.lt(exponent, thirty_two);
        let medium_arm = builder.and(base_lt_307, exp_lt_32);
        builder.or(small_arm, medium_arm)
    }

    /// Ports solc's `checked_exp_signed` Yul helper: the first squaring is
    /// pulled out because it is the only one with a possibly negative base;
    /// afterwards the shared unsigned loop applies and only the final multiply
    /// needs sign-aware bounds.
    fn lower_checked_pow_signed(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        exponent: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        let min_imm = builder.imm_u256(Self::signed_min(info.bits));
        let max_imm = builder.imm_u256(Self::signed_max(info.bits));
        let zero = builder.imm_u64(0);
        let one = builder.imm_u64(1);

        let join = builder.create_block();
        let mut results: Vec<(BlockId, ValueId)> = Vec::new();

        // if iszero(exponent) { power := 1 } (note that 0**0 == 1)
        let exp_one_block = builder.create_block();
        let exp_zero = builder.iszero(exponent);
        results.push((builder.current_block(), one));
        builder.branch(exp_zero, join, exp_one_block);

        // if eq(exponent, 1) { power := base }
        builder.switch_to_block(exp_one_block);
        let base_zero_block = builder.create_block();
        let exp_one = builder.eq(exponent, one);
        results.push((builder.current_block(), base));
        builder.branch(exp_one, join, base_zero_block);

        // if iszero(base) { power := 0 }
        builder.switch_to_block(base_zero_block);
        let first_square_block = builder.create_block();
        let base_zero = builder.iszero(base);
        results.push((builder.current_block(), zero));
        builder.branch(base_zero, join, first_square_block);

        // First squaring, pulled out because base can still be negative here.
        // Exponent is at least 2. Overflow check for base * base:
        // positive base panics iff `base > max / base`, negative base panics
        // iff `base < max / base` (both sides signed; the quotient is
        // `-(max / |base|)`, so the comparison is `|base| > max / |base|`).
        builder.switch_to_block(first_square_block);
        let positive_check = builder.create_block();
        let negative_check = builder.create_block();
        let square_block = builder.create_block();
        let base_positive = builder.sgt(base, zero);
        builder.branch(base_positive, positive_check, negative_check);

        // Bases 1 and -1 short-circuit before the squaring loop: their powers
        // are exactly `1` and `parity(exponent) ? -1 : 1` and can never panic.
        // solc instead runs its loop (~log2(exponent) iterations of no-op
        // squarings); the shortcut costs one comparison on the general path
        // and caps the worst case (huge exponents) far below solc.
        builder.switch_to_block(positive_check);
        let positive_small_check = builder.create_block();
        let base_is_one = builder.eq(base, one);
        results.push((builder.current_block(), one));
        builder.branch(base_is_one, join, positive_small_check);

        // Positive bases reuse the unsigned small-base specialization (solc
        // does not, but it is provably equivalent): within the bounds the
        // native power is exact, and for `exponent >= 2` it panics iff the
        // result exceeds max — `base * base <= power` makes the squaring
        // check redundant with the range check on the result.
        builder.switch_to_block(positive_small_check);
        let positive_native = builder.create_block();
        let positive_loop = builder.create_block();
        let use_native = Self::small_base_exp_is_exact(builder, base, exponent);
        builder.branch(use_native, positive_native, positive_loop);

        builder.switch_to_block(positive_native);
        let power = builder.exp(base, exponent);
        let out_of_range = builder.gt(power, max_imm);
        self.emit_panic_if(builder, out_of_range, PanicCode::ArithmeticOverflowUnderflow);
        results.push((builder.current_block(), power));
        builder.jump(join);

        builder.switch_to_block(positive_loop);
        let quotient = builder.div(max_imm, base);
        let overflow = builder.gt(base, quotient);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
        builder.jump(square_block);

        builder.switch_to_block(negative_check);
        let minus_one_block = builder.create_block();
        let negative_loop = builder.create_block();
        let minus_one = builder.imm_u256(U256::MAX);
        let base_is_minus_one = builder.eq(base, minus_one);
        builder.branch(base_is_minus_one, minus_one_block, negative_loop);

        builder.switch_to_block(minus_one_block);
        let exp_odd = builder.and(exponent, one);
        let parity_power = builder.select(exp_odd, minus_one, one);
        results.push((builder.current_block(), parity_power));
        builder.jump(join);

        builder.switch_to_block(negative_loop);
        let quotient = builder.sdiv(max_imm, base);
        let overflow = builder.slt(base, quotient);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
        builder.jump(square_block);

        builder.switch_to_block(square_block);
        // if and(exponent, 1) { power := base } (power starts at 1)
        let exp_odd = builder.and(exponent, one);
        let power = builder.select(exp_odd, base, one);
        let squared = builder.mul(base, base);
        let halved = builder.shr(one, exponent);

        // Below this point, base is always positive.
        let (power, base) = self.emit_checked_exp_loop(builder, power, squared, halved, max_imm);

        // Final multiply with sign-aware bounds: positive power panics iff
        // `power * base > max`, negative power panics iff `power * base < min`.
        let power_positive = builder.sgt(power, zero);
        let quotient = builder.div(max_imm, base);
        let above_max = builder.gt(power, quotient);
        let overflow = builder.and(power_positive, above_max);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
        let power_negative = builder.slt(power, zero);
        let quotient = builder.sdiv(min_imm, base);
        let below_min = builder.slt(power, quotient);
        let underflow = builder.and(power_negative, below_min);
        self.emit_panic_if(builder, underflow, PanicCode::ArithmeticOverflowUnderflow);
        let power = builder.mul(power, base);
        results.push((builder.current_block(), power));
        builder.jump(join);

        builder.switch_to_block(join);
        builder.phi(results)
    }

    /// Ports solc's `checked_exp_helper` squaring loop. The only overflow
    /// check needed is for `base * base`: `|power| <= base` holds by
    /// induction, so `|power * base| <= base * base <= max` (equally true for
    /// the signed caller, whose `power` may be negative but whose `base` is
    /// already positive). The final multiply is left to the caller. Returns
    /// the `(power, base)` values; the builder ends up in the exit block.
    fn emit_checked_exp_loop(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        power_init: ValueId,
        base_init: ValueId,
        exponent_init: ValueId,
        max_imm: ValueId,
    ) -> (ValueId, ValueId) {
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);

        // for { } gt(exponent, 1) { }
        builder.switch_to_block(header);
        let power_phi = builder.phi(vec![(preheader, power_init)]);
        let base_phi = builder.phi(vec![(preheader, base_init)]);
        let exp_phi = builder.phi(vec![(preheader, exponent_init)]);
        let one = builder.imm_u64(1);
        let has_more = builder.gt(exp_phi, one);
        builder.branch(has_more, body, exit);

        builder.switch_to_block(body);
        // Overflow check for base * base.
        let quotient = builder.div(max_imm, base_phi);
        let overflow = builder.gt(base_phi, quotient);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
        // if and(exponent, 1) { power := mul(power, base) }. The product is
        // computed unconditionally but only selected when the exponent bit is
        // set, in which case the `base * base` check above proves it exact
        // (`|power * base| <= base * base <= max`); otherwise its (possibly
        // wrapped) value is dropped.
        let exp_odd = builder.and(exp_phi, one);
        let multiplied = builder.mul(power_phi, base_phi);
        let power_next = builder.select(exp_odd, multiplied, power_phi);
        let base_next = builder.mul(base_phi, base_phi);
        let exp_next = builder.shr(one, exp_phi);
        let latch = builder.current_block();
        builder.jump(header);

        builder.add_phi_incoming(power_phi, latch, power_next);
        builder.add_phi_incoming(base_phi, latch, base_next);
        builder.add_phi_incoming(exp_phi, latch, exp_next);

        builder.switch_to_block(exit);
        (power_phi, base_phi)
    }

    fn unsigned_add_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        result: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        if info.bits == 256 {
            // The add wrapped iff the result is smaller than an operand.
            return builder.lt(result, lhs);
        }
        // In-range n-bit operands cannot wrap a 256-bit add (their sum is
        // below `2 * 2^n <= 2^256`), so a range check on the result is exact.
        let max = builder.imm_u256(Self::unsigned_max(info.bits));
        builder.gt(result, max)
    }

    fn unsigned_mul_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        if info.bits <= 128 {
            // The 256-bit product of in-range operands (each below `2^128`)
            // cannot wrap, so a range check on the result is exact.
            let max = builder.imm_u256(Self::unsigned_max(info.bits));
            return builder.gt(result, max);
        }
        // Division-inverse check: the product is exact iff `rhs == 0` or
        // `result / rhs == lhs`.
        let rhs_zero = builder.iszero(rhs);
        let quotient = builder.div(result, rhs);
        let inverse_ok = builder.eq(quotient, lhs);
        let exact = builder.or(rhs_zero, inverse_ok);
        let wrapped = builder.iszero(exact);
        if info.bits == 256 {
            return wrapped;
        }
        // Sub-word products can also stay within 256 bits while exceeding the
        // n-bit range.
        let max = builder.imm_u256(Self::unsigned_max(info.bits));
        let out_of_range = builder.gt(result, max);
        builder.or(wrapped, out_of_range)
    }

    fn signed_add_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        if info.bits != 256 {
            // Sign-extended sub-word operands cannot wrap a 256-bit add, so a
            // range check on the result is exact.
            return self.signed_range_overflow(builder, result, info);
        }
        // For `lhs >= 0` the add overflowed iff `result < rhs`; for `lhs < 0`
        // it overflowed iff `result >= rhs`. Both comparisons yield 0/1, so
        // the overflow flag is their XOR.
        let zero = builder.imm_u64(0);
        let lhs_neg = builder.slt(lhs, zero);
        let wrapped = builder.slt(result, rhs);
        builder.xor(lhs_neg, wrapped)
    }

    fn signed_sub_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        if info.bits != 256 {
            // Sign-extended sub-word operands cannot wrap a 256-bit sub, so a
            // range check on the result is exact.
            return self.signed_range_overflow(builder, result, info);
        }
        // For `rhs >= 0` the sub overflowed iff `result > lhs`; for `rhs < 0`
        // it overflowed iff `result < lhs` (`result == lhs` requires
        // `rhs == 0`, so `iszero(sgt)` is exact in the negative case). Both
        // comparisons yield 0/1, so the overflow flag is their XOR.
        let zero = builder.imm_u64(0);
        let rhs_neg = builder.slt(rhs, zero);
        let grew = builder.sgt(result, lhs);
        builder.xor(rhs_neg, grew)
    }

    fn signed_mul_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        if info.bits <= 128 {
            // The 256-bit product of sign-extended operands of at most 128
            // bits stays below `2^254` in magnitude, so it cannot wrap and a
            // range check on the result is exact.
            return self.signed_range_overflow(builder, result, info);
        }
        // Division-inverse check: the product is exact iff `rhs == 0` or
        // `result / rhs == lhs`.
        let rhs_zero = builder.iszero(rhs);
        let quotient = builder.sdiv(result, rhs);
        let inverse_ok = builder.eq(quotient, lhs);
        let exact = builder.or(rhs_zero, inverse_ok);
        let wrapped = builder.iszero(exact);
        if info.bits != 256 {
            // Sub-word products can also stay within 256 bits while exceeding
            // the n-bit range. The `sdiv(MIN, -1)` anomaly cannot fire here:
            // `result == MIN_256 && rhs == -1` requires `lhs == MIN_256`,
            // which is not an in-range sub-word value.
            let range = self.signed_range_overflow(builder, result, info);
            return builder.or(wrapped, range);
        }
        // The division check misses exactly `lhs == MIN && rhs == -1`, where
        // EVM defines `sdiv(result == MIN, -1) == MIN == lhs`. Cover it with
        // the superset `lhs == MIN && rhs < 0`, all of which truly overflow.
        let min = builder.imm_u256(Self::signed_min(info.bits));
        let lhs_min = builder.eq(lhs, min);
        let zero = builder.imm_u64(0);
        let rhs_neg = builder.slt(rhs, zero);
        let min_overflow = builder.and(lhs_min, rhs_neg);
        builder.or(wrapped, min_overflow)
    }

    fn signed_range_overflow(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        info: IntegerInfo,
    ) -> ValueId {
        let min = builder.imm_u256(Self::signed_min(info.bits));
        let max = builder.imm_u256(Self::signed_max(info.bits));
        let below_min = builder.slt(value, min);
        let above_max = builder.sgt(value, max);
        builder.or(below_min, above_max)
    }

    fn emit_signed_min_div_minus_one_check(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        rhs: ValueId,
        info: IntegerInfo,
    ) {
        let min = builder.imm_u256(Self::signed_min(info.bits));
        let minus_one = builder.imm_u256(U256::MAX);
        let is_min = builder.eq(lhs, min);
        let is_minus_one = builder.eq(rhs, minus_one);
        let overflow = builder.and(is_min, is_minus_one);
        self.emit_panic_if(builder, overflow, PanicCode::ArithmeticOverflowUnderflow);
    }

    /// Emits an array bounds check: `if (!(index < len)) Panic(0x32)`.
    ///
    /// Constant operands fold at lowering: a provably in-range constant index
    /// emits no check at all, and a provably out-of-range constant index
    /// emits an unconditional panic (matching solc's runtime semantics for
    /// out-of-bounds accesses), as a constant branch that later passes fold.
    pub(super) fn emit_index_bounds_check(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        index: ValueId,
        len: ValueId,
    ) {
        if let (Some(index_const), Some(len_const)) =
            (Self::const_u256_of(builder, index), Self::const_u256_of(builder, len))
        {
            if index_const < len_const {
                return;
            }
            let always = builder.imm_bool(true);
            self.emit_panic_if(builder, always, PanicCode::ArrayOutOfBounds);
            return;
        }
        let in_range = builder.lt(index, len);
        self.emit_panic_if_zero(builder, in_range, PanicCode::ArrayOutOfBounds);
    }

    /// Returns the constant value of a MIR immediate, if `value` is one.
    fn const_u256_of(builder: &FunctionBuilder<'_>, value: ValueId) -> Option<U256> {
        match builder.func().value(value) {
            crate::mir::Value::Immediate(imm) => imm.as_u256(),
            _ => None,
        }
    }

    pub(super) fn emit_panic_if_zero(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        code: PanicCode,
    ) {
        // Branch directly on the value: zero falls into the revert block
        // without materializing an `iszero`/`eq` flag.
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(value, continue_block, revert_block);

        builder.switch_to_block(revert_block);
        self.emit_panic_revert(builder, code);

        builder.switch_to_block(continue_block);
    }

    pub(super) fn emit_panic_if(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: ValueId,
        code: PanicCode,
    ) {
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(cond, revert_block, continue_block);

        builder.switch_to_block(revert_block);
        self.emit_panic_revert(builder, code);

        builder.switch_to_block(continue_block);
    }

    pub(super) fn emit_panic_revert(&mut self, builder: &mut FunctionBuilder<'_>, code: PanicCode) {
        let selector = U256::from(0x4e48_7b71u64) << 224;
        let selector = builder.imm_u256(selector);
        let zero = builder.imm_u64(0);
        builder.mstore(zero, selector);
        let code_offset = builder.imm_u64(4);
        let code = builder.imm_u64(code.as_u64());
        builder.mstore(code_offset, code);
        let size = builder.imm_u64(36);
        builder.revert(zero, size);
    }

    pub(super) fn unsigned_max(bits: u16) -> U256 {
        if bits >= 256 { U256::MAX } else { (U256::from(1) << bits) - U256::from(1) }
    }

    pub(super) fn signed_min(bits: u16) -> U256 {
        U256::MAX - (U256::from(1) << (bits - 1)) + U256::from(1)
    }

    pub(super) fn signed_max(bits: u16) -> U256 {
        (U256::from(1) << (bits - 1)) - U256::from(1)
    }

    /// Lowers a unary operation.
    pub(super) fn lower_unary_op(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        op: hir::UnOp,
        operand: ValueId,
        int_info: Option<IntegerInfo>,
        span: Span,
    ) -> ValueId {
        use hir::UnOpKind;

        match op.kind {
            UnOpKind::Not => builder.iszero(operand),
            UnOpKind::BitNot => builder.not(operand),
            UnOpKind::Neg => {
                let zero = builder.imm_u256(U256::ZERO);
                if !self.in_unchecked_block {
                    let int_info = self.require_checked_arithmetic_info(int_info, span);
                    if let Some(info) = int_info
                        && info.signed
                    {
                        let min = builder.imm_u256(Self::signed_min(info.bits));
                        let overflow = builder.eq(operand, min);
                        self.emit_panic_if(
                            builder,
                            overflow,
                            PanicCode::ArithmeticOverflowUnderflow,
                        );
                    }
                }
                builder.sub(zero, operand)
            }
            UnOpKind::PreInc | UnOpKind::PostInc => {
                let one = builder.imm_u64(1);
                builder.add(operand, one)
            }
            UnOpKind::PreDec | UnOpKind::PostDec => {
                let one = builder.imm_u64(1);
                builder.sub(operand, one)
            }
        }
    }
}
