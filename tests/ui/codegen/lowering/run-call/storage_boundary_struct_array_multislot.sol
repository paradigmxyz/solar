//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: test() => 45
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_struct_array_multislot.sol

contract StorageBoundaryStructArrayMultislot {
    struct S {
        uint256 a;
        uint256 b;
        uint256 c;
    }

    function getSource() internal pure returns (S[3][1] storage array) {
        assembly {
            array.slot := sub(0, 5)
        }
    }

    function getDest() internal pure returns (S[3][1] storage array) {
        assembly {
            array.slot := 5
        }
    }

    function test() public returns (uint256 sum) {
        S[3][1] storage source = getSource();
        S[3][1] storage dest = getDest();
        for (uint256 i = 0; i < 3; ++i) {
            source[0][i] = S({a: i * 3 + 1, b: i * 3 + 2, c: i * 3 + 3});
        }
        dest[0] = source[0];
        for (uint256 i = 0; i < 3; ++i) {
            sum += dest[0][i].a + dest[0][i].b + dest[0][i].c;
        }
    }
}
