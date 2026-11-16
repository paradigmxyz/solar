//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: guard 1 =>
//@ run-call-fail: guard 0 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000473746f7000000000000000000000000000000000000000000000000000000000
//@ run-call: pass 5 => 5

// An `Error(string)` revert stores its length at the free-memory-pointer word, and the revert
// reads that word back from the same block, so a contract whose only memory use is string reverts
// never initializes the free-memory pointer.
// CHECK-LABEL: @module RevertStringSkipsFmpInit_runtime
// CHECK-NOT: push 128
// CHECK: push 100
// CHECK-NEXT: push 28
// CHECK-NEXT: revert
// CHECK-NOT: push 128
contract RevertStringSkipsFmpInit {
    function guard(uint256 x) external pure {
        require(x != 0, "stop");
    }

    function pass(uint256 x) external pure returns (uint256) {
        require(x != 0, "stop");
        return x;
    }
}
