//@ revisions: default yul yul_gas yul_size sonatina sir llvm sonatina_gas sir_gas llvm_gas sonatina_size sir_size llvm_size
//@[default] compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@[default] filecheck:
//@[yul] compile-flags: --codegen-backend yul -Onone
//@[yul_gas] compile-flags: --codegen-backend yul -Ogas
//@[yul_size] compile-flags: --codegen-backend yul -Osize
//@[sonatina] compile-flags: --codegen-backend sonatina -Onone
//@[sir] compile-flags: --codegen-backend sir -Onone
//@[llvm] compile-flags: --codegen-backend llvm -Onone
//@[sonatina_gas] compile-flags: --codegen-backend sonatina -Ogas
//@[sir_gas] compile-flags: --codegen-backend sir -Ogas
//@[llvm_gas] compile-flags: --codegen-backend llvm -Ogas
//@[sonatina_size] compile-flags: --codegen-backend sonatina -Osize
//@[sir_size] compile-flags: --codegen-backend sir -Osize
//@[llvm_size] compile-flags: --codegen-backend llvm -Osize
//@ run-call: sum 1; constructor=[42] => 253
//@ run-call: sum 7; constructor=[42] => 385
//@ run-call: memoryProbe 0; constructor=[42] => 42
//@ run-call: memoryProbe 32; constructor=[42] => 42
//@ run-call: memoryProbe 224; constructor=[42] => 42
//@ run-call: memoryProbe 65536; constructor=[42] => 42
//@ run-call-fail: memoryProbe 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff; constructor=[42]
//@ run-call: zeroCopy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0; constructor=[42] => 42
//@ run-call: dataByte 0; constructor=[42] => 171
//@ run-call: dataByte 300; constructor=[42] => 42
//@ run-call: echo 123; constructor=[42] => 123
//@ run-call: virgin 0; constructor=[42] => 0
//@ run-call: virgin 32; constructor=[42] => 0
// solc 0.8.30 without --via-ir reports `Stack too deep` for this contract.
pragma solidity ^0.8.0;

contract StackTooDeepLocals {
    uint256 immutable marker;

    constructor(uint256 n) { marker = n; }

    function virgin(uint256 offset) external pure returns (uint256 result) {
        assembly { result := mload(offset) }
    }

    function memoryProbe(uint256 offset) external view returns (uint256 result) {
        uint256 expected = marker;
        assembly {
            mstore(offset, expected)
            result := mload(offset)
        }
    }

    function zeroCopy(uint256 offset, uint256 size) external view returns (uint256) {
        assembly {
            calldatacopy(offset, 0, size)
            returndatacopy(offset, 0, size)
            mcopy(offset, offset, size)
        }
        return marker;
    }

    function dataByte(uint256 index) external pure returns (uint8) {
        bytes memory data = hex"abababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababab2a";
        return uint8(data[index]);
    }

    function echo(uint256 value) external view returns (uint256 result) {
        assembly {
            let p := mload(0x40)
            mstore(p, value)
            if iszero(staticcall(gas(), 4, p, 32, p, 32)) { revert(0, 0) }
            result := mload(p)
        }
    }

    // CHECK-LABEL: @module StackTooDeepLocals_runtime
    // CHECK: push 0x188b85b4
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: push 21
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: mload
    // CHECK: add
    // CHECK: return
    function sum(uint256 x) external pure returns (uint256) {
        uint256 a0 = x + 0;
        uint256 a1 = x + 1;
        uint256 a2 = x + 2;
        uint256 a3 = x + 3;
        uint256 a4 = x + 4;
        uint256 a5 = x + 5;
        uint256 a6 = x + 6;
        uint256 a7 = x + 7;
        uint256 a8 = x + 8;
        uint256 a9 = x + 9;
        uint256 a10 = x + 10;
        uint256 a11 = x + 11;
        uint256 a12 = x + 12;
        uint256 a13 = x + 13;
        uint256 a14 = x + 14;
        uint256 a15 = x + 15;
        uint256 a16 = x + 16;
        uint256 a17 = x + 17;
        uint256 a18 = x + 18;
        uint256 a19 = x + 19;
        uint256 a20 = x + 20;
        uint256 a21 = x + 21;

        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10
            + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19 + a20 + a21;
    }
}
