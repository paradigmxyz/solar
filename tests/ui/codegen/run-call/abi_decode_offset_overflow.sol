//@ run-call-fail: test()
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_offset_overflow_in_array_3.sol

contract AbiDecodeOffsetOverflow {
    struct MemoryUint {
        uint256 field;
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
}
