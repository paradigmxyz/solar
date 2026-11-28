//@ codegen-matrix: standard
//@ run-call: words [1, 2, 3] => [1, 2, 3]
//@ run-call: words [] => []
//@ run-call: triple [1, 2, 3] => 6
//@ run-call: narrow [1, 2, 255] => [1, 2, 255]
//@ run-call-fail: 0xdc95fc7e0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000ffffffffffffffff
//@ run-call-fail: 0xdc95fc7e000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: 0x2b933a3b000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000100
pragma solidity ^0.8.0;

// A word array has no invalid element, so its decoder checks only the array's range and a `uint256[]`
// argument costs no time per element. A `uint8[]` argument still rejects an element above 255.
contract AbiCalldataWordArrayNoLoop {
    function words(uint256[] calldata a) external pure returns (uint256[] calldata) {
        return a;
    }

    function triple(uint256[3] calldata a) external pure returns (uint256) {
        return a[0] + a[1] + a[2];
    }

    function narrow(uint8[] calldata a) external pure returns (uint8[] calldata) {
        return a;
    }
}
