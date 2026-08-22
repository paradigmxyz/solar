//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f() => 0
// Solc's via-IR modifier frame semantics preserve the incoming return value
// when a modifier skips its placeholder.

contract ModifierSkippedPlaceholderViaIr {
    modifier skip() {
        if (true) return;
        _;
    }

    function f() external pure skip returns (uint256 r) {
        r = 7;
    }
}
