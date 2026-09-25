// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice ABI encoding into memory a caller already owns.
/// @dev Compiler-owned module, imported as `solar:core/v1/Abi.sol`. This is a
/// source library over `Bytes`, and other compilers run it as written.
///
/// Every function writes at an offset measured from the start of the encoding,
/// not from the backing allocation, and touches only the bytes it encodes:
/// whatever else `out` holds is preserved. A value narrower than a word is
/// written as its cleaned word, so the padding an ABI reader expects is
/// initialized rather than inherited from the buffer.
///
/// `encodeInto` writes the value types below. A dynamic value's head is an
/// offset to a tail whose position depends on the whole tuple, so a tuple goes
/// through `writeEncoding`, with the `abi.encode` call written as its argument:
/// `Abi.writeEncoding(out, offset, abi.encode(a, b))`. The body copies the
/// encoding `abi.encode` allocates; this compiler stages that encoding past the
/// free memory pointer without allocating it, and copies it from there. Either
/// way the encoding is complete before `out` changes, so an argument that
/// aliases `out` is encoded as it was.
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

    /// @dev Writes the bytes of `encoding` at `offset`. Returns the bytes
    /// written. Pass an `abi.encode`, `abi.encodeWithSelector`,
    /// `abi.encodeWithSignature`, or `abi.encodeCall` call to skip allocating it.
    function writeEncoding(bytes memory out, uint256 offset, bytes memory encoding)
        internal
        pure
        returns (uint256 written)
    {
        written = encoding.length;
        Bytes.copyInto(out, offset, encoding, 0, written);
    }

    /// @dev `writeEncoding`, reporting a write that does not fit instead of
    /// failing.
    function tryWriteEncoding(bytes memory out, uint256 offset, bytes memory encoding)
        internal
        pure
        returns (bool ok, uint256 written)
    {
        if (!fits(out, offset, encoding.length)) return (false, 0);
        return (true, writeEncoding(out, offset, encoding));
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
