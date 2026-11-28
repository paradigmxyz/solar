// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

type W is uint256;

contract Base {
    function g(W x) internal pure returns (uint256) { return W.unwrap(x) + 1; }
    function h(string memory) internal pure returns (uint256) { return 10; }
}

// g(uint256) and h(bytes) are new overloads, not overrides: Base's g(W) and h(string) stay
// callable and are the ones picked for W and string arguments.
contract Derived is Base {
    function g(uint256 x) internal pure returns (uint256) { return x + 2; }
    function h(bytes memory) internal pure returns (uint256) { return 20; }

    /// @custom:ensures r == 7
    function bad_udvt() public pure returns (uint256 r) { r = g(W.wrap(5)); }

    /// @custom:ensures r == 6
    // Refused rather than resolved by bare name.
    // expect: cannot tell the overloads
    function bad_true_spec_udvt() public pure returns (uint256 r) { r = g(W.wrap(5)); }

    /// @custom:ensures r == 20
    function bad_str() public pure returns (uint256 r) { r = h(string("a")); }

    /// @custom:ensures r == 10
    // Refused rather than resolved by bare name.
    // expect: cannot tell the overloads
    function bad_true_spec_str() public pure returns (uint256 r) { r = h(string("a")); }
}
