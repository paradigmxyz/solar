//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Nitro-style compact hashes should stage small all-static packed data in
// scratch memory and coalesce adjacent sub-word writes into one word store.
contract AbiEncodePackedStaticHash {
    // CHECK-LABEL: fn @hash
    // CHECK: {{v[0-9]+}} = shl 136, arg0
    // CHECK: {{v[0-9]+}} = shl 72, arg1
    // CHECK: mstore 0, {{v[0-9]+}}
    // CHECK: mstore 23, arg2
    // CHECK: keccak256 0, 55
    function hash(uint64 size, uint64 maxSize, bytes32 root) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("Memory:", size, maxSize, root));
    }
}
