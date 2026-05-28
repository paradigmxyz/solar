// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for branchless conditional optimizations
/// @dev Solar should optimize these to avoid JUMPI where possible

contract BranchlessTernary {
    /// @dev Simple ternary that could be optimized to:
    /// z := xor(x, mul(xor(x, y), iszero(condition)))
    function ternaryUint(bool condition, uint256 x, uint256 y) public pure returns (uint256) {
        return condition ? x : y;
    }

    /// @dev Same pattern with bytes32
    function ternaryBytes32(bool condition, bytes32 x, bytes32 y) public pure returns (bytes32) {
        return condition ? x : y;
    }

    /// @dev Same pattern with address
    function ternaryAddress(bool condition, address x, address y) public pure returns (address) {
        return condition ? x : y;
    }

    /// @dev Coalesce pattern: return x if non-zero, else y
    /// Optimal: z := or(x, mul(y, iszero(x)))
    function coalesceUint(uint256 x, uint256 y) public pure returns (uint256) {
        return x != 0 ? x : y;
    }

    /// @dev Coalesce for bytes32
    function coalesceBytes32(bytes32 x, bytes32 y) public pure returns (bytes32) {
        return x != bytes32(0) ? x : y;
    }

    /// @dev Coalesce for address
    function coalesceAddress(address x, address y) public pure returns (address) {
        return x != address(0) ? x : y;
    }

    /// @dev Min function - branchless version:
    /// z := xor(y, mul(xor(y, x), lt(x, y)))
    function min(uint256 x, uint256 y) public pure returns (uint256) {
        return x < y ? x : y;
    }

    /// @dev Max function - branchless version:
    /// z := xor(y, mul(xor(y, x), gt(x, y)))
    function max(uint256 x, uint256 y) public pure returns (uint256) {
        return x > y ? x : y;
    }

    /// @dev Abs difference - branchless version possible
    function absDiff(uint256 x, uint256 y) public pure returns (uint256) {
        return x > y ? x - y : y - x;
    }

    /// @dev Boolean to uint conversion
    /// Optimal: z := iszero(iszero(b)) or just: z := b (if b is already 0/1)
    function boolToUint(bool b) public pure returns (uint256) {
        return b ? 1 : 0;
    }

    /// @dev Sign function for int
    function sign(int256 x) public pure returns (int256) {
        if (x > 0) return 1;
        if (x < 0) return -1;
        return 0;
    }
}
