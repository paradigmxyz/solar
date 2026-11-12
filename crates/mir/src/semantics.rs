//! What each MIR operation computes, as the operation schema declares it.
//!
//! A row of the schema in `op_schema.rs` states its operation's meaning with a
//! `#[semantics(...)]` attribute written in the vocabulary of [`Semantics`]:
//!
//! - `opcode(name)`: the EVM opcode with the mnemonic `name` in the EVM IR opcode table, applied to
//!   the operation's operands in canonical order, which is the opcode's pop order. Pure opcodes
//!   compute their results with the table's word semantics; memory, storage, environment, and call
//!   opcodes act on the machine that runs them. `opcode(match location { Memory => name, .. })`
//!   picks the opcode by the operation's pointer location; a location without an arm has no
//!   interpretation.
//! - `log(offset, size, topics)`: `log0` through `log4`, by topic count.
//! - `word(value)`, `low_bits(value, bits)`, and `sign_extend(value, from, to)`: casts.
//! - `not_equal(a, b)`, `select(condition, if_true, if_false)`, `phi(incoming)`, `call(callee,
//!   args)`, and `checked(op, arithmetic, lhs, rhs)`.
//! - `allocate(size, semantics)`: a fresh memory region, and `data_copy(data, dest, size)`:
//!   constant data copied to memory.
//! - `heap_floor()`: the initial heap floor resolved by backend placement.
//! - `insert_value(ty, aggregate, index, value)`, `extract_value(ty, aggregate, index)`, and
//!   `make_slice(location, ptr, len)`: functional typed products, without allocation.
//! - Slot-hashing and storage-byte operations retain their scratch writes, header checks, padding
//!   masks and tail clearing; they do not delegate interpretation to lowering.
//!
//! [`InstKind::semantics`] returns one instruction's declaration, and the MIR interpreter
//! (`solar-mir-interpreter`) runs it instead of classifying operations itself. An operation
//! without a declaration has no interpretation. That covers the semantic operations that later
//! passes expand, and the placeholders whose values the backend's layout decides, such as frame
//! addresses, immutables, and library addresses.
//!
//! MIR does not depend on the opcode table, so it names opcodes by mnemonic. The interpreter's
//! tests check that every declared mnemonic names an opcode with the operation's stack effect, and
//! the compiler's tests check that instruction selection never picks another opcode, so the
//! interpreter gives an operation the meaning the schema states, not whatever the backend happens
//! to emit for it.

use crate::{
    AllocationSemantics, ArithmeticKind, BlockId, Callee, CheckedOp, DataRef, InstKind, LogTopics,
    MirType, PtrLocation, ValueId,
};
use alloy_primitives::Bytes;
use smallvec::SmallVec;

