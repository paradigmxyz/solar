// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract NestedCalls {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }

    // Nested calls as arguments
    function nested2(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        return add(mul(a, b), c); // mul(a,b) + c
    }

    function nested3(uint256 a, uint256 b, uint256 c, uint256 d) public pure returns (uint256) {
        return add(add(a, b), add(c, d)); // (a+b) + (c+d)
    }

    function deepNested(uint256 x) public pure returns (uint256) {
        return add(add(add(x, 1), 2), 3); // ((x+1)+2)+3
    }
}
