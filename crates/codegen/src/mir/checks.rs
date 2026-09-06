//! Semantic failures retained until conditional checks are expanded.

use solar_interface::{Symbol, sym};

/// Solidity's built-in `Panic(uint256)` error codes.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PanicCode {
    Assert = 0x01,
    ArithmeticOverflowUnderflow = 0x11,
    DivisionByZero = 0x12,
    EnumConversion = 0x21,
    StorageEncoding = 0x22,
    EmptyArrayPop = 0x31,
    ArrayOutOfBounds = 0x32,
    MemoryAllocationOverflow = 0x41,
    InvalidInternalFunction = 0x51,
}

impl PanicCode {
    pub(crate) const fn from_u64(value: u64) -> Option<Self> {
        Some(match value {
            0x01 => Self::Assert,
            0x11 => Self::ArithmeticOverflowUnderflow,
            0x12 => Self::DivisionByZero,
            0x21 => Self::EnumConversion,
            0x22 => Self::StorageEncoding,
            0x31 => Self::EmptyArrayPop,
            0x32 => Self::ArrayOutOfBounds,
            0x41 => Self::MemoryAllocationOverflow,
            0x51 => Self::InvalidInternalFunction,
            _ => return None,
        })
    }

    pub(crate) const fn as_u64(self) -> u64 {
        self as u64
    }
}

/// Why a revert with no user-supplied payload fires.
///
/// These reverts carry no data by default. With `--revert-strings debug`, each reason other than
/// [`RevertReason::Empty`] is encoded as an `Error(string)` payload with the same message solc
/// attaches to the corresponding check, so a failing transaction explains which internal check
/// rejected it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RevertReason {
    /// Empty data in every mode: `require` and `revert()` without a message, stripped messages,
    /// and checks solc never attaches a message to, such as decoded ABI word validators.
    Empty,
    /// A non-payable external entry point received Ether.
    EtherSentToNonPayable,
    /// The selector did not match any external function and no fallback exists, but the
    /// contract has a `receive` function.
    UnknownSelector,
    /// The call matched nothing and the contract has neither a fallback nor a `receive`.
    NoFallbackNorReceive,
    /// ABI-encoded input ends before the static head of a tuple.
    TupleDataTooShort,
    /// A tuple element offset points outside the encoded input.
    InvalidTupleOffset,
    /// A dynamic array or `bytes` head offset points outside the encoded input.
    InvalidCalldataArrayOffset,
    /// A dynamic array or `bytes calldata` length exceeds the encodable range.
    InvalidCalldataArrayLength,
    /// A dynamic array's element data does not fit the encoded input.
    InvalidCalldataArrayStride,
    /// A `bytes` or `string` decoded to memory does not fit the encoded input.
    InvalidByteArrayLength,
    /// A struct member offset exceeds the encodable range.
    InvalidStructOffset,
    /// Calldata ends before the static head of a struct.
    StructCalldataTooShort,
    /// ABI-encoded memory data ends before the static head of a struct.
    StructDataTooShort,
    /// A calldata array element or struct member offset is out of range while re-encoding.
    InvalidCalldataAccessOffset,
    /// A calldata array element length exceeds the encodable range while re-encoding.
    InvalidCalldataAccessLength,
    /// A calldata array element's data does not fit in calldata while re-encoding.
    InvalidCalldataAccessStride,
    /// A calldata tail element offset is out of range.
    InvalidCalldataTailOffset,
    /// A calldata tail element length exceeds the encodable range.
    InvalidCalldataTailLength,
    /// A calldata tail element's data does not fit in calldata.
    CalldataTailTooShort,
    /// A slice end exceeds the sliced value's length.
    SliceGreaterThanLength,
    /// A slice starts after its end.
    SliceStartsAfterEnd,
    /// An external call target has no code.
    TargetContractHasNoCode,
    /// A non-view library function was called directly instead of through `DELEGATECALL`.
    LibraryCalledWithoutDelegatecall,
}

impl RevertReason {
    pub(crate) const fn name(self) -> Symbol {
        match self {
            Self::Empty => sym::empty,
            Self::EtherSentToNonPayable => sym::ether_sent_to_non_payable,
            Self::UnknownSelector => sym::unknown_selector,
            Self::NoFallbackNorReceive => sym::no_fallback_nor_receive,
            Self::TupleDataTooShort => sym::tuple_data_too_short,
            Self::InvalidTupleOffset => sym::invalid_tuple_offset,
            Self::InvalidCalldataArrayOffset => sym::invalid_calldata_array_offset,
            Self::InvalidCalldataArrayLength => sym::invalid_calldata_array_length,
            Self::InvalidCalldataArrayStride => sym::invalid_calldata_array_stride,
            Self::InvalidByteArrayLength => sym::invalid_byte_array_length,
            Self::InvalidStructOffset => sym::invalid_struct_offset,
            Self::StructCalldataTooShort => sym::struct_calldata_too_short,
            Self::StructDataTooShort => sym::struct_data_too_short,
            Self::InvalidCalldataAccessOffset => sym::invalid_calldata_access_offset,
            Self::InvalidCalldataAccessLength => sym::invalid_calldata_access_length,
            Self::InvalidCalldataAccessStride => sym::invalid_calldata_access_stride,
            Self::InvalidCalldataTailOffset => sym::invalid_calldata_tail_offset,
            Self::InvalidCalldataTailLength => sym::invalid_calldata_tail_length,
            Self::CalldataTailTooShort => sym::calldata_tail_too_short,
            Self::SliceGreaterThanLength => sym::slice_greater_than_length,
            Self::SliceStartsAfterEnd => sym::slice_starts_after_end,
            Self::TargetContractHasNoCode => sym::target_contract_has_no_code,
            Self::LibraryCalledWithoutDelegatecall => sym::library_called_without_delegatecall,
        }
    }

