//! Shared Solidity checked-arithmetic and bounds checks.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn panic_if(&mut self, condition: ValueId, code: u64) {
        let panic_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, panic_block, continue_block);
        self.builder.switch_to_block(panic_block);
        let selector = self.builder.imm_u256(U256::from(0x4e48_7b71_u64) << 224);
        let code = self.builder.imm_u256(U256::from(code));
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.mstore(zero, selector);
        let four = self.builder.imm_u256(U256::from(4));
        self.builder.mstore(four, code);
        let size = self.builder.imm_u256(U256::from(36));
        self.builder.revert(zero, size);
        self.builder.switch_to_block(continue_block);
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
