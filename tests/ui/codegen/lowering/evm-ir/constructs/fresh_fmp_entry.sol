//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --implicit-check-not=mload
//@[run] compile-flags: -Ogas
//@ run-call: lookup 0x => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: lookup 0x1234 => 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
//@ run-call: lookup 0xabababababababababababababababababababababababababababababababababababababababab => 0xb1775312b2633a4a45fe7eeca3bc95f5c3fd760a80564c97329f9e5bb3d762e1

contract FreshFmpEntry {
    // The copy is the entry's first allocation, so its pointer is the heap floor and the entry
    // never loads the slot. Nothing reads the bumped pointer, so the entry neither stores it nor
    // initializes the slot.
    // CHECK-LABEL: @module FreshFmpEntry_runtime
    // CHECK: push 0xc3b2556d
    // CHECK-NEXT: sub
    // CHECK-NEXT: push bb{{[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 36
    // CHECK-NOT: push 64
    // CHECK: calldatacopy
    function lookup(bytes calldata name) external pure returns (bytes32) {
        bytes memory copy = name;
        return keccak256(copy);
    }
}
