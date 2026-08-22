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
//@[none, gas, size] run-call: f() => 0
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
