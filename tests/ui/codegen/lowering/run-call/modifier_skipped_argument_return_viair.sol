//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f() => 0
// Solc's via-IR modifier frames discard argument-side return writes when the
// placeholder is skipped.

contract ModifierSkippedArgumentReturnViaIr {
    modifier skip(uint256 value) {
        if (value == 7) return;
        _;
    }

    function f() external pure skip(r = 7) returns (uint256 r) {}
}
