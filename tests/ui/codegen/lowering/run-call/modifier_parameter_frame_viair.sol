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
//@[none, gas, size] run-call: f 0 => 0
// Solc's via-IR modifier frame semantics reset parameters per placeholder.

contract ModifierParameterFrameViaIr {
    modifier twice() {
        _;
        _;
    }

    function f(uint256 a) external pure twice returns (uint256 r) {
        r = a++;
    }
}
