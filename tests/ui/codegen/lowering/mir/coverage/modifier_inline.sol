//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A struct constructor helper is lowered into the code that calls it, and counts in its own
// right even when the call is in the body of a modifier, whose expansion has counters of its own.
struct S {
    uint256 a;
}

contract Inlined {
    uint256 x;

    function mk(uint256 a) internal pure returns (S memory) {
        return S(a > 1 ? a : 1);
    }

    modifier m(uint256 v) {
        x = mk(v).a;
        _;
    }

    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // The entry of the function, and the entry of the expansion of the modifier.
    // CHECK-NEXT: coverage 4
    // CHECK-NEXT: coverage 5
    // The entry of the helper, the leaf of its ternary, and its path, as in any other function.
    // CHECK-NEXT: coverage 1
    // CHECK: [[COND:v[0-9]+]] = gt arg0, 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: jumpi [[COND]], [[TRUE:bb[0-9]+]],
    // CHECK: [[TRUE]]:
    // CHECK-NEXT: coverage 3
    // The placeholder of the modifier.
    // CHECK: coverage 6
    function f(uint256 v) external m(v) {}
}
