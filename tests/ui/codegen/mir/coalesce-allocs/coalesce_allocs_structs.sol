//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: combine 5 => 13
//@ run-call: combine 0 => 3

contract CoalesceAllocsFixture {
    struct Inner {
        uint256 lo;
        uint256 hi;
    }

    struct Outer {
        Inner first;
        Inner second;
        uint256 tag;
    }

    // The deferred outer allocation must not coalesce with its dynamic child
    // allocations.
    // CHECK-LABEL: fn @combine
    // CHECK: [[OUTER_PTR:v[0-9]+]] = alloc 96
    // CHECK: [[FIRST:v[0-9]+]] = mload 64
    // CHECK: mstore [[FIRST]], arg0
    // CHECK: [[SECOND:v[0-9]+]] = add [[FIRST]], 64
    // CHECK: mstore [[SECOND]], 2
    // CHECK: returndata
    function combine(uint256 x) public pure returns (uint256) {
        Outer memory outer = Outer(Inner(x, 1), Inner(2, x), 3);
        return outer.first.lo + outer.second.hi + outer.tag;
    }
}
