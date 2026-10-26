//! What each MIR operation computes, as the operation schema declares it.
//!
//! A row of the schema in `op_schema.rs` states its operation's meaning with a
//! `#[semantics(...)]` attribute written in the vocabulary of [`Semantics`]:
//!
//! - `opcode(NAME)`: the EVM opcode `NAME` applied to the operation's operands in canonical order,
//!   which is the opcode's pop order. Pure opcodes compute their results with the word semantics of
//!   the opcode table; memory, storage, environment, and call opcodes act on the machine that runs
//!   them.
//! - `word(value)`, `low_bits(value, bits)`, and `sign_extend(value, from, to)`: casts.
//! - `not_equal(a, b)`, `select(condition, if_true, if_false)`, `phi(incoming)`, `call(callee,
//!   args)`, and `checked(op, arithmetic, lhs, rhs)`.
//! - `allocate(size, semantics)`: a fresh memory region, and `data_copy(data, dest, size)`:
//!   constant data copied to memory.
//!
//! [`InstKind::semantics`] returns one instruction's declaration. Tools interpret it instead of
//! classifying operations themselves: constant folding evaluates the word operations
//! (`utils::eval`), and the interpreter (`utils::interp`) adds memory and calls. An operation
//! without a declaration has no interpretation. That covers the semantic operations that later
//! passes expand, and the placeholders whose values the backend's layout decides, such as frame
//! addresses, immutables, and library addresses.
//!
//! Instruction selection picks opcodes on its own, in `isle/mir-to-evm/select.isle`, and a test
//! checks that it picks each declared opcode. The interpreter therefore gives an operation the
//! meaning the schema states, not whatever the backend happens to emit for it.
//!
//! [`InstKind::semantics`]: super::InstKind::semantics

use super::{
    AllocationSemantics, ArithmeticKind, BlockId, Callee, CheckedOp, DataRef, LogTopics,
    PtrLocation, ValueId,
};
use crate::backend::evm::op;
use smallvec::{SmallVec, smallvec};

/// What one MIR operation computes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Semantics<'a> {
    /// The EVM opcode applied to these operands, in pop order.
    Opcode(u8, SmallVec<[ValueId; 8]>),
    /// The operand's word, unchanged: a widening or pointer cast of a value whose type already
    /// keeps its bits clean.
    Word(ValueId),
    /// The operand's low `bits` bits.
    LowBits(ValueId, u32),
    /// The operand's low `from` bits, sign-extended to `to` bits.
    SignExtend(ValueId, u32, u32),
    /// One when the operands differ, zero otherwise.
    NotEqual(ValueId, ValueId),
    /// The second operand when the first is nonzero, otherwise the third.
    Select(ValueId, ValueId, ValueId),
    /// The incoming value of the edge the block was entered along.
    Phi(&'a [(BlockId, ValueId)]),
    /// A call of a MIR function or a builtin on these arguments.
    Call(&'a Callee, &'a [ValueId]),
    /// The checked arithmetic operation on the operands, which panics when its result does not
    /// fit the arithmetic type or its divisor is zero.
    Checked(CheckedOp, ArithmeticKind, ValueId, ValueId),
    /// The address of a fresh memory region of the operand's size, with these alignment,
    /// initialization, and failure rules. Lowering bumps the free memory pointer for it.
    Allocate(ValueId, AllocationSemantics),
    /// The constant data at the reference, copied to memory at the first operand for the second
    /// operand's byte count.
    DataCopy(DataRef, ValueId, ValueId),
}

/// Constructors for the `#[semantics(...)]` declarations of the operation schema, which bind an
/// operation's fields by reference.
pub(super) mod declare {
    use super::*;

