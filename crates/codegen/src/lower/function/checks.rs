//! Shared Solidity checked-arithmetic and bounds checks.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn panic_if(&mut self, condition: ValueId, code: u64) {
        self.builder.panic_if(condition, code);
    }

    pub(super) fn checked_add(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.add(lhs, rhs);
        let overflow = self.builder.lt(result, lhs);
        self.panic_if(overflow, 0x41);
        result
    }

    pub(super) fn checked_mul(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        let result = self.builder.mul(lhs, rhs);
        let rhs_zero = self.builder.iszero(rhs);
        let quotient = self.builder.div(result, rhs);
        let exact = self.builder.eq(quotient, lhs);
        let valid = self.builder.or(rhs_zero, exact);
        let overflow = self.builder.iszero(valid);
        self.panic_if(overflow, 0x41);
        result
    }

    pub(super) fn bounds_check(&mut self, index: ValueId, length: ValueId) {
        let in_range = self.builder.lt(index, length);
        let invalid = self.builder.iszero(in_range);
        self.panic_if(invalid, 0x32);
    }
}
