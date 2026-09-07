//@ codegen-matrix: standard ir raw
//@[ir] filecheck: --enable-var-scope
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck: --enable-var-scope
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped



//@ run-call-fail: TailEntryRangeBefore::run 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff
//@ run-call-fail: TailEntryRangeWord::run 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: TailEntryRangePartial::run 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffff
//@ run-call-fail: TailEntryRangeAfter::run 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff
//@ run-call-fail: TailEntryRangeWrapping::run 7 => 0x
//@ run-call-fail: TailEntryRangeVariable::run 1024 => 0x0000000000000000000000000000000000000000000000000000000000000400

// CHECK-LABEL: @module TailEntryRangeBefore_runtime
// CHECK-NOT: mload
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[BODY:bb[0-9]+]]
// CHECK: [[BODY]] [cold]:
// CHECK-NEXT: push 191
// CHECK-NEXT: mstore
// CHECK: push 1
// CHECK-NEXT: push 191
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryRangeWord_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: push 32
// CHECK-NEXT: push 192
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryRangePartial_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: push 2{{$}}
// CHECK-NEXT: push 223
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryRangeAfter_runtime
// CHECK-NOT: mload
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[BODY:bb[0-9]+]]
// CHECK: [[BODY]] [cold]:
// CHECK-NEXT: push 224
// CHECK-NEXT: mstore
// CHECK: push 1
// CHECK-NEXT: push 224
// CHECK-NEXT: revert
// CHECK-LABEL: @module TailEntryRangeWrapping_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: {{push 2[[:space:]]+push 0[[:space:]]+not[[:space:]]+revert}}
// CHECK-LABEL: @module TailEntryRangeVariable_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: revert

contract TailEntryRangeBefore {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(191, a)
            revert(191, 1)
        }
    }
}

contract TailEntryRangeWord {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(192, a)
            revert(192, 32)
        }
    }
}

contract TailEntryRangePartial {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(223, a)
            revert(223, 2)
        }
    }
}

contract TailEntryRangeAfter {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(224, a)
            revert(224, 1)
        }
    }
}

contract TailEntryRangeWrapping {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(0, a)
            revert(115792089237316195423570985008687907853269984665640564039457584007913129639935, 2)
        }
    }
}

contract TailEntryRangeVariable {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            mstore(a, a)
            revert(a, 32)
        }
    }
}
