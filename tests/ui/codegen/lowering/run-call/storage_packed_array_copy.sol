//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test() => 36, 36
// ported-from: test/libsolidity/semanticTests/storage/storage_packed_array_copy.sol

contract StoragePackedArrayCopy {
    bytes8[9] x;
    bytes17[10] y;

    constructor() {
        for (uint256 i = 0; i < x.length; ++i) x[i] = bytes8(uint64(i));
        y[8] = bytes8(uint64(2));
        y[9] = bytes8(uint64(2));
    }

    function test() public returns (uint256 sumX, uint256 sumY) {
        y = x;
        for (uint256 i = 0; i < x.length; ++i) sumX += uint64(x[i]);
        for (uint256 i = 0; i < y.length; ++i) sumY += uint64(bytes8(y[i]));
    }
}
