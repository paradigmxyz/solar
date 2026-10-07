//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: checked 1, 2, 3, 4 => 84
//@ run-call-fail: checked 0x8000000000000000000000000000000000000000000000000000000000000000, 0x8000000000000000000000000000000000000000000000000000000000000000, 3, 0x8000000000000000000000000000000000000000000000000000000000000000; gas=23000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
pragma solidity ^0.8.0;

contract Exp {
    function checked(uint256 a, uint256 b, uint256 base, uint256 exponent) external pure returns (uint256) {
        uint256 sum = a + b;
        uint256 power;
        assembly { power := exp(base, exponent) }
        return sum + power;
    }
}
