//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: test() => 0x00000000000000010000, 0x00000000000000020000, 0x00000000000000030000, 0x00000000000000040000, 0x00000000000000050000
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_different_packing.sol

contract StorageArrayDifferentPacking {
    bytes8[] data1;
    bytes10[] data2;

    function test()
        public
        returns (bytes10 a, bytes10 b, bytes10 c, bytes10 d, bytes10 e)
    {
        data1 = new bytes8[](9);
        for (uint256 i = 0; i < data1.length; ++i) data1[i] = bytes8(uint64(i));
        data2 = data1;
        a = data2[1];
        b = data2[2];
        c = data2[3];
        d = data2[4];
        e = data2[5];
    }
}
