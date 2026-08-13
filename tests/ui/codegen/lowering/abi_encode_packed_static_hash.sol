//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Nitro-style compact hashes should coalesce adjacent sub-word writes into one
// word store in a semantic bytes object.
contract AbiEncodePackedStaticHash {
    // CHECK-LABEL: fn @hash{{[( ]}}
    // CHECK: {{v[0-9]+}} = shl 136, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = shl 72, {{v[0-9]+}}
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, {{v[0-9]+}}
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, arg2
    // CHECK: keccak256_bytes {{v[0-9]+}}
    function hash(uint64 size, uint64 maxSize, bytes32 root) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("Memory:", size, maxSize, root));
    }
}
