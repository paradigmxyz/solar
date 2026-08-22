//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: chain 12, 3 => 15
//@[none, gas, size] run-call-fail: chain 12, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@[none, gas, size] run-call-fail: chain 11, 3
//@[none, gas, size] run-call: chain 0, 1 => 1

contract RequireModuloCases {
    function chain(uint256 a, uint256 b) external pure returns (uint256) {
        require(a % b == 0);
        require(b % 3 == 0 || a % 2 == 0);
        return a + b;
    }
}
