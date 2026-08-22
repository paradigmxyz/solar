//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: foo() => 0
// Solc's via-IR modifier frames reset return variables for each placeholder.

contract ModifierReturnResetViaIr {
    bool private active = true;

    modifier twice() {
        _;
        active = false;
        _;
    }

    function foo() external twice returns (uint256 result) {
        if (active) result = 1;
    }
}
