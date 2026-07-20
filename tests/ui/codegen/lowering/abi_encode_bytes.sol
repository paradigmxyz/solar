//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// `abi.encode(...)` allocates a fresh `bytes memory` `[length][data...]` from
// the free memory pointer; it must never stage argument words at absolute low
// memory (which clobbers the free memory pointer at 0x40 with 3+ words).
// `keccak256(abi.encode(...))` hashes the encoding staged at the unbumped free
// memory pointer without materializing a `bytes` object.
contract AbiEncodeBytes {
    function hash3(uint a, uint b, uint c) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b, c));
    }

    function encode3(uint a, uint b, uint c) external pure returns (bytes memory) {
        return abi.encode(a, b, c);
    }

    function encodeDynamic(uint a, string memory s) external pure returns (bytes memory) {
        return abi.encode(a, s);
    }

    function hashDynamic(uint a, string memory s) external pure returns (bytes32) {
        return keccak256(abi.encode(a, s));
    }

    function roundtrip(uint a) external pure returns (uint) {
        return abi.decode(abi.encode(a), (uint));
    }
}
