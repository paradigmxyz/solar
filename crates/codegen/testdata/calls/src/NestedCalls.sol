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

    // ========== External call variants (using this.) ==========

    function inner(uint256 x) external pure returns (uint256) {
        return x * 2;
    }

    function outer(uint256 x) external view returns (uint256) {
        return this.inner(this.inner(x)); // Should return x * 4
    }

    // Bitwise operations for nested external call testing
    function bitwiseAnd(uint256 a, uint256 b) external pure returns (uint256) {
        return a & b;
    }

    function bitwiseOr(uint256 a, uint256 b) external pure returns (uint256) {
        return a | b;
    }

    function mixedBitwise(uint256 a, uint256 b) external view returns (uint256) {
        // bitwiseOr(bitwiseAnd(a, 0xF0), bitwiseAnd(b, 0x0F))
        return this.bitwiseOr(this.bitwiseAnd(a, 0xF0), this.bitwiseAnd(b, 0x0F));
    }

    // Shift operations
    function shiftLeft(uint256 a, uint256 bits) external pure returns (uint256) {
        return a << bits;
    }

    function shiftRight(uint256 a, uint256 bits) external pure returns (uint256) {
        return a >> bits;
    }

    function nestedShifts(uint256 x) external view returns (uint256) {
        // shiftRight(shiftLeft(1, 8), 4) = 256 >> 4 = 16
        return this.shiftRight(this.shiftLeft(1, 8), 4);
    }
}
