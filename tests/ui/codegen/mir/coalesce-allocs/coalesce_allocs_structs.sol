//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: combine(uint256) 5 => 13
//@[run] run-call: combine(uint256) 0 => 3

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

    // Constructing the nested value allocates the outer struct and both inner
    // structs back to back. The group fuses into one free-memory-pointer bump
    // of the summed size, and the inner pointers become constant offsets.
    // CHECK-LABEL: fn @combine
    // CHECK: [[BASE:v[0-9]+]] = mload 64
    // CHECK-NEXT: [[NEXT:v[0-9]+]] = add [[BASE]], 224
    // CHECK-NEXT: mstore 64, [[NEXT]]
    // CHECK-NOT: mstore 64,
    // CHECK: returndata
    function combine(uint256 x) public pure returns (uint256) {
        Outer memory outer = Outer(Inner(x, 1), Inner(2, x), 3);
        return outer.first.lo + outer.second.hi + outer.tag;
    }
}
