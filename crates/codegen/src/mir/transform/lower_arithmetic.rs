//! Expand checked arithmetic after semantic optimization, before scalar cleanup.
//!
//! Emit explicit Solidity zero/overflow checks and exponentiation-by-squaring loops. Preserve
//! checked widths and signedness; wrapping signed division still checks zero. Splitting blocks
//! redirects successor phi predecessors locally, and every emitted operation inherits the source
//! instruction's debug context. Later cleanup can combine exposed checks and scalar expressions.

use crate::mir::{
    ArithmeticKind, CheckedOp, FunctionBuilder, InstKind, Module, PanicCode, ValueId,
    pass::{MirPass, run_function_pass},
    transform::utils::redirect_successor_predecessors,
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
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> solar_interface::Result<bool> {
        Ok(run_function_pass(module, analyses, |func, _| {
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
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let (terminator, metadata) = func.blocks[block].take_terminator();
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                for id in instructions {
                    let InstKind::CheckedBinary { op, arithmetic, lhs, rhs } =
                        builder.func().inst(id).kind
                    else {
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(id);
                        continue;
                    };
                    let context = builder.func().inst(id).metadata.debug_context();
                    builder.set_debug_context(&context);
                    // result = scalar arithmetic(lhs, rhs); check overflow/zero
                    let result = ArithmeticLowerer { builder: &mut builder }
                        .binary(op, lhs, rhs, arithmetic);
                    replacements.insert(
                        builder.func().inst_result_value(id).expect("arithmetic result"),
                        result,
                    );
                }
                // continuation: remaining instructions; original terminator
                let end = builder.current_block();
                if let Some(terminator) = terminator {
                    builder.func_mut().blocks[end].set_terminator(terminator, metadata);
                }
                if end != block {
                    redirect_successor_predecessors(builder.func_mut(), block, end);
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        }))
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
        let sign_condition = if is_add { self.builder.iszero(signs_differ) } else { signs_differ };
        let mut overflow = self.builder.and(sign_condition, result_changed_sign);
        if bits < 256 {
            // overflow |= result < min || result > max
            let (min, max) = signed_bounds(bits, self.builder);
            overflow = self.add_signed_range_check(overflow, result, min, max);
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
        // valid = rhs == 0 || (signed ? sdiv : div)(result, rhs) == lhs
        // overflow = !valid
        let rhs_zero = self.builder.iszero(rhs);
        let quotient = match kind {
            ArithmeticKind::Unsigned(_) => self.builder.div(result, rhs),
            ArithmeticKind::Signed(_) => self.builder.sdiv(result, rhs),
        };
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let mut overflow = self.builder.iszero(valid);
        if let ArithmeticKind::Signed(bits) = kind {
            // overflow |= result < min || result > max
            // overflow |= lhs == min && rhs == -1
            let (min, max) = signed_bounds(bits, self.builder);
            overflow = self.add_signed_range_check(overflow, result, min, max);
            let minus_one = self.builder.imm(U256::MAX);
            let lhs_is_min = self.builder.eq(lhs, min);
            let rhs_is_minus_one = self.builder.eq(rhs, minus_one);
            let special = self.builder.and(lhs_is_min, rhs_is_minus_one);
            overflow = self.builder.or(overflow, special);
        } else if let ArithmeticKind::Unsigned(bits) = kind
            && bits < 256
        {
            // overflow |= result > max
            let max = self.builder.imm((U256::from(1) << bits) - U256::ONE);
            let too_wide = self.builder.gt(result, max);
            overflow = self.builder.or(overflow, too_wide);
        }
        overflow
    }

    fn add_signed_range_check(
        &mut self,
        overflow: ValueId,
        result: ValueId,
        min: ValueId,
        max: ValueId,
    ) -> ValueId {
        let below = self.builder.slt(result, min);
        let above = self.builder.sgt(result, max);
        let out_of_range = self.builder.or(below, above);
        self.builder.or(overflow, out_of_range)
    }

    fn checked_pow(&mut self, base: ValueId, exponent: ValueId, kind: ArithmeticKind) -> ValueId {
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

        let next_exponent = self.builder.shr(one, current_exponent);
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
        match op {
            CheckedOp::Add => {
                // result = add lhs, rhs
                // panic_if overflow(result, lhs, rhs)
                let result = self.builder.add(lhs, rhs);
                let overflow = match kind {
                    ArithmeticKind::Unsigned(256) => self.builder.lt(result, lhs),
                    ArithmeticKind::Unsigned(bits) => {
                        let max = self.builder.imm((U256::ONE << bits) - U256::ONE);
                        self.builder.gt(result, max)
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
