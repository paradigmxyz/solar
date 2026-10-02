//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@ run-call: ping()
//@ run-call: sum 17, 23 => 40
//@ run-call: headLength [1, 2, 3] => 3
//@ run-call: headLength [] => 0
//@ run-call-fail: sum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: sum 17, 23; value=1
//@ run-call-fail: 0xffffffff

// CHECK-LABEL: @module Signatures
// CHECK-LABEL: fn @entry()
// CHECK: calldataload 4
// CHECK: switch
// CHECK-NOT: tail_call
contract Signatures {
    function ping() external pure { }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }

    function headLength(uint256[] calldata x) external pure returns (uint256) {
        return x.length;
    }
}