    /// The operand's word, unchanged.
    pub(crate) fn word(value: &ValueId) -> Semantics<'static> {
        Semantics::Word(*value)
    }

    /// The operand's low `bits` bits.
    pub(crate) fn low_bits(value: &ValueId, bits: &u32) -> Semantics<'static> {
        Semantics::LowBits(*value, *bits)
    }

    /// The operand's low `from` bits, sign-extended to `to` bits.
    pub(crate) fn sign_extend(value: &ValueId, from: &u32, to: &u32) -> Semantics<'static> {
        Semantics::SignExtend(*value, *from, *to)
    }

    /// Whether the operands differ.
    pub(crate) fn not_equal(a: &ValueId, b: &ValueId) -> Semantics<'static> {
        Semantics::NotEqual(*a, *b)
    }

    /// A choice between two operands.
    pub(crate) fn select(
        condition: &ValueId,
        if_true: &ValueId,
        if_false: &ValueId,
    ) -> Semantics<'static> {
        Semantics::Select(*condition, *if_true, *if_false)
    }

    /// The incoming value of the edge taken.
    pub(crate) fn phi(incoming: &[(BlockId, ValueId)]) -> Semantics<'_> {
        Semantics::Phi(incoming)
    }

    /// A call.
    pub(crate) fn call<'a>(callee: &'a Callee, args: &'a [ValueId]) -> Semantics<'a> {
        Semantics::Call(callee, args)
    }

    /// Checked arithmetic.
    pub(crate) fn checked(
        op: &CheckedOp,
        arithmetic: &ArithmeticKind,
        lhs: &ValueId,
        rhs: &ValueId,
    ) -> Semantics<'static> {
        Semantics::Checked(*op, *arithmetic, *lhs, *rhs)
    }

    /// A fresh memory region.
    pub(crate) fn allocate(size: &ValueId, semantics: &AllocationSemantics) -> Semantics<'static> {
        Semantics::Allocate(*size, *semantics)
    }

    /// The opcode that loads a word from `location`, which has none for return data.
    pub(crate) fn load(ptr: &ValueId, location: &PtrLocation) -> Option<Semantics<'static>> {
        let opcode = match location {
            PtrLocation::Memory => op::MLOAD,
            PtrLocation::Calldata => op::CALLDATALOAD,
            PtrLocation::Returndata => return None,
            PtrLocation::Storage => op::SLOAD,
            PtrLocation::Transient => op::TLOAD,
        };
        Some(Semantics::Opcode(opcode, smallvec![*ptr]))
    }

    /// The opcode that stores a word to `location`, which only memory and the two storages allow.
    pub(crate) fn store(
        ptr: &ValueId,
        value: &ValueId,
        location: &PtrLocation,
    ) -> Option<Semantics<'static>> {
        let opcode = match location {
            PtrLocation::Memory => op::MSTORE,
            PtrLocation::Storage => op::SSTORE,
            PtrLocation::Transient => op::TSTORE,
            PtrLocation::Calldata | PtrLocation::Returndata => return None,
        };
        Some(Semantics::Opcode(opcode, smallvec![*ptr, *value]))
    }

    /// The opcode that copies `len` bytes from `location` to memory, which storage lacks.
    pub(crate) fn copy(
        dest: &ValueId,
        src: &ValueId,
        len: &ValueId,
        location: &PtrLocation,
    ) -> Option<Semantics<'static>> {
        let opcode = match location {
            PtrLocation::Memory => op::MCOPY,
            PtrLocation::Calldata => op::CALLDATACOPY,
            PtrLocation::Returndata => op::RETURNDATACOPY,
            PtrLocation::Storage | PtrLocation::Transient => return None,
        };
        Some(Semantics::Opcode(opcode, smallvec![*dest, *src, *len]))
    }

    /// The `LOGn` opcode for `n` topics.
    pub(crate) fn log(offset: &ValueId, size: &ValueId, topics: &LogTopics) -> Semantics<'static> {
        let mut operands = smallvec![*offset, *size];
        operands.extend_from_slice(topics.as_slice());
        Semantics::Opcode(op::LOG0 + topics.len() as u8, operands)
    }

    /// Constant data copied to memory.
    pub(crate) fn data_copy(data: &DataRef, dest: &ValueId, size: &ValueId) -> Semantics<'static> {
        Semantics::DataCopy(*data, *dest, *size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::evm::op, mir::InstKind};
    use std::fmt::Write as _;

    #[test]
    fn declared_semantics() {
        let mut table = String::new();
        for (mnemonic, semantics) in InstKind::DECLARED_SEMANTICS {
            let semantics = if semantics.is_empty() { "-" } else { semantics };
            writeln!(table, "{mnemonic}: {semantics}").unwrap();
        }
        snapbox::assert_data_eq!(table, snapbox::file!["semantics.snap"]);
    }

    #[test]
    fn opcodes_match_stack_effects() {
        for kind in InstKind::forms() {
            if let Some(Semantics::Opcode(opcode, operands)) = kind.semantics() {
                let pushes = u8::from(kind.op_def().result.produces_value());
                let stack_io =
                    op::stack_io(opcode).map(|(pops, pushes)| (usize::from(pops), pushes));
                assert_eq!(stack_io, Some((operands.len(), pushes)), "{kind:?}");
            }
        }
    }

    #[test]
    fn selection_picks_the_declared_opcodes() {
        for kind in InstKind::forms() {
            if let Some(Semantics::Opcode(opcode, _)) = kind.semantics()
                && let Some(selected) = kind.evm_opcode()
            {
                assert_eq!(selected, opcode, "{kind:?}");
            }
        }
    }
}
