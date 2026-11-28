// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Mutual recursion through internal function pointers: bad_a and bad_b call each other forever,
// so every call runs out of gas (or overflows the EVM stack) and reverts. succeeds_if true is false.
contract PtrRecursion {
    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function bad_a(uint256 x) public pure returns (uint256 result) {
        function(uint256) internal pure returns (uint256) p = bad_b;
        result = p(x);
    }

    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function bad_b(uint256 x) public pure returns (uint256 result) {
        function(uint256) internal pure returns (uint256) p = bad_a;
        result = p(x);
    }

    /// The same recursion by name: the verifier sees it and lets the call revert.
    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function direct_a(uint256 x) public pure returns (uint256 result) {
        result = direct_b(x);
    }

    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function direct_b(uint256 x) public pure returns (uint256 result) {
        result = direct_a(x);
    }

    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function five(uint256) internal pure returns (uint256 result) {
        result = 5;
    }

    /// A pointer call to a terminating function with a succeeds_if: true.
    /// @custom:succeeds_if true
    /// @custom:ensures result == 5
    function ok_ptr(uint256 x) public pure returns (uint256 result) {
        function(uint256) internal pure returns (uint256) p = five;
        result = p(x);
    }
}
