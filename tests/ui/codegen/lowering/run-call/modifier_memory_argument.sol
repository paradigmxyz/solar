//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
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
