//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: bitwise(bytes1) 0x01 => 0x03, 0x01, 0x09
//@ run-call: bitwise(bytes1) 0xf1 => 0xf3, 0x01, 0xf9
//@ run-call: shifts(bytes1) 0x81 => 0x02, 0x40
//@ run-call: wide(bytes2) 0x0102 => 0x0103
//@ run-call: storageBoundary() => 0x09

contract FixedBytesCompoundAssignments {
    bytes private data;

    function bitwise(bytes1 value) external pure returns (bytes1 orValue, bytes1 andValue, bytes1 xorValue) {
        orValue = value;
        orValue |= 0x02;
        andValue = value;
        andValue &= 0x0f;
        xorValue = value;
        xorValue ^= 0x08;
    }

    function shifts(bytes1 value) external pure returns (bytes1 left, bytes1 right) {
        left = value;
        left <<= 1;
        right = value;
        right >>= 1;
    }

    function wide(bytes2 value) external pure returns (bytes2) {
        value |= 0x0001;
        return value;
    }

    function storageBoundary() external returns (bytes1) {
        data = new bytes(35);
        data[31] = 0x01;
        data[31] |= 0x08;
        return data[31];
    }
}
