// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice ABI encoding into memory a caller already owns.
/// @dev Compiler-owned module, imported as `solar:core/v1/Abi.sol`. This is an
/// ordinary source library over `Bytes`; nothing here is lowered specially.
///
/// Every function writes at an offset measured from the start of the encoding,
/// not from the backing allocation, and touches only the bytes it encodes:
/// whatever else `out` holds is preserved. A value narrower than a word is
/// written as its cleaned word, so the padding an ABI reader expects is
/// initialized rather than inherited from the buffer.
///
/// The value types below are the supported set. A dynamic value's head is an
/// offset to a tail whose position depends on the whole tuple, so encoding one
/// needs the tuple form this module does not have; build such an encoding with
/// `abi.encode` instead. Because every argument here is a value type, no input
/// can alias `out`.
///
/// A write that does not fit fails with `Panic(0x32)` before modifying `out`;
/// the `try` forms report that instead, and write nothing. Neither form undoes
/// side effects of evaluating the arguments, which happens before the call.
library Abi {
    /// @dev Bytes one ABI word occupies.
    uint256 internal constant WORD = 32;

    /// @dev Bytes a function selector occupies.
    uint256 internal constant SELECTOR = 4;

    /// @dev Writes `value` at `offset`. Returns the bytes written.
    function encodeInto(bytes memory out, uint256 offset, uint256 value)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes32(out, offset, bytes32(value));
        return WORD;
    }

    /// @dev Writes `value` at `offset`, sign-extended to a word.
    function encodeInto(bytes memory out, uint256 offset, int256 value)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes32(out, offset, bytes32(uint256(value)));
        return WORD;
    }

    /// @dev Writes `value` at `offset`, zero-padded to a word.
    function encodeInto(bytes memory out, uint256 offset, address value)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes32(out, offset, bytes32(uint256(uint160(value))));
        return WORD;
    }

    /// @dev Writes `value` at `offset` as zero or one.
    function encodeInto(bytes memory out, uint256 offset, bool value)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes32(out, offset, bytes32(uint256(value ? 1 : 0)));
        return WORD;
    }

    /// @dev Writes `value` at `offset`, left-aligned as the ABI defines.
    function encodeInto(bytes memory out, uint256 offset, bytes32 value)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes32(out, offset, value);
        return WORD;
    }

    /// @dev Writes `selector` at `offset`. Returns the bytes written.
    function encodeSelectorInto(bytes memory out, uint256 offset, bytes4 selector)
        internal
        pure
        returns (uint256 written)
    {
        Bytes.writeBytes4(out, offset, selector);
        return SELECTOR;
    }

    /// @dev `encodeInto`, reporting a write that does not fit instead of failing.
    function tryEncodeInto(bytes memory out, uint256 offset, uint256 value)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, WORD)) return (false, 0);
        return (true, encodeInto(out, offset, value));
    }

    /// @dev `encodeInto`, reporting a write that does not fit instead of failing.
    function tryEncodeInto(bytes memory out, uint256 offset, int256 value)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, WORD)) return (false, 0);
        return (true, encodeInto(out, offset, value));
    }

    /// @dev `encodeInto`, reporting a write that does not fit instead of failing.
    function tryEncodeInto(bytes memory out, uint256 offset, address value)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, WORD)) return (false, 0);
        return (true, encodeInto(out, offset, value));
    }

    /// @dev `encodeInto`, reporting a write that does not fit instead of failing.
    function tryEncodeInto(bytes memory out, uint256 offset, bool value)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, WORD)) return (false, 0);
        return (true, encodeInto(out, offset, value));
    }

    /// @dev `encodeInto`, reporting a write that does not fit instead of failing.
    function tryEncodeInto(bytes memory out, uint256 offset, bytes32 value)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, WORD)) return (false, 0);
        return (true, encodeInto(out, offset, value));
    }

    /// @dev `encodeSelectorInto`, reporting a write that does not fit instead
    /// of failing.
    function tryEncodeSelectorInto(bytes memory out, uint256 offset, bytes4 selector)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, SELECTOR)) return (false, 0);
        return (true, encodeSelectorInto(out, offset, selector));
    }

    /// @dev Whether `count` bytes at `offset` lie inside `out`. The sum is
    /// checked, so an offset near the top of the word reports a miss rather
    /// than wrapping into range.
    function fits(bytes memory out, uint256 offset, uint256 count)
        internal
        pure
        returns (bool)
    {
        unchecked {
            uint256 end = offset + count;
            if (end < offset) return false;
            return end <= out.length;
        }
    }
}
