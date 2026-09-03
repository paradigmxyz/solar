//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Nitro-style compact hashes can use the scratch area without allocating a
// temporary bytes object.
contract AbiEncodePackedStaticHash {
    // CHECK-LABEL: fn @hash{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = or 0x4d656d6f72793a{{.*}}, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = shl 72, {{v[0-9]+}}
    // CHECK: mstore 0, {{v[0-9]+}}
    // CHECK: mstore 23, arg2
    // CHECK: {{v[0-9]+}} = keccak256 0, 55
    function hash(uint64 size, uint64 maxSize, bytes32 root) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("Memory:", size, maxSize, root));
    }

    // CHECK-LABEL: fn @hashLocal{{[( ]}}
    // CHECK-NOT: alloc memorybytes
    // CHECK: mstore 0, {{v[0-9]+}}
    // CHECK: mstore 23, arg2
    // CHECK: {{v[0-9]+}} = keccak256 0, 55
    function hashLocal(uint64 size, uint64 maxSize, bytes32 root)
        external
        pure
        returns (bytes32)
    {
        bytes memory preimage = abi.encodePacked("Memory:", size, maxSize, root);
        return keccak256(preimage);
    }
}
