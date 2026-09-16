// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Hashing a range of a buffer without copying it out first.
/// @dev Compiler-owned module, imported as `solar:core/v1/Hash.sol`. The body
/// copies the range into a fresh buffer and hashes that, which is the plain
/// Solidity spelling; the compiler hashes the range where it lies. Both check
/// the range against the buffer and raise `Panic(0x32)` when it does not fit.
library Hash {
    /// @dev The keccak-256 hash of the `count` bytes of `b` at `offset`.
    function keccak256Range(bytes memory b, uint256 offset, uint256 count)
        internal
        pure
        returns (bytes32)
    {
        if (count > b.length || offset > b.length - count) count = _outOfBounds();
        bytes memory range = new bytes(count);
        for (uint256 k; k < count; ++k) {
            range[k] = b[offset + k];
        }
        return keccak256(range);
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
