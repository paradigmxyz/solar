// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Hashing a range of a buffer without copying it out first.
/// @dev Compiler-owned module, imported as `solar:core/Hash.sol`. The reviewed
/// raw operation hashes in place without allocating, including with solc.
/// The full range is checked first, raising `Panic(0x32)` when it does not fit.
library Hash {
    /// @dev The keccak-256 hash of the `count` bytes of `b` at `offset`.
    /// @custom:solar-trusted
    function keccak256Range(bytes memory b, uint256 offset, uint256 count)
        internal
        pure
        returns (bytes32 result)
    {
        if (count > b.length || offset > b.length - count) count = _outOfBounds();
        assembly ("memory-safe") {
            result := keccak256(add(add(b, 32), offset), count)
        }
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
