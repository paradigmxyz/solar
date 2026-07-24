//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir --pretty-json
//@ filecheck:
// solc 0.8.30 without --via-ir reports `Stack too deep` for this contract.
pragma solidity ^0.8.0;

contract StackTooDeepCall {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x2b096926
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: push 19
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: mload
    // CHECK: add
    // CHECK: return
    function call(uint256 x) external pure returns (uint256) {
        return sum(
            x + 0,
            x + 1,
            x + 2,
            x + 3,
            x + 4,
            x + 5,
            x + 6,
            x + 7,
            x + 8,
            x + 9,
            x + 10,
            x + 11,
            x + 12,
            x + 13,
            x + 14,
            x + 15,
            x + 16,
            x + 17,
            x + 18,
            x + 19
        );
    }

    function sum(
        uint256 a0,
        uint256 a1,
        uint256 a2,
        uint256 a3,
        uint256 a4,
        uint256 a5,
        uint256 a6,
        uint256 a7,
        uint256 a8,
        uint256 a9,
        uint256 a10,
        uint256 a11,
        uint256 a12,
        uint256 a13,
        uint256 a14,
        uint256 a15,
        uint256 a16,
        uint256 a17,
        uint256 a18,
        uint256 a19
    ) internal pure returns (uint256) {
        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9
            + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19;
    }
}
