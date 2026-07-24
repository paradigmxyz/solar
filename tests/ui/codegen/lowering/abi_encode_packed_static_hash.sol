//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// Nitro-style compact hashes should stage small all-static packed data in
// scratch memory and coalesce adjacent sub-word writes into one word store.
contract AbiEncodePackedStaticHash {
    function hash(uint64 size, uint64 maxSize, bytes32 root) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("Memory:", size, maxSize, root));
    }
}
