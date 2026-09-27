// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract C {}

library L {
    enum Kind { A, B }
    struct S { C c; Kind k; uint256 n; }

    /// @notice Emitted values
    /// @dev Event details
    /// @param c Contract value
    /// @param k Enum value
    /// @param s Struct value
    event E(C c, Kind k, S s);

    /// @notice Rejected values
    /// @dev Error details
    /// @param c Contract value
    /// @param k Enum value
    /// @param s Struct value
    error F(C c, Kind k, S s);

    /// @notice Library function
    /// @dev Function details
    function f(C c, Kind k, S memory s) external pure returns (uint256) {
        return uint256(uint160(address(c))) + uint256(k) + s.n;
    }
}
