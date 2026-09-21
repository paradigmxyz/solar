//! Checked arithmetic and scalar operator lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn truncate_wrapping_result(
        &mut self,
        value: ValueId,
        kind: Option<ArithmeticKind>,
    ) -> ValueId {
        match kind {
            Some(kind) => self.builder.cast(value, kind.ty().mir_type()),
            None => value,
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
        let (lhs, rhs) = if let Some(kind) = arithmetic {
            let lhs = self.builder.cast(lhs, kind.ty().mir_type());
            let rhs = if matches!(
                op,
                BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar | BinOpKind::Pow
            ) {
                rhs
            } else {
                self.builder.cast(rhs, kind.ty().mir_type())
            };
            (lhs, rhs)
        } else {
            (lhs, rhs)
        };
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
            // lhs/rhs = ptrtoint(object) when assembly supplied raw pointer bits
            let lhs = if matches!(self.builder.func().value_ty(lhs), Some(MirType::MemoryObject(_)))
            {
                self.builder.cast_word(lhs)
            } else {
                lhs
            };
            let rhs = if matches!(self.builder.func().value_ty(rhs), Some(MirType::MemoryObject(_)))
            {
                self.builder.cast_word(rhs)
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
                Some(arithmetic.unwrap_or(ArithmeticKind::Unsigned(256)).ty().mir_type()),
            );
        }
        if let Some(kind) = arithmetic
            && matches!(
                op,
                BinOpKind::Add
                    | BinOpKind::Sub
                    | BinOpKind::Mul
                    | BinOpKind::BitAnd
                    | BinOpKind::BitOr
                    | BinOpKind::BitXor
                    | BinOpKind::Shl
                    | BinOpKind::Shr
                    | BinOpKind::Sar
            )
        {
            let scalar_ty = kind.ty().mir_type();
            let lhs = self.builder.cast(lhs, scalar_ty);
            let rhs = if matches!(op, BinOpKind::Shl | BinOpKind::Shr | BinOpKind::Sar)
                && scalar_ty != MirType::I256
                && self.builder.func().value_ty(rhs).and_then(MirType::integer_bits)
                    > scalar_ty.integer_bits()
                && !self.builder.func().value_u256(rhs).is_some_and(|value| {
                    value.bit_len() <= scalar_ty.integer_bits().unwrap() as usize
                }) {
                let bits = scalar_ty.integer_bits().unwrap();
                let limit = self.builder.imm(bits);
                let too_large = self.builder.gt(rhs, limit);
                let count = self.builder.select(too_large, limit, rhs);
                self.builder.cast(count, scalar_ty)
            } else {
                self.builder.cast(rhs, scalar_ty)
            };
            let operation = match op {
                BinOpKind::Add => Some(InstKind::Add(lhs, rhs)),
                BinOpKind::Sub => Some(InstKind::Sub(lhs, rhs)),
                BinOpKind::Mul => Some(InstKind::Mul(lhs, rhs)),
                BinOpKind::BitAnd => Some(InstKind::And(lhs, rhs)),
                BinOpKind::BitOr => Some(InstKind::Or(lhs, rhs)),
                BinOpKind::BitXor => Some(InstKind::Xor(lhs, rhs)),
                BinOpKind::Shl => Some(InstKind::Shl(rhs, lhs)),
                BinOpKind::Shr if matches!(kind, ArithmeticKind::Signed(_)) => {
                    Some(InstKind::Sar(rhs, lhs))
                }
                BinOpKind::Shr => Some(InstKind::Shr(rhs, lhs)),
                BinOpKind::Sar => Some(InstKind::Sar(rhs, lhs)),
                _ => None,
            };
            if let Some(operation) = operation {
                return self.builder.emit_inst(operation, Some(scalar_ty));
            }
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
            BinOpKind::Ne => self.builder.ne(lhs, rhs),
            BinOpKind::Le => {
                let gt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.sgt(lhs, rhs),
                    _ => self.builder.gt(lhs, rhs),
                };
                self.builder.eq_zero(gt)
            }
            BinOpKind::Ge => {
                let lt = match arithmetic {
                    Some(ArithmeticKind::Signed(_)) => self.builder.slt(lhs, rhs),
                    _ => self.builder.lt(lhs, rhs),
                };
                self.builder.eq_zero(lt)
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
        if let Some(kind) = ty.and_then(arithmetic_kind) {
            let scalar_ty = kind.ty().mir_type();
            let value = self.builder.cast(value, scalar_ty);
            if op == UnOpKind::BitNot {
                return self.builder.emit_inst(InstKind::Not(value), Some(scalar_ty));
            }
            if op == UnOpKind::Neg && self.unchecked {
                let zero = self.builder.imm(0);
                let zero = self.builder.cast(zero, scalar_ty);
                return self.builder.emit_inst(InstKind::Sub(zero, value), Some(scalar_ty));
            }
        }
        match op {
            UnOpKind::Not => self.builder.eq_zero(value),
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
                        Some(ArithmeticKind::Signed(bits).ty().mir_type()),
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
