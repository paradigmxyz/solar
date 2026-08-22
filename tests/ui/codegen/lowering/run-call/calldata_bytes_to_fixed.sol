//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: first(bytes) 0x0102 => 0x01
//@[none, gas, size] run-call: half(bytes) 0x010203 => 0x01020300000000000000000000000000
//@[none, gas, size] run-call: word(bytes) 0x => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[none, gas, size] run-call: word(bytes) 0x01 => 0x0100000000000000000000000000000000000000000000000000000000000000
//@[none, gas, size] run-call: word(bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e00
//@[none, gas, size] run-call: word(bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@[none, gas, size] run-call: word(bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@[none, gas, size] run-call: sliced(bytes,uint256,uint256) 0xff010203, 1, 4 => 0x01020300000000000000000000000000

contract CalldataBytesToFixed {
    function first(bytes calldata value) external pure returns (bytes1) {
        return bytes1(value);
    }

    function half(bytes calldata value) external pure returns (bytes16) {
        return bytes16(value);
    }

    function word(bytes calldata value) external pure returns (bytes32) {
        return bytes32(value);
    }

    function sliced(bytes calldata value, uint256 start, uint256 end)
        external
        pure
        returns (bytes16)
    {
        return bytes16(value[start:end]);
    }
}
