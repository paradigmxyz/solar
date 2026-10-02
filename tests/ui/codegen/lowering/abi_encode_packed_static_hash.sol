//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Nitro-style compact hashes can use the scratch area without allocating a
// temporary bytes object.
contract AbiEncodePackedStaticHash {
    // CHECK-LABEL: fn @hash{{[( ]}}
    // CHECK: keccak256_packed (data hex"4d656d6f72793a", u64 {{v[0-9]+}}, u64 {{v[0-9]+}}, bytes32 arg2)
    function hash(uint64 size, uint64 maxSize, bytes32 root) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("Memory:", size, maxSize, root));
    }

    // CHECK-LABEL: fn @hashLocal{{[( ]}}
    // CHECK: keccak256_packed (data hex"4d656d6f72793a", u64 {{v[0-9]+}}, u64 {{v[0-9]+}}, bytes32 arg2)
    function hashLocal(uint64 size, uint64 maxSize, bytes32 root)
        external
        pure
        returns (bytes32)
    {
        bytes memory preimage = abi.encodePacked("Memory:", size, maxSize, root);
        return keccak256(preimage);
    }
}
