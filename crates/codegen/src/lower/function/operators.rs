//! Checked arithmetic and scalar operator lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn signed_add_sub_overflow(
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
            let (min, max) = signed_bounds(bits, &mut self.builder);
            overflow = self.add_signed_range_check(overflow, result, min, max);
        }
        overflow
    }

    pub(super) fn mul_overflow(
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
            let (min, max) = signed_bounds(bits, &mut self.builder);
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

    pub(super) fn truncate_wrapping_result(
        &mut self,
        value: ValueId,
        kind: Option<ArithmeticKind>,
    ) -> ValueId {
        match kind {
            Some(ArithmeticKind::Unsigned(bits)) if bits < 256 => self.mask_to_bits(value, bits),
            Some(ArithmeticKind::Signed(bits)) if (8..256).contains(&bits) => {
                let byte = self.builder.imm(u64::from(bits / 8 - 1));
                self.builder.signextend(byte, value)
            }
            _ => value,
        }
    }

    pub(super) fn mask_to_bits(&mut self, value: ValueId, bits: u16) -> ValueId {
        if bits >= 256 {
            return value;
        }
        let mask = self.builder.imm((U256::from(1) << bits) - U256::ONE);
        self.builder.and(value, mask)
    }

    pub(super) fn clean_fixed_bytes(&mut self, value: ValueId, bytes: u8) -> ValueId {
        if bytes >= 32 {
            return value;
        }
        let mask = self.builder.imm(U256::MAX << (256 - usize::from(bytes) * 8));
        self.builder.and(value, mask)
    }

    pub(super) fn checked_pow(
        &mut self,
        base: ValueId,
        exponent: ValueId,
        kind: ArithmeticKind,
    ) -> ValueId {
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

    pub(super) fn binary(
        &mut self,
        op: BinOpKind,
        lhs: ValueId,
        rhs: ValueId,
        ty: Option<Ty<'gcx>>,
    ) -> ValueId {
        let arithmetic = ty.and_then(arithmetic_kind);
        match op {
            BinOpKind::Add => {
                let result = self.builder.add(lhs, rhs);
                if self.unchecked {
                    return self.truncate_wrapping_result(result, arithmetic);
                }
                if let Some(kind) = arithmetic {
                    let overflow = match kind {
                        ArithmeticKind::Unsigned(bits) => {
                            if bits == 256 {
                                self.builder.lt(result, lhs)
                            } else {
                                let max =
                                    self.builder.imm((U256::from(1) << bits) - U256::ONE);
                                self.builder.gt(result, max)
                            }
                        }
                        ArithmeticKind::Signed(bits) => {
                            self.signed_add_sub_overflow(lhs, rhs, result, bits, true)
                        }
                    };
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                result
            }
            BinOpKind::Sub => {
                let result = self.builder.sub(lhs, rhs);
                if self.unchecked {
                    return self.truncate_wrapping_result(result, arithmetic);
                }
                if let Some(kind) = arithmetic {
                    let overflow = match kind {
                        ArithmeticKind::Unsigned(_) => self.builder.lt(lhs, rhs),
                        ArithmeticKind::Signed(bits) => {
                            self.signed_add_sub_overflow(lhs, rhs, result, bits, false)
                        }
                    };
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                result
            }
            BinOpKind::Mul => {
                let result = self.builder.mul(lhs, rhs);
                if self.unchecked {
                    return self.truncate_wrapping_result(result, arithmetic);
                }
                if let Some(kind) = arithmetic {
                    let overflow = self.mul_overflow(lhs, rhs, result, kind);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                result
            }
            BinOpKind::Div => {
                self.builder.panic_if_zero(rhs, PanicCode::DivisionByZero);
                if !self.unchecked
                    && let Some(ArithmeticKind::Signed(bits)) = arithmetic
                {
                    let (min, _) = signed_bounds(bits, &mut self.builder);
                    let lhs_is_min = self.builder.eq(lhs, min);
                    let minus_one = self.builder.imm(U256::MAX);
                    let rhs_is_minus_one = self.builder.eq(rhs, minus_one);
                    let overflow = self.builder.and(lhs_is_min, rhs_is_minus_one);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                let result = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.sdiv(lhs, rhs),
                    _ => self.builder.div(lhs, rhs),
                };
                if self.unchecked && matches!(arithmetic, Some(ArithmeticKind::Signed(_))) {
                    return self.truncate_wrapping_result(result, arithmetic);
                }
                result
            }
            BinOpKind::Rem => {
                self.builder.panic_if_zero(rhs, PanicCode::DivisionByZero);
                match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.smod(lhs, rhs),
                    _ => self.builder.mod_(lhs, rhs),
                }
            }
            BinOpKind::Lt => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.slt(lhs, rhs),
                _ => self.builder.lt(lhs, rhs),
            },
            BinOpKind::Gt => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.sgt(lhs, rhs),
                _ => self.builder.gt(lhs, rhs),
            },
            BinOpKind::Eq => self.builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = self.builder.eq(lhs, rhs);
                self.builder.iszero(eq)
            }
            BinOpKind::Le => {
                let gt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.sgt(lhs, rhs),
                    _ => self.builder.gt(lhs, rhs),
                };
                self.builder.iszero(gt)
            }
            BinOpKind::Ge => {
                let lt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.slt(lhs, rhs),
                    _ => self.builder.lt(lhs, rhs),
                };
                self.builder.iszero(lt)
            }
            BinOpKind::And | BinOpKind::BitAnd => self.builder.and(lhs, rhs),
            BinOpKind::Or | BinOpKind::BitOr => self.builder.or(lhs, rhs),
            BinOpKind::BitXor => self.builder.xor(lhs, rhs),
            BinOpKind::Shl => {
                let result = self.builder.shl(rhs, lhs);
                self.truncate_wrapping_result(result, arithmetic)
            }
            BinOpKind::Shr => match arithmetic {
                Some(ArithmeticKind::Signed(_)) => self.builder.sar(rhs, lhs),
                _ => self.builder.shr(rhs, lhs),
            },
            BinOpKind::Sar => self.builder.sar(rhs, lhs),
            BinOpKind::Pow => {
                if self.unchecked {
                    let result = self.builder.exp(lhs, rhs);
                    self.truncate_wrapping_result(result, arithmetic)
                } else if let Some(kind) = arithmetic {
                    self.checked_pow(lhs, rhs, kind)
                } else {
                    self.builder.exp(lhs, rhs)
                }
            }
        }
    }

    pub(super) fn unary(
        &mut self,
        op: UnOpKind,
        value: ValueId,
        span: Span,
        ty: Option<Ty<'gcx>>,
    ) -> Option<ValueId> {
        Some(match op {
            UnOpKind::Not => self.builder.iszero(value),
            UnOpKind::Neg => {
                if !self.unchecked
                    && let Some(ArithmeticKind::Signed(bits)) = ty.and_then(arithmetic_kind)
                {
                    let (min, _) = signed_bounds(bits, &mut self.builder);
                    let overflow = self.builder.eq(value, min);
                    self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                }
                let zero = self.builder.imm(U256::ZERO);
                let result = self.builder.sub(zero, value);
                if self.unchecked {
                    self.truncate_wrapping_result(result, ty.and_then(arithmetic_kind))
                } else {
                    result
                }
            }
            UnOpKind::BitNot => {
                let result = self.builder.not(value);
                let Some(ty) = ty else { return Some(result) };
                self.clean_bit_not_result(result, ty)
            }
            UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                return self.context.report_unsupported(span, "increment or decrement");
            }
        })
    }

    fn clean_bit_not_result(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Udvt(inner, _) => self.clean_bit_not_result(value, inner),
            TyKind::Elementary(ElementaryType::UInt(size)) => self.mask_to_bits(value, size.bits()),
            TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
                self.clean_fixed_bytes(value, size.bytes())
            }
            _ => value,
        }
    }
}

pub(super) fn fixed_bytes_width(ty: Ty<'_>) -> Option<u8> {
    match ty.peel_refs().kind {
        TyKind::Udvt(inner, _) => fixed_bytes_width(inner),
        TyKind::Elementary(ElementaryType::FixedBytes(size)) => Some(size.bytes()),
        _ => None,
    }
}
