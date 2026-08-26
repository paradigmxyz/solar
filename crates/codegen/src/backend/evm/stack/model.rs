//! Stack model for tracking EVM stack state.
//!
//! The StackModel maps MIR ValueIds to stack positions and provides operations
//! for manipulating the stack (DUP, SWAP, POP).

use crate::{backend::evm::op::StackOp, mir::ValueId};
use smallvec::SmallVec;

/// Legacy DUP/SWAP reach used by bounded planning and calling conventions.
#[allow(dead_code)]
pub(crate) const MAX_STACK_ACCESS: usize = 16;

/// Maximum total stack depth for EVM.
#[allow(dead_code)]
pub(crate) const MAX_STACK_DEPTH: usize = 1024;

/// Represents the current state of the EVM stack.
///
/// Stack positions are 0-indexed from the top:
/// - Position 0 = top of stack
/// - Position 1 = second from top
/// - etc.
#[derive(Clone, Debug)]
pub(crate) struct StackModel {
    /// The stack, with index 0 being the top.
    /// Each entry is either a known ValueId or None (for anonymous or successor-unused words).
    stack: SmallVec<[Option<ValueId>; 16]>,
    /// Greatest depth reached since this model was created. Clearing at a
    /// block boundary deliberately retains the high-water mark so codegen can
    /// validate untracked internal-call prefixes after emitting a function.
    max_depth: usize,
}

impl StackModel {
    /// Creates a new empty stack model.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self { stack: SmallVec::new(), max_depth: 0 }
    }

    /// Creates a stack model from values ordered top to bottom.
    pub(crate) fn from_top_to_bottom(values: impl IntoIterator<Item = Option<ValueId>>) -> Self {
        let stack = values.into_iter().collect::<SmallVec<_>>();
        let max_depth = stack.len();
        Self { stack, max_depth }
    }

    /// Returns the current stack depth.
    #[must_use]
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Returns the greatest depth reached by this model.
    #[must_use]
    pub(crate) fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Retains a high-water mark observed on another control-flow path.
    pub(crate) fn inherit_max_depth(&mut self, max_depth: usize) {
        self.max_depth = self.max_depth.max(max_depth);
    }

    /// Pushes a value onto the stack.
    pub(crate) fn push(&mut self, value: ValueId) {
        self.stack.insert(0, Some(value));
        self.max_depth = self.max_depth.max(self.stack.len());
    }

    /// Pushes an unknown/anonymous value onto the stack.
    pub(crate) fn push_unknown(&mut self) {
        self.stack.insert(0, None);
        self.max_depth = self.max_depth.max(self.stack.len());
    }

    /// Pops the top value from the stack.
    /// Returns the value that was at the top, if known.
    pub(crate) fn pop(&mut self) -> Option<ValueId> {
        debug_assert!(!self.stack.is_empty(), "Stack underflow");
        if self.stack.is_empty() { None } else { self.stack.remove(0) }
    }

    /// Returns the value at the given stack depth (0 = top).
    #[must_use]
    pub(crate) fn peek(&self, depth: usize) -> Option<ValueId> {
        self.stack.get(depth).copied().flatten()
    }

    /// Returns the value at the top of the stack.
    #[must_use]
    pub(crate) fn top(&self) -> Option<ValueId> {
        self.peek(0)
    }

    /// Finds the depth of a value on the stack.
    /// Returns None if the value is not on the stack.
    #[must_use]
    pub(crate) fn find(&self, value: ValueId) -> Option<usize> {
        self.stack.iter().position(|&v| v == Some(value))
    }

    /// Returns true if the value is on the stack.
    #[must_use]
    pub(crate) fn contains(&self, value: ValueId) -> bool {
        self.find(value).is_some()
    }

    /// Returns true if the value is at the top of the stack.
    #[must_use]
    pub(crate) fn is_on_top(&self, value: ValueId) -> bool {
        self.peek(0) == Some(value)
    }

    /// Simulates a DUP operation.
    /// `n` is 1-indexed (DUP1 = duplicate top, DUP2 = duplicate second from top).
    pub(crate) fn dup(&mut self, n: u8) {
        debug_assert!(StackOp::Dup(n).is_valid(), "DUP depth out of range: {n}");
        let depth = (n - 1) as usize;
        debug_assert!(
            depth < self.stack.len(),
            "DUP{} attempted but stack only has {} elements",
            n,
            self.stack.len()
        );
        if let Some(&value) = self.stack.get(depth) {
            self.stack.insert(0, value);
            self.max_depth = self.max_depth.max(self.stack.len());
        }
    }

    /// Simulates a SWAP operation.
    /// `n` is 1-indexed (SWAP1 = swap top with second, SWAP2 = swap top with third).
    pub(crate) fn swap(&mut self, n: u8) {
        debug_assert!(StackOp::Swap(n).is_valid(), "SWAP depth out of range: {n}");
        let depth = n as usize;
        debug_assert!(
            depth < self.stack.len(),
            "SWAP{} attempted but stack only has {} elements",
            n,
            self.stack.len()
        );
        if depth < self.stack.len() {
            self.stack.swap(0, depth);
        }
    }

    /// Simulates swapping two non-top stack elements.
    pub(crate) fn exchange(&mut self, n: u8, m: u8) {
        debug_assert!(StackOp::Exchange(n, m).is_valid(), "EXCHANGE depths out of range");
        debug_assert!(usize::from(m) < self.stack.len(), "EXCHANGE stack underflow");
        self.stack.swap(usize::from(n), usize::from(m));
    }

    /// Applies one physical stack operation.
    pub(crate) fn apply(&mut self, op: StackOp) {
        match op {
            StackOp::Dup(n) => self.dup(n),
            StackOp::Swap(n) => self.swap(n),
            StackOp::Exchange(n, m) => self.exchange(n, m),
            StackOp::Pop => {
                self.pop();
            }
        }
    }

    /// Renames one tracked stack word without changing the physical stack.
    pub(crate) fn rename(&mut self, value: ValueId, replacement: ValueId) -> bool {
        if let Some(pos) = self.find(value) {
            self.stack[pos] = Some(replacement);
            true
        } else {
            false
        }
    }

    /// Forgets the identity of stack words that do not satisfy `keep`.
    pub(crate) fn forget_values_not_matching(&mut self, mut keep: impl FnMut(ValueId) -> bool) {
        for slot in &mut self.stack {
            if slot.is_some_and(|value| !keep(value)) {
                *slot = None;
            }
        }
    }

    /// Clears the stack.
    pub(crate) fn clear(&mut self) {
        self.stack.clear();
    }

    /// Returns an iterator over all values on the stack (top to bottom).
    pub(crate) fn iter(&self) -> impl Iterator<Item = Option<ValueId>> + '_ {
        self.stack.iter().copied()
    }

    /// Returns the stack contents as a slice (top to bottom).
    #[must_use]
    pub(crate) fn as_slice(&self) -> &[Option<ValueId>] {
        &self.stack
    }
}

