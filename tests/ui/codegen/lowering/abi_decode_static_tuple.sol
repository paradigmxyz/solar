//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiDecodeStaticTuple {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: {{v[0-9]+}} = lt [[LEN]], 96
    // CHECK: [[SLICE:v[0-9]+]] = make_memory_slice {{v[0-9]+}}, 32
    // CHECK: {{v[0-9]+}} = memory_slice_load_word memory, [[SLICE]], 0
    // CHECK: {{v[0-9]+}} = iszero
    // CHECK: {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}, {{v[0-9]+}}
    function decode(bytes memory data) external pure returns (uint256 a, bool b, address c) {
        return abi.decode(data, (uint256, bool, address));
    }
}
