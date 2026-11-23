//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` the overflow checks of a function share one panic block, since cfg-simplify shares
// no blocks.
contract C {
    // CHECK-LABEL: fn @f(
    // CHECK: = sub
    // CHECK-NEXT: = lt
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[PANIC:bb[0-9]+]],
    // CHECK: = add
    // CHECK-NEXT: = lt
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[PANIC]],
    // CHECK: [[PANIC]] [cold]:
    function f(uint256 x, uint256 y, bool c) external pure returns (uint256) {
        if (c) {
            return x + y;
        }
        return x - y;
    }
}
