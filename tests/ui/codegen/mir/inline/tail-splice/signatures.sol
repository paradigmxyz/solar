//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@ run-call: Spliced::sum 17, 23 => 40
//@ run-call: Spliced::diff 23, 17 => 6
//@ run-call: Spliced::mix 17, 23 => 6
//@ run-call-fail: Spliced::sum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Spliced::sum 17, 23; value=1
//@ run-call: Kept::ping()
//@ run-call: Kept::sum 17, 23 => 40

// Every route reads the same head words and only `entry` calls it, so the routes move into the
// switch.
// CHECK-LABEL: @module Spliced
// CHECK-LABEL: fn @entry()
// CHECK: switch
// CHECK-NOT: tail_call
contract Spliced {
    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }

    function diff(uint256 x, uint256 y) external pure returns (uint256) {
        return x - y;
    }

    function mix(uint256 x, uint256 y) external pure returns (uint256) {
        return x ^ y;
    }
}

// A route without arguments reads no word to share, so every route keeps its boundary.
// CHECK-LABEL: @module Kept
// CHECK-LABEL: fn @entry()
// CHECK: tail_call
contract Kept {
    function ping() external pure { }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }

    function diff(uint256 x, uint256 y) external pure returns (uint256) {
        return x - y;
    }
}
