// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

library Pub {
    function f(uint256 a) public pure returns (uint256) {
        return a + 1;
    }

    function g(uint256 a) internal pure returns (uint256) {
        return f(a);
    }
}

// Pub.f is public: the call runs the deployed library by DELEGATECALL.
contract DelegateLib {
    function run(uint256 a) external pure returns (uint256) {
        return Pub.f(a);
    }
}

// Pub.g calls f by name inside the library: an internal call, inlined.
contract InlineLib {
    function run(uint256 a) external pure returns (uint256) {
        return Pub.g(a);
    }
}
