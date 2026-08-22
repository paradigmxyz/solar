//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f 0 => 0
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
