//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped

//@ run-call-fail: TailEntryObserverLoad::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: TailEntryObserverSize::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: TailEntryObserverGas::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: TailEntryObserverCopy::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: TailEntryObserverStorage::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: TailEntryObserverCall::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001

// CHECK-LABEL: @module TailEntryObserverLoad_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: mload
// CHECK: revert
// CHECK-LABEL: @module TailEntryObserverSize_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: msize
// CHECK: revert
// CHECK-LABEL: @module TailEntryObserverGas_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: gas
// CHECK: revert
// CHECK-LABEL: @module TailEntryObserverCopy_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: calldatacopy
// CHECK: revert
// CHECK-LABEL: @module TailEntryObserverStorage_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: sload
// CHECK: revert
// CHECK-LABEL: @module TailEntryObserverCall_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: push 192
// CHECK-NEXT: mload
// CHECK: staticcall
// CHECK: revert

contract TailEntryObserverLoad {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            let observed := mload(32)
            mstore(0, a)
            mstore(32, observed)
            revert(0, 64)
        }
    }
}

contract TailEntryObserverSize {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            let observed := msize()
            mstore(0, a)
            mstore(32, gt(observed, 0))
            revert(0, 64)
        }
    }
}

contract TailEntryObserverGas {
    function run(uint256 a) external view { fail(a); }
    function other(uint256 a) external view { fail(a); }
    function fail(uint256 a) internal view {
        assembly {
            let observed := gas()
            mstore(0, a)
            mstore(32, gt(observed, 0))
            revert(0, 64)
        }
    }
}

contract TailEntryObserverCopy {
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly {
            calldatacopy(32, 4, 32)
            mstore(0, a)
            mstore(32, mload(32))
            revert(0, 64)
        }
    }
}

contract TailEntryObserverStorage {
    function run(uint256 a) external view { fail(a); }
    function other(uint256 a) external view { fail(a); }
    function fail(uint256 a) internal view {
        assembly {
            let observed := sload(0)
            mstore(0, a)
            mstore(32, observed)
            revert(0, 64)
        }
    }
}

contract TailEntryObserverCall {
    function run(uint256 a) external view { fail(a); }
    function other(uint256 a) external view { fail(a); }
    function fail(uint256 a) internal view {
        assembly {
            let observed := staticcall(gas(), 4, 0, 0, 32, 0)
            mstore(0, a)
            mstore(32, observed)
            revert(0, 64)
        }
    }
}
