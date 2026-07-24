//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir --pretty-json
//@ filecheck:
// solc 0.8.30 without --via-ir reports `Stack too deep` for this contract.
pragma solidity ^0.8.0;

contract StackTooDeepParams {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x8c4ee692
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 640
    // CHECK-NEXT: sgt
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK: push 612
    // CHECK-NEXT: calldataload
    // CHECK: add
    // CHECK: return
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
    ) external pure returns (uint256) {
        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9
            + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19;
    }
}
