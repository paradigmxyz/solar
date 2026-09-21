// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;
contract Counter {

    function ensure(bool ok) internal pure { require(ok); }
    function checkedAdd(uint256 a, uint256 b) internal pure returns (uint256 c) {
        unchecked { c = a + b; } if (c < a) panicArithmetic();
    }
    function checkedSub(uint256 a, uint256 b) internal pure returns (uint256) {
        if (b > a) panicArithmetic(); unchecked { return a - b; }
    }
    function checkedMul(uint256 a, uint256 b) internal pure returns (uint256 c) {
        unchecked { c = a * b; } if (a != 0 && c / a != b) panicArithmetic();
    }
    function panicArithmetic() internal pure {
        assembly { mstore(0, shl(224, 0x4e487b71)) mstore(4, 0x11) revert(0, 36) }
    }

    uint256 private value;
    function get() public view returns (uint256) { return value; }
    function increment(uint256 n) public returns (uint256) { value = checkedAdd(value, n); return value; }
    function subtract(uint256 n) public returns (uint256) { value = checkedSub(value, n); return value; }
    function accumulate(uint256 n) public returns (uint256) {
        ensure(n <= 1000);
        uint256 acc = value;
        for (uint256 i = 0; i < n; i = checkedAdd(i, 1)) acc = checkedAdd(acc, i);
        value = acc; return acc;
    }

}
