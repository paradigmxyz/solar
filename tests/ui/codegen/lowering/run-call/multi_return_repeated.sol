//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test() => 1

library MultiReturnRepeatedLib {
    function sort(uint256 a, uint256 b) internal pure returns (uint256, uint256) {
        return a < b ? (a, b) : (b, a);
    }
}

contract MultiReturnRepeated {
    function test() external pure returns (uint256) {
        (uint256 a, uint256 b) = MultiReturnRepeatedLib.sort(2, 1);
        (uint256 c, uint256 d) = MultiReturnRepeatedLib.sort(4, 3);
        return a == 1 && b == 2 && c == 3 && d == 4 ? 1 : 0;
    }
}
