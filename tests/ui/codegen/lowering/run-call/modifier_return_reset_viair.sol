//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: foo() => 0
//@[gas] run-call: foo() => 0
//@[size] run-call: foo() => 0
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
