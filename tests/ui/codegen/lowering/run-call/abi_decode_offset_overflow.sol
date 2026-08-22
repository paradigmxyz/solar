//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call-fail: test()
//@ run-call-fail: withinArray()
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_offset_overflow_in_array_3.sol
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_offset_overflow_in_array_2.sol

contract AbiDecodeOffsetOverflow {
    struct MemoryUint {
        uint256 field;
    }

    struct MemoryTuple {
        uint256 field1;
        uint256 field2;
    }

    function test() external pure returns (uint256) {
        uint256[] memory before = new uint256[](1);
        bytes memory corrupt = abi.encode(
            uint256(32),
            uint256(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80),
            uint256(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80)
        );
        MemoryUint memory afterCorrupt;
        afterCorrupt.field = uint256(
            0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
        );
        before[0] = 123456;
        uint256[][2] memory decoded = abi.decode(corrupt, (uint256[][2]));
        return decoded[1][0];
    }

    function withinArray() external pure returns (uint256) {
        uint256[] memory before = new uint256[](1);
        bytes memory corrupt = abi.encode(uint256(32), uint256(2));
        MemoryTuple memory afterCorrupt;
        before[0] = 123456;
        afterCorrupt.field1 = uint256(
            0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff60
        );
        afterCorrupt.field2 = uint256(
            0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff60
        );
        uint256[][] memory decoded = abi.decode(corrupt, (uint256[][]));
        return decoded[0][0] + decoded[1][0];
    }
}
