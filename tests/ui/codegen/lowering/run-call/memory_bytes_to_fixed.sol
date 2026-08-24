//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: shorten(bytes,uint256) 0x01020304, 0 => 0x00000000000000000000000000000000
//@ run-call: shorten(bytes,uint256) 0x01020304, 2 => 0x01020000000000000000000000000000
//@ run-call: shorten(bytes,uint256) 0x0102030405060708090a0b0c0d0e0f1011, 15 => 0x0102030405060708090a0b0c0d0e0f00
//@ run-call: shorten(bytes,uint256) 0x0102030405060708090a0b0c0d0e0f1011, 17 => 0x0102030405060708090a0b0c0d0e0f10
// ported-from: test/libsolidity/semanticTests/array/bytes_to_fixed_bytes_cleanup.sol

contract MemoryBytesToFixed {
    function shorten(bytes memory value, uint256 length) external pure returns (bytes16) {
        assembly ("memory-safe") {
            mstore(value, length)
        }
        return bytes16(value);
    }
}
