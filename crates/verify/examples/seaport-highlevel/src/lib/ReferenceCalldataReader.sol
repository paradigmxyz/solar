// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/**
 * @title ReferenceCalldataReader
 * @notice Reads calldata the way seaport-core's decoders do, in high-level
 *         Solidity: a word at any offset, zero-padded past the end of
 *         calldata (as CALLDATALOAD and CALLDATACOPY are), and head offsets
 *         and lengths masked to their low four bytes (OffsetOrLengthMask).
 *         Pointers are absolute calldata offsets, including the selector.
 */
contract ReferenceCalldataReader {
    uint256 internal constant _OFFSET_OR_LENGTH_MASK = 0xffffffff;

    /// @dev The word at calldata offset p, zero-padded past the end.
    function _cdWord(uint256 p) internal pure returns (uint256) {
        uint256 size = msg.data.length;
        if (p >= size) {
            return 0;
        }
        uint256 end = size - p < 32 ? size : p + 32;
        return uint256(bytes32(msg.data[p:end]));
    }

    /// @dev The low four bytes of the word at p, as an offset or length.
    function _cdMasked(uint256 p) internal pure returns (uint256) {
        return _cdWord(p) & _OFFSET_OR_LENGTH_MASK;
    }

    /// @dev The pointer the head at p + headOffset refers to, relative to p.
    function _cdPptrOffset(uint256 p, uint256 headOffset)
        internal
        pure
        returns (uint256)
    {
        unchecked {
            return p + _cdMasked(p + headOffset);
        }
    }
}
