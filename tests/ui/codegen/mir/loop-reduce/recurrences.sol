//@ codegen-matrix: standard
//@ run-call: sum 7, 3, 5, 0 => 7, 3, 0
//@ run-call: sum 7, 3, 5, 1 => 10, 8, 1
//@ run-call: sum 7, 3, 5, 2 => 18, 13, 2
//@ run-call: sum 7, 3, 5, 5 => 72, 28, 5
//@ run-call: sum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 3, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 4 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 4
//@[gas] run-call: sum 7, 3, 5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x8000000000000000000000000000000000000000000000000000000000000009, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call: sum 7, 3, 5, 0x100000000000000000000000000000000 => 0x8000000000000000000000000000000080000000000000000000000000000007, 0x500000000000000000000000000000003, 0x100000000000000000000000000000000
//@ run-call: descending 100, 17, 3, 4 => 50, 5
//@ run-call: descending 100, 17, 3, 5 => 45, 2
//@ run-call: linear 9, 7, 4 => 37
//@ run-call: checked 5, 7, 3 => 26
//@ run-call-fail: checked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: checkedConstant 4 => 8
//@ run-call-fail: checkedConstant 0x8000000000000000000000000000000000000000000000000000000000000000; gas=100000
pragma solidity ^0.8.0;

contract Recurrences {
    function sum(uint256 x, uint256 y, uint256 step, uint256 n) external pure returns (uint256, uint256, uint256 i) {
        assembly {
            for {} lt(i, n) { i := add(i, 1) } {
                x := add(y, x)
                y := add(step, y)
            }
        }
        return (x, y, i);
    }

    function descending(uint256 x, uint256 y, uint256 step, uint256 n) external pure returns (uint256, uint256) {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                x := sub(x, y)
                y := sub(y, step)
            }
        }
        return (x, y);
    }

    function linear(uint256 x, uint256 step, uint256 n) external pure returns (uint256) {
        unchecked {
            for (uint256 i; i < n; ++i) x += step;
        }
        return x;
    }

    function checked(uint256 x, uint256 step, uint256 n) external pure returns (uint256) {
        for (uint256 i; i < n; ++i) x += step;
        return x;
    }

    function checkedConstant(uint256 n) external pure returns (uint256 x) {
        for (uint256 i; i < n; ++i) x += 2;
    }
}
