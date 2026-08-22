//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: test() => 55
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_array_copy.sol

contract StorageBoundaryArrayCopy {
    function getX() internal pure returns (uint256[10][1] storage array) {
        assembly {
            array.slot := sub(0, 5)
        }
    }

    function getY() internal pure returns (uint256[10][1] storage array) {
        assembly {
            array.slot := 5
        }
    }

    function test() public returns (uint256 sum) {
        uint256[10][1] storage x = getX();
        uint256[10][1] storage y = getY();
        x[0] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        y[0] = x[0];
        for (uint256 i = 0; i < y[0].length; ++i) sum += y[0][i];
    }
}
