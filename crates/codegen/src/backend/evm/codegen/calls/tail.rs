//! Forward inherited return addresses through terminal void calls.
//!
//! At the MIR-to-EVM boundary, an internal call immediately followed by a void
//! return can install the callee's argument stack above the caller's inherited
//! return address. The callee then returns directly to the original continuation,
//! avoiding a new return label and the caller's return epilogue. MIR call and
//! return semantics remain unchanged.
//!
//! This currently requires nonrecursive static frames and a complete stack
//! argument convention. Dynamic frames retain their restoration protocol, and
//! mixed memory/stack arguments retain ordinary call lowering. Constructors and
//! external entries do not have the inherited internal return address. Discard
//! every tracked caller word except the callee's actuals before transferring;
//! even a zero-argument call must expose the hidden return address directly.

use super::super::{
    BlockId, DebugFunctionExit, EvmCodegen, Function, FunctionId, ICallStackEdge, InstKind,
    OptimizationMode, TargetSlot, Terminator, ValueId, op,
};

impl EvmCodegen<'_> {
    pub(in crate::backend::evm::codegen) fn void_tail_call<'a>(
        &self,
        caller: FunctionId,
        func: &'a Function,
        block: BlockId,
    ) -> Option<(FunctionId, &'a [ValueId])> {
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None)
            || !self.in_internal_function
            || !func.returns.is_empty()
            || !self.static_frame_functions.contains(caller)
            || self.recursive_frame_functions.contains(caller)
            || self.recursive_stack_functions.contains(caller)
        {
            return None;
        }
        let block = &func.blocks[block];
        if !matches!(&block.terminator, Some(Terminator::Stop))
            && !matches!(&block.terminator, Some(Terminator::Return { values }) if values.is_empty())
        {
            return None;
        }
        let inst = func.inst(*block.instructions.last()?);
        if let InstKind::ICall { function, args, returns: 0 } = &inst.kind
            && self.static_frame_functions.contains(*function)
            && !self.recursive_frame_functions.contains(*function)
            && !self.recursive_stack_functions.contains(*function)
            && (args.is_empty()
                || self.runtime_stack_args
                    && self
                        .stack_arg_mask(*function)
                        .is_some_and(|mask| mask.count() == args.len()))
        {
            Some((*function, args))
        } else {
            None
        }
    }

    pub(in crate::backend::evm::codegen) fn emit_void_tail_call(
        &mut self,
        caller: FunctionId,
        func: &Function,
        callee: FunctionId,
        args: &[ValueId],
    ) {
        // [inherited_return, caller_words] -> [inherited_return, args...]
        self.pop_stack_values_not_needed_by(args);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, args) {
            self.emit_operand(func, value);
        }
        let target = args.iter().rev().copied().map(TargetSlot::Value).collect::<Vec<_>>();
        let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else {
            // Regenerate with the callee's frame convention if the stack tuple
            // cannot be formed. The partial attempt is discarded.
            self.disabled_stack_only_functions.insert(callee);
            return;
        };
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }
        // jump callee
        self.emit_push_label(self.function_labels[&callee]);
        self.asm.emit_op(op::JUMP);
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
        // Keep the call edge in whole-program stack accounting. Charging a
        // fresh return word is conservative: this transfer reuses the old one.
        self.icall_stack_edges.push(ICallStackEdge {
            caller,
            callee,
            preserved_words: 0,
            argument_words: args.len(),
        });
    }
}