/// What one MIR operation computes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Semantics<'a> {
    /// The EVM opcode with this mnemonic applied to these operands, in pop order.
    Opcode(&'static str, SmallVec<[ValueId; 8]>),
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
    /// The address of a fresh memory region of the operand's size, with these initialization and
    /// failure rules. Lowering bumps the free memory pointer for it, except for the allocations it
    /// defers to the backend, which places them in its layout.
    Allocate(ValueId, AllocationSemantics),
    /// The constant data at the reference, copied to memory at the first operand for the second
    /// operand's byte count.
    DataCopy(DataRef, ValueId, ValueId),
    /// The initial heap floor of the transaction.
    HeapFloor,
    /// Wrapping memory-pointer offset, without bounds or provenance guarantees.
    /// This stays distinct from an integer opcode and does not imply integer folding.
    PointerAdd(ValueId, ValueId),
    /// Read the explicit free-memory pointer, without initializing it.
    FreePointer,
    /// Zero exactly the given byte range, preserving bytes outside it.
    MemoryZero(ValueId, ValueId),
    /// Functional update of one immediate product field; other fields are retained.
    InsertValue(MirType, ValueId, u32, ValueId),
    /// Read one immediate product field, without reading the other fields.
    ExtractValue(MirType, ValueId, u32),
    /// A logical pointer/length pair, without allocating or accessing its range.
    MakeSlice(PtrLocation, ValueId, ValueId),
    /// Hash the key and parent slot after writing them to scratch words 0 and 32.
    MappingSlot(ValueId, ValueId),
    /// Hash a bytes key followed by its parent slot in unreserved free memory.
    MappingSlotBytes(ValueId, ValueId),
    /// Hash the array parent slot after writing scratch word 0.
    StorageArrayDataSlot(ValueId),
    /// The array data hash plus the wrapping index times its slot stride.
    StorageArrayElementSlot(ValueId, ValueId, u64),
    /// Validate Solidity's short/long byte-string storage header, or panic 0x22.
    ValidateStorageBytes(ValueId),
    /// Allocate and read a Solidity storage byte string, preserving word reads.
    LoadStorageBytes(ValueId),
    /// Store a memory byte string, clearing an old long tail and masking padding.
    StoreStorageBytes(ValueId, ValueId),
    /// Store literal bytes without materializing a memory object.
    StoreStorageBytesLiteral(ValueId, &'a [u8]),
    /// Zero array-data words in the half-open index range, retaining scratch hashing.
    ClearStorageWords(ValueId, ValueId, ValueId),
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

    /// Constant data copied to memory.
    pub(crate) fn data_copy(data: &DataRef, dest: &ValueId, size: &ValueId) -> Semantics<'static> {
        Semantics::DataCopy(*data, *dest, *size)
    }

    /// The initial heap floor of the transaction.
    pub(crate) fn heap_floor() -> Semantics<'static> {
        Semantics::HeapFloor
    }

    /// `log0` through `log4`, by topic count.
    pub(crate) fn log(offset: &ValueId, size: &ValueId, topics: &LogTopics) -> Semantics<'static> {
        const LOGS: [&str; LogTopics::MAX + 1] = ["log0", "log1", "log2", "log3", "log4"];
        Semantics::Opcode(
            LOGS[topics.len()],
            [*offset, *size].into_iter().chain(topics.as_slice().iter().copied()).collect(),
        )
    }

    /// Wrapping pointer arithmetic, without a physical heap-layout assumption.
    pub(crate) fn pointer_add(ptr: &ValueId, offset: &ValueId) -> Semantics<'static> {
        Semantics::PointerAdd(*ptr, *offset)
    }

    /// The current allocation cursor, read without implicit entry initialization.
    pub(crate) fn free_pointer() -> Semantics<'static> {
        Semantics::FreePointer
    }

    /// Byte-exact memory initialization, without a backend placement decision.
    pub(crate) fn memory_zero(offset: &ValueId, size: &ValueId) -> Semantics<'static> {
        Semantics::MemoryZero(*offset, *size)
    }
    /// Functional product insertion.
    pub(crate) fn insert_value(
        ty: &MirType,
        aggregate: &ValueId,
        index: &u32,
        value: &ValueId,
    ) -> Semantics<'static> {
        Semantics::InsertValue(*ty, *aggregate, *index, *value)
    }

    /// Product projection.
    pub(crate) fn extract_value(
        ty: &MirType,
        aggregate: &ValueId,
        index: &u32,
    ) -> Semantics<'static> {
        Semantics::ExtractValue(*ty, *aggregate, *index)
    }

    /// Logical slice construction.
    pub(crate) fn make_slice(
        location: &PtrLocation,
        ptr: &ValueId,
        len: &ValueId,
    ) -> Semantics<'static> {
        Semantics::MakeSlice(*location, *ptr, *len)
    }

    /// Fixed-width mapping hashing, including scratch stores.
    pub(crate) fn mapping_slot(key: &ValueId, slot: &ValueId) -> Semantics<'static> {
        Semantics::MappingSlot(*key, *slot)
    }

    /// Variable-width mapping hashing, including its transient memory footprint.
    pub(crate) fn mapping_slot_bytes(key: &ValueId, slot: &ValueId) -> Semantics<'static> {
        Semantics::MappingSlotBytes(*key, *slot)
    }

    /// Dynamic array data hashing.
    pub(crate) fn storage_array_data_slot(slot: &ValueId) -> Semantics<'static> {
        Semantics::StorageArrayDataSlot(*slot)
    }

    /// Dynamic array element addressing with wrapping arithmetic.
    pub(crate) fn storage_array_element_slot(
        slot: &ValueId,
        index: &ValueId,
        stride: &u64,
    ) -> Semantics<'static> {
        Semantics::StorageArrayElementSlot(*slot, *index, *stride)
    }

    /// Validate a storage byte-string header.
    pub(crate) fn validate_storage_bytes(header: &ValueId) -> Semantics<'static> {
        Semantics::ValidateStorageBytes(*header)
    }

    /// Load storage bytes into a newly reserved memory object.
    pub(crate) fn load_storage_bytes(slot: &ValueId) -> Semantics<'static> {
        Semantics::LoadStorageBytes(*slot)
    }

    /// Store a memory byte string.
    pub(crate) fn store_storage_bytes(slot: &ValueId, object: &ValueId) -> Semantics<'static> {
        Semantics::StoreStorageBytes(*slot, *object)
    }

    /// Store literal bytes.
    pub(crate) fn store_storage_bytes_literal<'a>(
        slot: &ValueId,
        bytes: &'a Bytes,
    ) -> Semantics<'a> {
        Semantics::StoreStorageBytesLiteral(*slot, bytes)
    }

    /// Clear a half-open range of storage data words.
    pub(crate) fn clear_storage_words(
        slot: &ValueId,
        first: &ValueId,
        last: &ValueId,
    ) -> Semantics<'static> {
        Semantics::ClearStorageWords(*slot, *first, *last)
    }
}

