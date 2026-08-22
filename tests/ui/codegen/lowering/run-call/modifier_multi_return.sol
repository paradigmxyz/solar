//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f false => 1
//@[none, gas, size] run-call: f true => 1
// Solc's via-IR modifier frames reset return variables per placeholder.

contract ModifierMultiReturn {
    modifier repeat(bool twice) {
        if (twice) _;
        _;
    }

    function f(bool twice) external pure repeat(twice) returns (uint256 r) {
        r += 1;
        return r;
    }
}
