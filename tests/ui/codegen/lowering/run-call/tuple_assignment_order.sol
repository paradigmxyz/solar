//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: swap() => 2, 1

contract TupleAssignmentOrder {
    function swap() external pure returns (uint256, uint256) {
        uint256 a = 1;
        uint256 b = 2;
        (a, b) = (b, a);
        return (a, b);
    }
}
