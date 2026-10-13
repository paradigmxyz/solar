//! A symbolic stack of value identities, for checking and synthesizing runs of stack operations.

use crate::{backend::evm::op::StackOp, mir::ValueId};
use smallvec::SmallVec;

/// Maximum total stack depth for EVM.
pub(crate) const MAX_STACK_DEPTH: usize = 1024;

/// A symbolic EVM stack, top first.
#[derive(Clone, Debug)]
pub(crate) struct StackModel(SmallVec<[ValueId; 16]>);

impl StackModel {
    /// Creates a stack from values ordered top to bottom.
    pub(crate) fn from_top_to_bottom(values: impl IntoIterator<Item = ValueId>) -> Self {
        Self(values.into_iter().collect())
    }

    /// Pushes a value.
    pub(crate) fn push(&mut self, value: ValueId) {
        self.0.insert(0, value);
    }

    /// Returns the value at the given depth (0 = top).
    #[must_use]
    pub(crate) fn peek(&self, depth: usize) -> Option<ValueId> {
        self.0.get(depth).copied()
    }

    /// Applies one stack operation, which must be in range.
    pub(crate) fn apply(&mut self, op: StackOp) {
        debug_assert!(op.required_depth() <= self.0.len(), "{op:?} underflows the stack");
        match op {
            StackOp::Dup(n) => self.0.insert(0, self.0[usize::from(n) - 1]),
            StackOp::Swap(n) => self.0.swap(0, usize::from(n)),
            StackOp::Exchange(n, m) => self.0.swap(usize::from(n), usize::from(m)),
            StackOp::Pop => {
                self.0.remove(0);
            }
        }
    }

    /// Returns the values top to bottom.
    #[must_use]
    pub(crate) fn as_slice(&self) -> &[ValueId] {
        &self.0
    }
}
