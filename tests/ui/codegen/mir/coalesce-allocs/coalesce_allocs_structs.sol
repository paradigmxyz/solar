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

    // The outer allocation and its dynamic children coalesce into one heap
    // region once the outer stays dynamic: a later free-memory-pointer read
    // observes the bump, so the static deferral no longer applies.
    // CHECK-LABEL: fn @combine
    // CHECK: [[OUTER:v[0-9]+]] = mload 64
    // CHECK: [[SIZE:v[0-9]+]] = add [[OUTER]], 224
    // CHECK: mstore 64, [[SIZE]]
    // CHECK: [[CHILD:v[0-9]+]] = add [[OUTER]], 96
    // CHECK: returndata
    function combine(uint256 x) public pure returns (uint256) {
        Outer memory outer = Outer(Inner(x, 1), Inner(2, x), 3);
        return outer.first.lo + outer.second.hi + outer.tag;
    }
}
