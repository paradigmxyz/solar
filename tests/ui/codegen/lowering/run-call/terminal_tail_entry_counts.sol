//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped

//@ run-call-fail: TailEntryCount0::run() => 0x
// CHECK-LABEL: @module TailEntryCount1_runtime
// CHECK-NOT: mload
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[ONE:bb[0-9]+]]
// CHECK: [[ONE]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryCount12_runtime
// CHECK-NOT: mload
// CHECK: push 0x400
// CHECK-COUNT-12: mstore
// CHECK: push 384
// CHECK-NEXT: push 0x400
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryCount13_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 576
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[THIRTEEN:bb[0-9]+]]
// CHECK: [[THIRTEEN]] [cold]:
// CHECK-NEXT: push 192
// CHECK-NEXT: mload
// CHECK-COUNT-12: mload
// CHECK: push 0x400

contract TailEntryCount0 {
    function run() external pure { fail(); }
    function other() external pure { fail(); }
    function fail() internal pure {
        assembly {
            revert(0, 0)
        }
    }
}

//@ run-call-fail: TailEntryCount1::run 1 => 0x0000000000000000000000000000000000000000000000000000000000000001
contract TailEntryCount1 {
    function run(uint256 a0) external pure { fail(a0); }
    function other(uint256 a0) external pure { fail(a0); }
    function fail(uint256 a0) internal pure {
        assembly {
            mstore(0, a0)
            revert(0, 32)
        }
    }
}

//@ run-call-fail: TailEntryCount12::run 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12 => 0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000009000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c
contract TailEntryCount12 {
    function run(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11) external pure { fail(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11); }
    function other(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11) external pure { fail(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11); }
    function fail(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11) internal pure {
        assembly {
            mstore(1024, a0)
            mstore(1056, a1)
            mstore(1088, a2)
            mstore(1120, a3)
            mstore(1152, a4)
            mstore(1184, a5)
            mstore(1216, a6)
            mstore(1248, a7)
            mstore(1280, a8)
            mstore(1312, a9)
            mstore(1344, a10)
            mstore(1376, a11)
            revert(1024, 384)
        }
    }
}

//@ run-call-fail: TailEntryCount13::run 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13 => 0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000009000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000d
contract TailEntryCount13 {
    function run(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12) external pure { fail(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12); }
    function other(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12) external pure { fail(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12); }
    function fail(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12) internal pure {
        assembly {
            mstore(1024, a0)
            mstore(1056, a1)
            mstore(1088, a2)
            mstore(1120, a3)
            mstore(1152, a4)
            mstore(1184, a5)
            mstore(1216, a6)
            mstore(1248, a7)
            mstore(1280, a8)
            mstore(1312, a9)
            mstore(1344, a10)
            mstore(1376, a11)
            mstore(1408, a12)
            revert(1024, 416)
        }
    }
}
