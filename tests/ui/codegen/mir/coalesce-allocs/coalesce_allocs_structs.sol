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

    // The dynamic child allocations stay separate. The deferred outer allocation that nothing
    // reads is removed.
    // CHECK-LABEL: fn @combine
    // CHECK: [[X:v[0-9]+]] = calldataload 4
    // CHECK: [[FIRST:v[0-9]+]] = mload 64
    // CHECK: mstore [[FIRST]], [[X]]
    // CHECK: [[SECOND:v[0-9]+]] = add [[FIRST]], 64
    // CHECK: mstore [[SECOND]], 2
    // CHECK: returndata
    function combine(uint256 x) public pure returns (uint256) {
        Outer memory outer = Outer(Inner(x, 1), Inner(2, x), 3);
        return outer.first.lo + outer.second.hi + outer.tag;
    }
}
