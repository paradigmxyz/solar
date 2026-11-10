//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir-final
//@[gas] filecheck: --check-prefix=GAS
//@ run-call: words [1, 2, 3] => [1, 2, 3]
//@ run-call: words [] => []
//@ run-call-fail: 0xdc95fc7e0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000ffffffffffffffff
//@ run-call-fail: 0xdc95fc7e000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000007
//@ run-call: bytesNarrow [1, 2, 255] => [1, 2, 255]
//@ run-call-fail: 0x2b933a3b000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000100
pragma solidity ^0.8.0;

// The decoder validates a `uint256[]` argument one word at a time, but a word has no invalid
// values, so the loop has no effect and is skipped. A `uint8[]` argument keeps its loop, which
// rejects out-of-range elements.
contract DeadValidationLoop {
    // GAS-LABEL: fn @words
    // GAS-NOT: phi
    // GAS: returndata
    function words(uint256[] calldata a) external pure returns (uint256[] calldata) {
        return a;
    }

    // GAS-LABEL: fn @bytesNarrow
    // GAS: phi
    function bytesNarrow(uint8[] calldata a) external pure returns (uint8[] calldata) {
        return a;
    }
}
