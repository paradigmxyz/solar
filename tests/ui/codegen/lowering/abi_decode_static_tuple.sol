//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiDecodeStaticTuple {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: {{v[0-9]+}} = lt [[LEN]], 96
    // CHECK: {{v[0-9]+}} = mload
    // CHECK: {{v[0-9]+}} = iszero
    // CHECK: {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}, {{v[0-9]+}}
    function decode(bytes memory data) external pure returns (uint256 a, bool b, address c) {
        return abi.decode(data, (uint256, bool, address));
    }
}
