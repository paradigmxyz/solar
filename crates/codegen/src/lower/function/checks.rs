//! Shared Solidity checked-arithmetic and bounds checks.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn panic_if(&mut self, condition: ValueId, code: PanicCode) {
        self.builder.panic_if(condition, code);
    }

    pub(super) fn panic_if_zero(&mut self, condition: ValueId, code: PanicCode) {
        self.builder.panic_if_zero(condition, code);
    }

    pub(super) fn validate_enum_value(&mut self, ty: solar_sema::ty::Ty<'gcx>, value: ValueId) {
        let TyKind::Enum(id) = ty.peel_refs().kind else { return };
        let variants = self.gcx.hir.enumm(id).variants.len() as u64;
        let limit = self.builder.imm_u64(variants);
        let valid = self.builder.lt(value, limit);
        let invalid = self.builder.iszero(valid);
        self.panic_if(invalid, PanicCode::EnumConversion);
    }

    pub(super) fn checked_add(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.add(lhs, rhs);
        let overflow = self.builder.lt(result, lhs);
        self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        result
    }

    pub(super) fn checked_mul(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.mul(lhs, rhs);
        let rhs_zero = self.builder.iszero(rhs);
        let quotient = self.builder.div(result, rhs);
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let overflow = self.builder.iszero(valid);
        self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        result
    }

    pub(super) fn bounds_check(&mut self, index: ValueId, length: ValueId) {
        let in_range = self.builder.lt(index, length);
        let invalid = self.builder.iszero(in_range);
        self.panic_if(invalid, PanicCode::ArrayOutOfBounds);
    }
}
