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
/// any storage access. The byte operations move a range of bytes between a
/// buffer and the region's words, 32 bytes to a word from word 0; a count must
/// be below `2**69`, so every word they touch has an index below `2**64`, and
/// the range must lie in the buffer, or the call fails with `Panic(0x32)`
/// before any access. Solidity has no spelling of a slot derived from a
/// struct, so each body is memory-safe assembly; the compiler lowers each call
/// to the same hash and accesses by module identity.
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

    /// @dev Stores `count` bytes of `b` from `offset` into the region `root`
    /// roots, 32 bytes to a word from word 0. The last word's bytes past
    /// `count` are zero.
    function storeBytes(Root storage root, bytes memory b, uint256 offset, uint256 count) internal {
        if (count >> 69 != 0 || offset > b.length || count > b.length - offset) {
            count = _outOfBounds();
        }
        assembly ("memory-safe") {
            mstore(0x00, root.slot)
            let base := keccak256(0x00, 0x20)
            let src := add(add(b, 0x20), offset)
            for { let k := 0 } lt(shl(5, k), count) { k := add(k, 1) } {
                let w := mload(add(src, shl(5, k)))
                let rest := sub(count, shl(5, k))
                if lt(rest, 0x20) { w := and(w, not(shr(shl(3, rest), not(0)))) }
                sstore(add(base, k), w)
            }
        }
    }

    /// @dev Stores `count` bytes of `b` from `offset` into the region `root`
    /// roots, 32 bytes to a word from word 0. The last word's bytes past
    /// `count` are zero.
    function storeCalldataBytes(Root storage root, bytes calldata b, uint256 offset, uint256 count)
        internal
    {
        if (count >> 69 != 0 || offset > b.length || count > b.length - offset) {
            count = _outOfBounds();
        }
        assembly ("memory-safe") {
            mstore(0x00, root.slot)
            let base := keccak256(0x00, 0x20)
            let src := add(b.offset, offset)
            for { let k := 0 } lt(shl(5, k), count) { k := add(k, 1) } {
                let w := calldataload(add(src, shl(5, k)))
                let rest := sub(count, shl(5, k))
                if lt(rest, 0x20) { w := and(w, not(shr(shl(3, rest), not(0)))) }
                sstore(add(base, k), w)
            }
        }
    }

    /// @dev Writes `count` bytes of the region `root` roots, from the start of
    /// word 0, into `b` at `offset`. The other bytes of `b` stay unchanged.
    function loadBytes(Root storage root, bytes memory b, uint256 offset, uint256 count)
        internal
        view
    {
        if (count >> 69 != 0 || offset > b.length || count > b.length - offset) {
            count = _outOfBounds();
        }
        assembly ("memory-safe") {
            mstore(0x00, root.slot)
            let base := keccak256(0x00, 0x20)
            let dst := add(add(b, 0x20), offset)
            for { let k := 0 } lt(shl(5, k), count) { k := add(k, 1) } {
                let w := sload(add(base, k))
                let at := add(dst, shl(5, k))
                let rest := sub(count, shl(5, k))
                if lt(rest, 0x20) {
                    let keep := shr(shl(3, rest), not(0))
                    w := or(and(w, not(keep)), and(mload(at), keep))
                }
                mstore(at, w)
            }
        }
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
