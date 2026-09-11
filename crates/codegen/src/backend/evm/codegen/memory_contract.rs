//! Enforces the source memory-ownership contract during MIR-to-EVM emission.
//!
//! Without that contract, compiler-private memory must not be read or written:
//! source assembly may address it regardless of the current free-memory pointer.
//! Raw memory operations default to private. Source-semantic operations opt in
//! only at the opcode emission point, after operand scheduling, so scheduler
//! spills cannot inherit permission. A closed atomic stack-recovery sequence may use newly
//! expanded scratch only when it clears every word and memory extent is unobservable. This
//! check does not depend on debug metadata.

use super::{
    Assembler, CallGraphInfo, DeferredAlloc, DenseBitSet, EvmCodegen, EvmMemoryLayout, FunctionId,
    Gcx, Module, StackOp, U256, op,
};
use std::ops::{Deref, DerefMut};

pub(super) struct MemoryCheckedEmitter<'gcx> {
    assembler: Assembler<'gcx>,
    source_only: bool,
    reported: bool,
}

impl<'gcx> MemoryCheckedEmitter<'gcx> {
    pub(super) fn new(gcx: Gcx<'gcx>) -> Self {
        Self { assembler: Assembler::new(gcx), source_only: false, reported: false }
    }

    pub(super) fn clear(&mut self) {
        self.assembler.clear();
        self.source_only = false;
        self.reported = false;
    }

    pub(super) fn require_source_memory(&mut self, required: bool) {
        self.source_only = required;
    }

    pub(super) fn source_memory_required(&self) -> bool {
        self.source_only
    }

    pub(super) fn require_private_memory(&mut self) {
        if self.source_only && !self.reported {
            self.reported = true;
            if self.assembler.gcx.dcx().has_errors().is_ok() {
                self.assembler
                    .gcx
                    .dcx()
                    .err("codegen requires compiler memory in a stack-only context")
                    .emit();
            }
        }
    }

    pub(super) fn emit_op(&mut self, opcode: u8) {
        if matches!(
            opcode,
            op::MLOAD
                | op::MSTORE
                | op::MSTORE8
                | op::MSIZE
                | op::MCOPY
                | op::CALLDATACOPY
                | op::RETURNDATACOPY
                | op::CODECOPY
                | op::EXTCODECOPY
                | op::KECCAK256
                | op::CALL
                | op::CALLCODE
                | op::DELEGATECALL
                | op::STATICCALL
                | op::CREATE
                | op::CREATE2
                | op::RETURN
                | op::REVERT
                | op::LOG0
                | op::LOG1
                | op::LOG2
                | op::LOG3
                | op::LOG4
        ) {
            self.require_private_memory();
        }
        self.assembler.emit_op(opcode);
    }

    pub(super) fn emit_deferred_alloc(&mut self) -> DeferredAlloc {
        self.require_private_memory();
        self.assembler.emit_deferred_alloc()
    }

    /// Duplicates a buried stack word while owning only newly expanded, zeroed scratch.
    /// No source operation can run between staging and clearing these words. Successful memory
    /// expansion cannot wrap its address; an out-of-gas failure aborts before source execution.
    /// NOTE: MSIZE is not pure, so EVM block CSE neither caches it nor tracks stores at these
    /// dynamic addresses. Later rewrites must preserve the final zeroed contents.
    pub(super) fn emit_atomic_deep_stack_dup(
        &mut self,
        count: usize,
        stack_access_limit: usize,
        memory_extent_observable: bool,
    ) {
        assert!(
            !memory_extent_observable || self.assembler.gcx.dcx().has_errors().is_err(),
            "temporary memory expansion requires an unobservable memory extent",
        );
        let assembler = &mut self.assembler;
        for _ in 0..count {
            // mstore(msize(), top)
            assembler.emit_op(op::MSIZE);
            assembler.emit_op(op::MSTORE);
        }
        // dup reachable target
        assembler.emit_stack_op(StackOp::Dup(stack_access_limit as u8));
        for index in 0..count {
            // addr = msize() - (index + 1) * 32
            // restored = mload(addr); mstore(addr, 0)
            // swap restored, target
            assembler.emit_push(U256::from((index + 1) * EvmMemoryLayout::WORD_SIZE as usize));
            assembler.emit_op(op::MSIZE);
            assembler.emit_op(op::SUB);
            assembler.emit_op(op::DUP1);
            assembler.emit_op(op::MLOAD);
            assembler.emit_op(op::SWAP1);
            assembler.emit_push(U256::ZERO);
            assembler.emit_op(op::SWAP1);
            assembler.emit_op(op::MSTORE);
            assembler.emit_stack_op(StackOp::Swap(1));
        }
    }

    /// Rebuilds a tracked stack suffix using scratch that is cleared before control leaves here.
    /// Source indices count from the original top; the target lists its new top first.
    pub(super) fn emit_atomic_stack_layout(
        &mut self,
        source_words: usize,
        target: &[usize],
        memory_extent_observable: bool,
    ) {
        assert!(!memory_extent_observable);
        assert!(target.iter().all(|&index| index < source_words));
        let assembler = &mut self.assembler;
        for _ in 0..source_words {
            // mstore(msize(), top)
            assembler.emit_op(op::MSIZE);
            assembler.emit_op(op::MSTORE);
        }
        for &index in target.iter().rev() {
            // push mload(msize() - (source_words - index) * 32)
            assembler.emit_push(U256::from(
                (source_words - index) * EvmMemoryLayout::WORD_SIZE as usize,
            ));
            assembler.emit_op(op::MSIZE);
            assembler.emit_op(op::SUB);
            assembler.emit_op(op::MLOAD);
        }
        for index in 0..source_words {
            // mstore(msize() - (index + 1) * 32, 0)
            assembler.emit_push(U256::ZERO);
            assembler.emit_push(U256::from((index + 1) * EvmMemoryLayout::WORD_SIZE as usize));
            assembler.emit_op(op::MSIZE);
            assembler.emit_op(op::SUB);
            assembler.emit_op(op::MSTORE);
        }
    }

    pub(super) fn emit_source_op(&mut self, opcode: u8) {
        self.assembler.emit_op(opcode);
    }
}

impl<'gcx> Deref for MemoryCheckedEmitter<'gcx> {
    type Target = Assembler<'gcx>;

    fn deref(&self) -> &Self::Target {
        &self.assembler
    }
}

impl DerefMut for MemoryCheckedEmitter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.assembler
    }
}

impl EvmCodegen<'_> {
    /// Requires stack-owned compiler state for source ownership and recursive ABI lifetimes.
    /// Source ownership constrains each complete call context, without combining unrelated
    /// external entries. Frame-free recursive Yul tuples also constrain their reachable helpers:
    /// emitting a helper's frame arguments or return buffer would need private memory in its
    /// suspended caller. This requirement flows to descendants, not to unrelated callers.
    pub(super) fn collect_stack_only_memory_functions(
        module: &Module,
        call_graph: &CallGraphInfo,
    ) -> DenseBitSet<FunctionId> {
        let mut required = call_graph.source_only_memory_contexts(module);
        for (id, func) in module.functions.iter_enumerated() {
            if !required.contains(id)
                && func.attributes.is_yul
                && func.returns.len() > 1
                && call_graph.is_recursive(id)
            {
                required.insert(id);
                required.union(&call_graph.reachable_callees_from([id]));
            }
        }
        required
    }
}
