//! Checked arithmetic and scalar operator lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
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

    pub(super) fn binary(
        &mut self,
        op: BinOpKind,
        lhs: ValueId,
        rhs: ValueId,
        ty: Option<Ty<'gcx>>,
    ) -> ValueId {
        let arithmetic = ty.and_then(arithmetic_kind);
        let checked = match op {
            BinOpKind::Add if !self.unchecked && arithmetic.is_some() => Some(CheckedOp::Add),
            BinOpKind::Sub if !self.unchecked && arithmetic.is_some() => Some(CheckedOp::Sub),
            BinOpKind::Mul if !self.unchecked && arithmetic.is_some() => Some(CheckedOp::Mul),
            BinOpKind::Pow if !self.unchecked && arithmetic.is_some() => Some(CheckedOp::Pow),
            BinOpKind::Div => {
                Some(if self.unchecked { CheckedOp::WrappingDiv } else { CheckedOp::Div })
            }
            BinOpKind::Rem => Some(CheckedOp::Rem),
            _ => None,
        };
        if let Some(op) = checked {
            // lhs/rhs = word_cast(object) when assembly supplied raw pointer bits
            let lhs = if matches!(self.builder.func().value_ty(lhs), Some(MirType::MemoryObject(_)))
            {
                self.builder.word_cast(lhs)
            } else {
                lhs
            };
            let rhs = if matches!(self.builder.func().value_ty(rhs), Some(MirType::MemoryObject(_)))
            {
                self.builder.word_cast(rhs)
            } else {
                rhs
            };
            // result = checked_op<arithmetic>(lhs, rhs)
            return self.builder.emit_inst(
                InstKind::CheckedBinary {
                    op,
                    arithmetic: arithmetic.unwrap_or(ArithmeticKind::Unsigned(256)),
                    lhs,
                    rhs,
                },
                Some(MirType::uint256()),
            );
        }
        match op {
            BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul => {
                let result = match op {
                    BinOpKind::Add => self.builder.add(lhs, rhs),
                    BinOpKind::Sub => self.builder.sub(lhs, rhs),
                    _ => self.builder.mul(lhs, rhs),
                };
                if self.unchecked {
                    self.truncate_wrapping_result(result, arithmetic)
                } else {
                    result
                }
            }
            BinOpKind::Div | BinOpKind::Rem => unreachable!("division is a semantic operation"),
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
                } else {
                    self.builder.exp(lhs, rhs)
                }
            }
        }
    }

    pub(super) fn unary(&mut self, op: UnOpKind, value: ValueId, ty: Option<Ty<'gcx>>) -> ValueId {
        match op {
            UnOpKind::Not => self.builder.iszero(value),
            UnOpKind::Neg => {
                if !self.unchecked
                    && let Some(ArithmeticKind::Signed(bits)) = ty.and_then(arithmetic_kind)
                {
                    // result = checked_sub<signed>(0, value)
                    let zero = self.builder.imm(U256::ZERO);
                    return self.builder.emit_inst(
                        InstKind::CheckedBinary {
                            op: CheckedOp::Sub,
                            arithmetic: ArithmeticKind::Signed(bits),
                            lhs: zero,
                            rhs: value,
                        },
                        Some(MirType::uint256()),
                    );
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
                let Some(ty) = ty else { return result };
                self.clean_bit_not_result(result, ty)
            }
            UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                unreachable!("increment and decrement lower before `unary`")
            }
        }
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
