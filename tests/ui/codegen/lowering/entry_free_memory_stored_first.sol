//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: StoredFirst::f => 0xc0
//@ run-call: ObservedFirst::f => 0x80, 0xc0

// The slot's initial value matters only to a read that some path reaches before a store.

// The allocation stores the pointer before assembly reads it, so the entry skips the
// initialization: after the dispatch, `push 64` is the bump's address, not an `mstore`.
// CHECK-LABEL: @module StoredFirst_runtime
// CHECK: push 0x26121ff0
// CHECK-NEXT: sub
// CHECK-NEXT: push bb{{[0-9]+}}
// CHECK-NEXT: jumpi
// CHECK-NEXT: push 128
// CHECK-NEXT: push 64
// CHECK-NEXT: dup 2
contract StoredFirst {
    function f() external pure returns (uint256 p) {
        bytes memory b = new bytes(32);
        b[0] = 0x01;
        assembly ("memory-safe") {
            p := mload(0x40)
        }
    }
}

// The raw read comes first and sees the initial value, so the entry keeps the initialization.
// CHECK-LABEL: @module ObservedFirst_runtime
// CHECK: push 0x26121ff0
// CHECK-NEXT: sub
// CHECK-NEXT: push bb{{[0-9]+}}
// CHECK-NEXT: jumpi
// CHECK-NEXT: push 128
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 64
// CHECK-NEXT: mstore
contract ObservedFirst {
    function f() external pure returns (uint256 p, uint256 q) {
        assembly ("memory-safe") {
            p := mload(0x40)
        }
        bytes memory b = new bytes(32);
        b[0] = 0x01;
        assembly ("memory-safe") {
            q := mload(0x40)
        }
    }
}
