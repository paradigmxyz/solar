//@ revisions: mir run
//@[mir] compile-flags: -Ogas -Zdump=mir-final
//@[mir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: copy => 304
//@[run] run-call: keep 4 => 5
//@[run] run-call-fail: grow 0xffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
// An allocation that starts at the heap floor and has a constant size cannot overflow the 64-bit
// allocation limit, since codegen keeps the floor far below `2^64`. The copy of a storage struct
// reads its source into an allocation that lowering turns into plain loads and stores, and the
// overflow test of that allocation goes with it. An allocation whose size the caller picks keeps
// its test.

contract DeadAllocationCheck {
    struct S {
        uint256 x;
        uint256 y;
    }

    S s;
    S t;

    // CHECK-LABEL: fn @strct(
    // CHECK-NOT: heap_floor
    // CHECK-NOT: jumpi
    // CHECK-NOT: mstore 32, 65
    // CHECK: stop
    function strct() external {
        s = t;
    }

    // CHECK-LABEL: fn @copy(
    // CHECK-NOT: heap_floor
    // CHECK-NOT: mstore 32, 65
    function copy() external returns (uint256) {
        t.x = 3;
        t.y = 4;
        s = t;
        return s.x * 100 + s.y;
    }

    // CHECK-LABEL: fn @keep(
    // CHECK-NOT: heap_floor
    // CHECK-NOT: mstore 32, 65
    function keep(uint256 a) external pure returns (uint256) {
        uint256[4] memory m;
        m[0] = a;
        return m[0] + 1;
    }

    // CHECK-LABEL: fn @grow(
    // CHECK: heap_floor
    // CHECK: mstore 32, 65
    function grow(uint256 n) external pure returns (uint256) {
        return new uint256[](n).length;
    }
}
