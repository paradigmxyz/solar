//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/smtCheckerTests/array_members/push_as_lhs_2d.sol
// ported-from: test/libsolidity/smtCheckerTests/array_members/push_as_lhs_3d.sol

contract Test {
    uint256[][] nested;
    uint256[][][] deeplyNested;
    bytes data;
    uint256[] arr;

    function testCallLvalue() external {
        nested.push().push() = 2;
        deeplyNested.push().push().push() = 3;
        data.push() = 0x01;
        data.push(0x01) = arr.pop(); //~ ERROR: expression has to be an lvalue
    }
}
