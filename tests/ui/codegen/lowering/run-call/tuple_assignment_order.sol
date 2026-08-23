//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: swap() => 2, 1

contract TupleAssignmentOrder {
    function swap() external pure returns (uint256, uint256) {
        uint256 a = 1;
        uint256 b = 2;
        (a, b) = (b, a);
        return (a, b);
    }
}
