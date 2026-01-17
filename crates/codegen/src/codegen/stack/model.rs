//! Stack model for tracking EVM stack state.
//!
//! The StackModel maps MIR ValueIds to stack positions and provides operations
//! for manipulating the stack (DUP, SWAP, POP).

use crate::mir::ValueId;
use smallvec::SmallVec;

/// Maximum stack depth accessible via DUP/SWAP (DUP16, SWAP16).
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
pub struct StackModel {
    /// The stack, with index 0 being the top.
    /// Each entry is either a known ValueId or None (for unknown/spilled values).
    stack: SmallVec<[Option<ValueId>; 16]>,
}

impl StackModel {
    /// Creates a new empty stack model.
    #[must_use]
    pub fn new() -> Self {
        Self { stack: SmallVec::new() }
    }

    /// Returns the current stack depth.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Returns true if the stack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Pushes a value onto the stack.
    pub fn push(&mut self, value: ValueId) {
        self.stack.insert(0, Some(value));
    }

    /// Pushes an unknown/anonymous value onto the stack.
    pub fn push_unknown(&mut self) {
        self.stack.insert(0, None);
    }

    /// Pops the top value from the stack.
    /// Returns the value that was at the top, if known.
    pub fn pop(&mut self) -> Option<ValueId> {
        debug_assert!(!self.stack.is_empty(), "Stack underflow");
        if self.stack.is_empty() { None } else { self.stack.remove(0) }
    }

    /// Returns the value at the given stack depth (0 = top).
    #[must_use]
    pub fn peek(&self, depth: usize) -> Option<ValueId> {
        self.stack.get(depth).copied().flatten()
    }

    /// Returns the value at the top of the stack.
    #[must_use]
    pub fn top(&self) -> Option<ValueId> {
        self.peek(0)
    }

    /// Finds the depth of a value on the stack.
    /// Returns None if the value is not on the stack.
    #[must_use]
    pub fn find(&self, value: ValueId) -> Option<usize> {
        self.stack.iter().position(|&v| v == Some(value))
    }

    /// Returns true if the value is on the stack.
    #[must_use]
    pub fn contains(&self, value: ValueId) -> bool {
        self.find(value).is_some()
    }

    /// Counts how many times a value appears on the stack.
    #[must_use]
    pub fn count(&self, value: ValueId) -> usize {
        self.stack.iter().filter(|&&v| v == Some(value)).count()
    }

    /// Returns true if the value is at the top of the stack.
    #[must_use]
    pub fn is_on_top(&self, value: ValueId) -> bool {
        self.peek(0) == Some(value)
    }

    /// Returns true if the value is accessible via DUP (depth < 16).
    #[must_use]
    pub fn is_accessible(&self, value: ValueId) -> bool {
        self.find(value).is_some_and(|d| d < MAX_STACK_ACCESS)
    }

    /// Simulates a DUP operation.
    /// `n` is 1-indexed (DUP1 = duplicate top, DUP2 = duplicate second from top).
    pub fn dup(&mut self, n: u8) {
        let depth = (n - 1) as usize;
        if let Some(&value) = self.stack.get(depth) {
            self.stack.insert(0, value);
        }
    }

    /// Simulates a SWAP operation.
    /// `n` is 1-indexed (SWAP1 = swap top with second, SWAP2 = swap top with third).
    pub fn swap(&mut self, n: u8) {
        let depth = n as usize;
        if depth < self.stack.len() {
            self.stack.swap(0, depth);
        }
    }

    /// Removes a value from the stack at any position.
    /// Used when we know a value is dead and we want to track its removal.
    pub fn remove(&mut self, value: ValueId) -> bool {
        if let Some(pos) = self.find(value) {
            self.stack.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clears the stack.
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Returns an iterator over all values on the stack (top to bottom).
    pub fn iter(&self) -> impl Iterator<Item = Option<ValueId>> + '_ {
        self.stack.iter().copied()
    }

    /// Returns the stack contents as a slice (top to bottom).
    #[must_use]
    pub fn as_slice(&self) -> &[Option<ValueId>] {
        &self.stack
    }
}

impl Default for StackModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Operations to emit for stack manipulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackOp {
    /// DUP1-DUP16: Duplicate the nth stack element.
    Dup(u8),
    /// SWAP1-SWAP16: Swap top with the nth stack element.
    Swap(u8),
    /// POP: Remove top of stack.
    Pop,
}

impl StackOp {
    /// Returns the opcode byte for this operation.
    #[must_use]
    pub const fn opcode(self) -> u8 {
        match self {
            Self::Dup(n) => 0x80 + n - 1,  // DUP1 = 0x80
            Self::Swap(n) => 0x90 + n - 1, // SWAP1 = 0x90
            Self::Pop => 0x50,
        }
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
        assert!(model.is_empty());
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
}