    pub(crate) fn from_name(name: Symbol) -> Option<Self> {
        Some(match name {
            sym::empty => Self::Empty,
            sym::ether_sent_to_non_payable => Self::EtherSentToNonPayable,
            sym::unknown_selector => Self::UnknownSelector,
            sym::no_fallback_nor_receive => Self::NoFallbackNorReceive,
            sym::tuple_data_too_short => Self::TupleDataTooShort,
            sym::invalid_tuple_offset => Self::InvalidTupleOffset,
            sym::invalid_calldata_array_offset => Self::InvalidCalldataArrayOffset,
            sym::invalid_calldata_array_length => Self::InvalidCalldataArrayLength,
            sym::invalid_calldata_array_stride => Self::InvalidCalldataArrayStride,
            sym::invalid_byte_array_length => Self::InvalidByteArrayLength,
            sym::invalid_struct_offset => Self::InvalidStructOffset,
            sym::struct_calldata_too_short => Self::StructCalldataTooShort,
            sym::struct_data_too_short => Self::StructDataTooShort,
            sym::invalid_calldata_access_offset => Self::InvalidCalldataAccessOffset,
            sym::invalid_calldata_access_length => Self::InvalidCalldataAccessLength,
            sym::invalid_calldata_access_stride => Self::InvalidCalldataAccessStride,
            sym::invalid_calldata_tail_offset => Self::InvalidCalldataTailOffset,
            sym::invalid_calldata_tail_length => Self::InvalidCalldataTailLength,
            sym::calldata_tail_too_short => Self::CalldataTailTooShort,
            sym::slice_greater_than_length => Self::SliceGreaterThanLength,
            sym::slice_starts_after_end => Self::SliceStartsAfterEnd,
            sym::target_contract_has_no_code => Self::TargetContractHasNoCode,
            sym::library_called_without_delegatecall => Self::LibraryCalledWithoutDelegatecall,
            _ => return None,
        })
    }

    /// The message solc attaches to this check with `--revert-strings debug`, if any.
    pub(crate) const fn message(self) -> Option<&'static str> {
        Some(match self {
            Self::Empty => return None,
            Self::EtherSentToNonPayable => "Ether sent to non-payable function",
            Self::UnknownSelector => "Unknown signature and no fallback defined",
            Self::NoFallbackNorReceive => "Contract does not have fallback nor receive functions",
            Self::TupleDataTooShort => "ABI decoding: tuple data too short",
            Self::InvalidTupleOffset => "ABI decoding: invalid tuple offset",
            Self::InvalidCalldataArrayOffset => "ABI decoding: invalid calldata array offset",
            Self::InvalidCalldataArrayLength => "ABI decoding: invalid calldata array length",
            Self::InvalidCalldataArrayStride => "ABI decoding: invalid calldata array stride",
            Self::InvalidByteArrayLength => "ABI decoding: invalid byte array length",
            Self::InvalidStructOffset => "ABI decoding: invalid struct offset",
            Self::StructCalldataTooShort => "ABI decoding: struct calldata too short",
            Self::StructDataTooShort => "ABI decoding: struct data too short",
            Self::InvalidCalldataAccessOffset => "Invalid calldata access offset",
            Self::InvalidCalldataAccessLength => "Invalid calldata access length",
            Self::InvalidCalldataAccessStride => "Invalid calldata access stride",
            Self::InvalidCalldataTailOffset => "Invalid calldata tail offset",
            Self::InvalidCalldataTailLength => "Invalid calldata tail length",
            Self::CalldataTailTooShort => "Calldata tail too short",
            Self::SliceGreaterThanLength => "Slice is greater than length",
            Self::SliceStartsAfterEnd => "Slice starts after end",
            Self::TargetContractHasNoCode => "Target contract does not contain code",
            Self::LibraryCalledWithoutDelegatecall => {
                "Non-view function of library called without DELEGATECALL"
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RevertKind {
    Panic(PanicCode),
    Reason(RevertReason),
}
