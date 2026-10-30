// SPDX-License-Identifier: MIT
pragma solidity ^0.8.4;

/// @dev Benchmark replacement for Solady's gas- and code-dependent test helper.
/// The runner installs this body in the pinned test source for every compiler.
abstract contract FairBrutalizer {
    /// @dev Dirties scratch space and sixteen free-memory words without reserving them.
    /// The zero slot and free memory pointer stay intact for the upstream checks.
    function _brutalizeMemory() internal pure {
        uint256 zero;
        /// @solidity memory-safe-assembly
        assembly {
            let pattern := 0xa5c3e187694b2d0ff1d3b597795b3d1f13579bdf2468ace00123456789abcdef
            mstore(zero, pattern)
            mstore(0x20, not(pattern))
            let free := mload(0x40)
            for { let i := 0 } lt(i, 0x200) { i := add(i, 0x20) } {
                mstore(add(free, i), xor(pattern, i))
            }
        }
    }
}
