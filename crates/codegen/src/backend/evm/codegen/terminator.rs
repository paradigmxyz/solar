//! Shared terminator emission: external returns, return-data reverts, and labeled jumps.
//!
//! Conditional jumps consume a word and a requested zero/nonzero sense. They do not require
//! a normalized boolean. EVM IR cleanup selects the final condition after layout: double zero
//! tests can disappear, inequality can use SUB, and constant comparisons can absorb an
//! inversion by adjusting their bound. Keeping those selections after layout also covers
//! polarity changes from revert sharing, without changing boolean values still used as data.

use super::{DebugFunctionExit, EvmCodegen, Function, Label, U256, op};

impl<'gcx> EvmCodegen<'gcx> {
    pub(super) fn emit_external_return(&mut self, func: &Function) {
        if let Some(exit) = self.constructor_exit {
            // return [] => push constructor_exit; jump
            self.asm.emit_push_label(exit);
            self.asm.emit_op(op::JUMP);
        } else {
            // return [] => stop
            self.asm.emit_op(op::STOP);
        }
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
    }

    pub(super) fn emit_revert_returndata(&mut self) {
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            // returndatacopy 0, 0, returndatasize
            // revert 0, returndatasize
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_op(op::RETURNDATASIZE);
            self.asm.emit_op(op::RETURNDATACOPY);
            self.asm.emit_op(op::RETURNDATASIZE);
            self.asm.emit_push(U256::ZERO);
        } else {
            // revert 0, 0
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
        }
        self.asm.emit_op(op::REVERT);
    }

    /// Consumes the top word as a zero/nonzero condition, preserving the stack below it.
    pub(super) fn emit_conditional_jump(&mut self, target: Label, jump_if_zero: bool) {
        // [condition]; [iszero]; push target; jumpi
        if jump_if_zero {
            self.asm.emit_op(op::ISZERO);
        }
        self.asm.emit_push_label(target);
        self.asm.emit_op(op::JUMPI);
    }
}
