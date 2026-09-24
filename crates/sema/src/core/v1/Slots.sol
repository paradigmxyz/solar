// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Storage words laid out from a root slot, the way a dynamic storage
/// array lays out its elements.
/// @dev Compiler-owned module, imported as `solar:core/v1/Slots.sol`. A `Root`
/// occupies one slot like any one-word struct. Word `index` of the region it
/// roots is the slot `keccak256(slot) + index`, where a dynamic array stored
/// at that slot keeps its elements. Unlike an array's length, the root word is
/// whatever its owner stores there, which is what a packed layout that keeps
/// its length beside its first bytes needs.
///
/// An index must be below `2**64`, so a region never wraps around into the
/// slots of another variable; a larger index fails with `Panic(0x32)` before
/// any storage access. Solidity has no spelling of a slot derived from a
/// struct, so each body is one memory-safe assembly access; the compiler
/// lowers each call to the same hash and access by module identity.
library Slots {
    /// @dev One storage word that roots a region of words derived from its slot.
    struct Root {
        bytes32 word;
    }

    /// @dev The word `index` slots into the region `root` roots.
    function load(Root storage root, uint256 index) internal view returns (bytes32 value) {
        if (index >> 64 != 0) index = _outOfBounds();
        assembly ("memory-safe") {
            mstore(0x00, root.slot)
            value := sload(add(keccak256(0x00, 0x20), index))
        }
    }

    /// @dev Stores `value` `index` slots into the region `root` roots.
    function store(Root storage root, uint256 index, bytes32 value) internal {
        if (index >> 64 != 0) index = _outOfBounds();
        assembly ("memory-safe") {
            mstore(0x00, root.slot)
            sstore(add(keccak256(0x00, 0x20), index), value)
        }
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
