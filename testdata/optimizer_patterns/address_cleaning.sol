// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for address cleaning optimizations
/// @dev Solar should use shr(96, shl(96, x)) instead of and(x, mask)

contract AddressCleaning {
    /// @dev Clean address via cast - should optimize to shift pair
    /// Optimal: shr(96, shl(96, addr))
    function cleanAddress(address addr) public pure returns (address) {
        return address(uint160(addr));
    }

    /// @dev Function that receives address from untrusted source
    function processAddress(address user) public pure returns (address) {
        // Implicit cleaning should happen for address parameters
        return user;
    }

    /// @dev Extract address from bytes32
    /// Optimal for low bytes: shr(96, shl(96, data))
    function addressFromBytes32Low(bytes32 data) public pure returns (address) {
        return address(uint160(uint256(data)));
    }

    /// @dev Extract address from high bytes
    function addressFromBytes32High(bytes32 data) public pure returns (address) {
        return address(bytes20(data));
    }

    /// @dev Pack and unpack address with flag
    function packAddressWithFlag(address addr, bool flag) public pure returns (bytes32) {
        return bytes32(uint256(uint160(addr)) | (flag ? (1 << 255) : 0));
    }

    /// @dev Unpack address from packed value
    function unpackAddress(bytes32 packed) public pure returns (address) {
        return address(uint160(uint256(packed)));
    }

    /// @dev Check if address has dirty bits (for testing)
    function hasDirtyBits(uint256 value) public pure returns (bool) {
        return value != uint256(uint160(value));
    }
}