/// Returns one instruction of every operation whose declared semantics name an opcode, with
/// each pointer location and topic count. Only tests use it: they check the declarations against
/// the opcode table and instruction selection.
pub fn opcode_samples() -> Vec<InstKind> {
    let operands = |count: usize| (0..count).map(ValueId::from_usize).collect::<Vec<_>>();
    let mut kinds = InstKind::DECLARED_SEMANTICS
        .iter()
        .filter_map(|&(name, _)| InstKind::operand_only(name))
        .map(|(arity, build)| build(&operands(arity)))
        .collect::<Vec<_>>();
    let [a, b, c] = [0, 1, 2].map(ValueId::from_usize);
    for location in [
        PtrLocation::Memory,
        PtrLocation::Calldata,
        PtrLocation::Returndata,
        PtrLocation::Storage,
        PtrLocation::Transient,
    ] {
        kinds.extend([
            InstKind::Load(a, location),
            InstKind::Store(a, b, location),
            InstKind::Copy(a, b, c, location),
        ]);
    }
    for count in 0..=LogTopics::MAX {
        let topics = LogTopics::new(&operands(count)).unwrap();
        kinds.push(InstKind::Log { offset: a, size: b, topics });
    }
    kinds.retain(|kind| matches!(kind.semantics(), Some(Semantics::Opcode(..))));
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn declared_semantics() {
        let mut table = String::new();
        for (mnemonic, semantics) in InstKind::DECLARED_SEMANTICS {
            // `stringify!` breaks long declarations over several lines.
            let semantics = semantics.split_whitespace().collect::<Vec<_>>().join(" ");
            let semantics = if semantics.is_empty() { "-" } else { &semantics };
            writeln!(table, "{mnemonic}: {semantics}").unwrap();
        }
        snapbox::assert_data_eq!(table, snapbox::file!["semantics.snap"]);
    }

    #[test]
    fn opcode_samples_cover_every_opcode_declaration() {
        let sampled =
            opcode_samples().iter().map(|kind| kind.op_def().mnemonic).collect::<Vec<_>>();
        for (mnemonic, semantics) in InstKind::DECLARED_SEMANTICS {
            if semantics.starts_with("opcode") || semantics.starts_with("log") {
                assert!(sampled.contains(mnemonic), "{mnemonic}");
            }
        }
    }
}
