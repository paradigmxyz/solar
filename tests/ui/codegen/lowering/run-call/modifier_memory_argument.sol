//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f(uint256[]) [7] => 7
//@ run-call: f(uint256[]) [1, 2] => 1

contract ModifierMemoryArgument {
    modifier rewrite(uint256[] memory values) {
        values[0] = 9;
        _;
    }

    function f(uint256[] calldata values) external pure rewrite(values) returns (uint256) {
        return values[0];
    }
}
