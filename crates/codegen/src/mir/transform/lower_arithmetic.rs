//! Expand checked arithmetic after semantic optimization, before scalar cleanup.
//!
//! Emit explicit Solidity zero/overflow checks and exponentiation-by-squaring loops. Preserve
//! checked widths and signedness; wrapping signed division still checks zero. As in solc, a
//! product of operands of at most 128 bits only checks its range, wider ones also test the
//! quotient, and only a 256-bit signed product tests `min * -1`, which the quotient misses.
//! A constant nonnegative base needs no loop: its powers grow monotonically, so the exponent
//! overflows exactly when it exceeds the largest one whose power fits, and the power is one
//! `exp`. A negative base keeps the loop, as its powers alternate in sign.
//! Splitting blocks redirects successor phi predecessors locally, and every emitted operation
//! inherits the source instruction's debug context. Later cleanup can combine exposed checks and
//! scalar expressions.

use crate::mir::{
    ArithmeticKind, CheckedOp, ConditionUse, FunctionBuilder, InstKind, MirType, Module, PanicCode,
    ValueId,
    analysis::integers::integer_mask,
    pass::{MirPass, run_function_pass},
    transform::utils::expand_block,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

pub(crate) struct LowerArithmetic;

impl MirPass for LowerArithmetic {
    fn name(&self) -> &'static str {
        "lower-arithmetic"
    }
    fn is_required(&self) -> bool {
        true
    }
    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            if !func
                .instructions()
                .any(|id| matches!(func.inst(id).kind, InstKind::CheckedBinary { .. }))
            {
                return false;
            }
            let mut replacements = FxHashMap::default();
            for block in func.blocks.indices() {
                if !func.blocks[block]
                    .instructions
                    .iter()
                    .any(|&id| matches!(func.inst(id).kind, InstKind::CheckedBinary { .. }))
                {
                    continue;
                }
                let mut builder = FunctionBuilder::new(func).with_bit_tests(analyses.target(gcx));
                expand_block(&mut builder, block, |builder, id| {
                    let InstKind::CheckedBinary { op, arithmetic, lhs, rhs } =
                        builder.func().inst(id).kind
                    else {
                        return false;
                    };
                    let context = builder.func().inst(id).metadata.debug_context();
                    builder.set_debug_context(&context);
                    // result = scalar arithmetic(lhs, rhs); check overflow/zero
                    let result = ArithmeticLowerer { builder: &mut *builder }
                        .binary(op, lhs, rhs, arithmetic);
                    let result = builder.cast(result, arithmetic.ty().mir_type());
                    replacements.insert(
                        builder.func().inst_result_value(id).expect("arithmetic result"),
                        result,
                    );
                    true
                });
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

struct ArithmeticLowerer<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
}

impl ArithmeticLowerer<'_, '_> {
    fn signed_add_sub_overflow(
        &mut self,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        bits: u16,
        is_add: bool,
    ) -> ValueId {
        // overflow = signed_add_sub_signs(lhs, rhs, result)
        let zero = self.builder.imm(U256::ZERO);
        let lhs_negative = self.builder.slt(lhs, zero);
        let rhs_negative = self.builder.slt(rhs, zero);
        let result_negative = self.builder.slt(result, zero);
        let signs_differ = self.builder.xor(lhs_negative, rhs_negative);
        let result_changed_sign = self.builder.xor(result_negative, lhs_negative);
        let sign_condition = if is_add { self.builder.eq_zero(signs_differ) } else { signs_differ };
        let mut overflow = self.builder.and(sign_condition, result_changed_sign);
        if bits < 256 {
            // overflow |= result < min || result > max
            let out_of_range =
                self.range_overflow(result, ArithmeticKind::Signed(bits), ConditionUse::VALUE);
            overflow = self.builder.or(overflow, out_of_range);
        }
        overflow
    }

    fn mul_overflow(
        &mut self,
        lhs: ValueId,
        rhs: ValueId,
        result: ValueId,
        kind: ArithmeticKind,
    ) -> ValueId {
        let (ArithmeticKind::Unsigned(bits) | ArithmeticKind::Signed(bits)) = kind;
        if bits <= 128 {
            // The exact product of two such operands fits in a word, so only its range can
            // overflow, as in solc.
            return self.range_overflow(result, kind, ConditionUse::BRANCH);
        }
        // valid = rhs == 0 || (signed ? sdiv : div)(result, rhs) == lhs
        // overflow = !valid
        let rhs_zero = self.builder.eq_zero(rhs);
        let quotient = match kind {
            ArithmeticKind::Unsigned(_) => self.builder.div(result, rhs),
            ArithmeticKind::Signed(_) => self.builder.sdiv(result, rhs),
        };
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let mut overflow = self.builder.eq_zero(valid);
        if bits < 256 {
            // overflow |= result out of range
            let out_of_range = self.range_overflow(result, kind, ConditionUse::VALUE);
            overflow = self.builder.or(overflow, out_of_range);
        } else if let ArithmeticKind::Signed(_) = kind {
            // `sdiv(min, -1)` wraps to `min`, so the quotient test accepts `min * -1`.
            // overflow |= lhs == min && rhs < 0
            let (min, _) = signed_bounds(bits, self.builder);
            let zero = self.builder.imm(U256::ZERO);
            let lhs_is_min = self.builder.eq(lhs, min);
            let rhs_negative = self.builder.slt(rhs, zero);
            let special = self.builder.and(lhs_is_min, rhs_negative);
            overflow = self.builder.or(overflow, special);
        }
        overflow
    }

    /// Returns whether a `bits`-wide `result` lies outside the range of `kind`.
    fn range_overflow(
        &mut self,
        result: ValueId,
        kind: ArithmeticKind,
        read: ConditionUse,
    ) -> ValueId {
        match kind {
            ArithmeticKind::Unsigned(bits) => {
                // result > max
                self.builder.exceeds_bits(result, bits.into(), read)
            }
            ArithmeticKind::Signed(bits) => {
                // result < min || result > max
                let (min, max) = signed_bounds(bits, self.builder);
                let below = self.builder.slt(result, min);
                let above = self.builder.sgt(result, max);
                self.builder.or(below, above)
            }
        }
    }

    fn checked_pow(&mut self, base: ValueId, exponent: ValueId, kind: ArithmeticKind) -> ValueId {
        if let Some(constant) = self.builder.func().value_u256(base) {
            let max = match kind {
                ArithmeticKind::Unsigned(bits) => Some(integer_mask(bits.into())),
                ArithmeticKind::Signed(bits) if !constant.bit(255) => {
                    Some(integer_mask(u32::from(bits) - 1))
                }
                ArithmeticKind::Signed(_) => None,
            };
            if let Some(max) = max {
                // Every power of 0 and 1 fits.
                if constant > U256::ONE {
                    // panic_if exponent > limit
                    let limit = self.builder.imm(U256::from(power_limit(constant, max)));
                    let overflow = self.builder.gt(exponent, limit);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                // power = exp base, exponent
                return self.builder.exp(base, exponent);
            }
        }
        // power = 1
        // current_base = base
        // current_exponent = exponent
        // while current_exponent > 0 {
        //     if odd { power = checked_mul(power, current_base) }
        //     if current_exponent >> 1 > 0 {
        //         current_base = checked_mul(current_base, current_base)
        //     }
        //     current_exponent >>= 1
        // }
        let one = self.builder.imm(U256::ONE);
        let zero = self.builder.imm(U256::ZERO);
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let power = self.builder.phi(vec![(preheader, one)]);
        let current_base = self.builder.phi(vec![(preheader, base)]);
        let current_exponent = self.builder.phi(vec![(preheader, exponent)]);
        let has_exponent = self.builder.gt(current_exponent, zero);
        self.builder.branch(has_exponent, body, exit);

        self.builder.switch_to_block(body);
        let odd = self.builder.and(current_exponent, one);
        let product = self.builder.mul(power, current_base);
        let product_overflow = self.mul_overflow(power, current_base, product, kind);
        let product_check = self.builder.and(odd, product_overflow);
        self.builder.panic_if(product_check, PanicCode::ArithmeticOverflowUnderflow);
        let next_power = self.builder.select(odd, product, power);

        let next_exponent = self.builder.shr(current_exponent, one);
        let square = self.builder.mul(current_base, current_base);
        let square_overflow = self.mul_overflow(current_base, current_base, square, kind);
        let has_next_exponent = self.builder.gt(next_exponent, zero);
        let square_check = self.builder.and(has_next_exponent, square_overflow);
        self.builder.panic_if(square_check, PanicCode::ArithmeticOverflowUnderflow);
        let latch = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(power, latch, next_power);
        self.builder.add_phi_incoming(current_base, latch, square);
        self.builder.add_phi_incoming(current_exponent, latch, next_exponent);

        self.builder.switch_to_block(exit);
        power
    }

    fn binary(
        &mut self,
        op: CheckedOp,
        lhs: ValueId,
        rhs: ValueId,
        kind: ArithmeticKind,
    ) -> ValueId {
        let widen = |builder: &mut FunctionBuilder<'_>, value| {
            if matches!(kind, ArithmeticKind::Signed(_))
                && let Some(bits) = builder.func().value_ty(value).and_then(MirType::integer_bits)
                && bits < 256
            {
                builder.emit_inst(InstKind::Sext(value, bits, 256), Some(MirType::I256))
            } else {
                builder.cast_word(value)
            }
        };
        let lhs = widen(self.builder, lhs);
        let rhs = if op == CheckedOp::Pow {
            self.builder.cast_word(rhs)
        } else {
            widen(self.builder, rhs)
        };
        match op {
            CheckedOp::Add => {
                // result = add lhs, rhs
                // panic_if overflow(result, lhs, rhs)
                let result = self.builder.add(lhs, rhs);
                let overflow = match kind {
                    ArithmeticKind::Unsigned(256) => self.builder.lt(result, lhs),
                    ArithmeticKind::Unsigned(_) => {
                        self.range_overflow(result, kind, ConditionUse::BRANCH)
                    }
                    ArithmeticKind::Signed(bits) => {
                        self.signed_add_sub_overflow(lhs, rhs, result, bits, true)
                    }
                };
                self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                result
            }
            CheckedOp::Sub => {
                if let ArithmeticKind::Signed(bits) = kind
                    && self.builder.func().value_u256(lhs) == Some(U256::ZERO)
                {
                    // panic_if rhs == signed_min
                    // result = sub 0, rhs
                    let (min, _) = signed_bounds(bits, self.builder);
                    let overflow = self.builder.eq(rhs, min);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                    return self.builder.sub(lhs, rhs);
                }
                // result = sub lhs, rhs
                // panic_if overflow(result, lhs, rhs)
                let result = self.builder.sub(lhs, rhs);
                let overflow = match kind {
                    ArithmeticKind::Unsigned(_) => self.builder.lt(lhs, rhs),
                    ArithmeticKind::Signed(bits) => {
                        self.signed_add_sub_overflow(lhs, rhs, result, bits, false)
                    }
                };
                self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                result
            }
            CheckedOp::Mul => {
                // result = mul lhs, rhs
                // panic_if overflow(result, lhs, rhs)
                let result = self.builder.mul(lhs, rhs);
                let overflow = self.mul_overflow(lhs, rhs, result, kind);
                self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                result
            }
            CheckedOp::Div | CheckedOp::WrappingDiv => {
                // panic_if rhs == 0
                // panic_if checked && lhs == signed_min && rhs == -1
                // result = div/sdiv lhs, rhs
                self.builder.panic_if_zero(rhs, PanicCode::DivisionByZero);
                if op == CheckedOp::Div
                    && let ArithmeticKind::Signed(bits) = kind
                {
                    let (min, _) = signed_bounds(bits, self.builder);
                    let lhs_is_min = self.builder.eq(lhs, min);
                    let minus_one = self.builder.imm(U256::MAX);
                    let rhs_is_minus_one = self.builder.eq(rhs, minus_one);
                    let overflow = self.builder.and(lhs_is_min, rhs_is_minus_one);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                let result = match kind {
                    ArithmeticKind::Signed(_) => self.builder.sdiv(lhs, rhs),
                    ArithmeticKind::Unsigned(_) => self.builder.div(lhs, rhs),
                };
                if op == CheckedOp::WrappingDiv
                    && let ArithmeticKind::Signed(bits) = kind
                    && bits < 256
                {
                    // result = signextend(width - 1, result)
                    let byte = self.builder.imm(u64::from(bits / 8 - 1));
                    self.builder.signextend(byte, result)
                } else {
                    result
                }
            }
            CheckedOp::Rem => {
                // panic_if rhs == 0
                // result = mod/smod lhs, rhs
                self.builder.panic_if_zero(rhs, PanicCode::DivisionByZero);
                match kind {
                    ArithmeticKind::Signed(_) => self.builder.smod(lhs, rhs),
                    ArithmeticKind::Unsigned(_) => self.builder.mod_(lhs, rhs),
                }
            }
            CheckedOp::Pow => self.checked_pow(lhs, rhs, kind),
        }
    }
}

fn signed_bounds(bits: u16, builder: &mut FunctionBuilder<'_>) -> (ValueId, ValueId) {
    let magnitude = U256::from(1) << (bits - 1);
    let min = builder.imm(U256::MAX - magnitude + U256::ONE);
    let max = builder.imm(magnitude - U256::ONE);
    (min, max)
}

/// The largest exponent whose power of `base`, at least 2, is at most `max`.
fn power_limit(base: U256, max: U256) -> u32 {
    let mut limit = 0;
    let mut power = U256::ONE;
    while let Some(next) = power.checked_mul(base)
        && next <= max
    {
        power = next;
        limit += 1;
    }
    limit
}