impl Default for StackModel {
    fn default() -> Self {
        Self::new()
    }

    /// Returns the operation's static EVM gas cost.
    #[must_use]
    pub(crate) const fn static_gas(self) -> u32 {
        if matches!(self, Self::Pop) { 2 } else { 3 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        model.push(v0);
        model.push(v1);

        assert_eq!(model.depth(), 2);
        assert_eq!(model.top(), Some(v1));
        assert_eq!(model.pop(), Some(v1));
        assert_eq!(model.pop(), Some(v0));
        assert_eq!(model.depth(), 0);
    }

    #[test]
    fn test_find() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        model.push(v0);
        model.push(v1);
        model.push(v2);

        assert_eq!(model.find(v2), Some(0)); // Top
        assert_eq!(model.find(v1), Some(1));
        assert_eq!(model.find(v0), Some(2));
        assert_eq!(model.find(ValueId::from_usize(99)), None);
    }

    #[test]
    fn test_dup() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        model.push(v0);
        model.push(v1);
        // Stack: [v1, v0]

        model.dup(1); // DUP1 - duplicate top
        // Stack: [v1, v1, v0]

        assert_eq!(model.depth(), 3);
        assert_eq!(model.peek(0), Some(v1));
        assert_eq!(model.peek(1), Some(v1));
        assert_eq!(model.peek(2), Some(v0));
    }

    #[test]
    fn test_swap() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        model.push(v0);
        model.push(v1);
        model.push(v2);
        // Stack: [v2, v1, v0]

        model.swap(1); // SWAP1 - swap top with second
        // Stack: [v1, v2, v0]

        assert_eq!(model.peek(0), Some(v1));
        assert_eq!(model.peek(1), Some(v2));
        assert_eq!(model.peek(2), Some(v0));
    }

    #[test]
    fn test_rename() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        model.push(v0);
        model.push(v1);

        assert!(model.rename(v0, v2));
        assert_eq!(model.as_slice(), &[Some(v1), Some(v2)]);
        assert!(!model.rename(v0, v1));
    }

    #[test]
    fn test_forget_values_not_matching() {
        let mut model = StackModel::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        model.push(v0);
        model.push(v1);
        model.forget_values_not_matching(|value| value == v1);

        assert_eq!(model.as_slice(), &[Some(v1), None]);
    }
}
