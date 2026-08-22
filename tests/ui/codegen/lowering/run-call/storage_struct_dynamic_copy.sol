//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: copy => 2550103
//@[none, gas, size] run-call: arrayCopy => 103
// ported-from: test/libsolidity/semanticTests/structs/struct_containing_bytes_copy_and_delete.sol

contract StorageStructDynamicCopy {
    struct S {
        uint256 x;
        bytes b;
        uint256 y;
    }

    S s1;
    S s2;
    S[] list;

    function copy() external returns (uint256) {
        s1.x = 7;
        s1.b = hex"010203";
        s1.y = 13;
        s2 = s1;
        s1.x = 9;
        s1.b[0] = bytes1(0xff);
        s1.y = 15;
        return uint256(uint8(s1.b[0])) * 10000 + uint256(uint8(s2.b[0])) * 100
            + s2.b.length;
    }

    function arrayCopy() external returns (uint256) {
        list.push();
        list.push();
        list[0].x = 7;
        list[0].b = hex"010203";
        list[0].y = 13;
        list[1] = list[0];
        list[0].b[0] = bytes1(0xff);
        return uint256(uint8(list[1].b[0])) * 100 + list[1].b.length;
    }
}
