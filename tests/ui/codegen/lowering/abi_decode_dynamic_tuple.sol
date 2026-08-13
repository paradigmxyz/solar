//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeDynamicTuple {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: abi_decode [u256, bytes, bytes], arg0
    // CHECK: ret
    function decode(bytes memory data)
        external
        pure
        returns (uint256 a, string memory s, bytes memory b)
    {
        return abi.decode(data, (uint256, string, bytes));
    }

    // CHECK-LABEL: fn @roundtrip{{[( ]}}
    // CHECK: abi_encode [word, memory_bytes, memory_bytes], args arg0, arg1, arg2
    // CHECK: memory_object_copy_from_slice memorybytes
    // CHECK: abi_decode [u256, bytes, bytes]
    function roundtrip(uint256 a, string memory s, bytes memory b)
        external
        pure
        returns (uint256, string memory, bytes memory)
    {
        return abi.decode(abi.encode(a, s, b), (uint256, string, bytes));
    }

    // CHECK-LABEL: fn @decodeBytes{{[( ]}}
    // CHECK: abi_decode [bytes], arg0
    // CHECK: ret
    function decodeBytes(bytes memory data) external pure returns (bytes memory) {
        return abi.decode(data, (bytes));
    }
}
