//@ codegen-matrix: standard
//@ run-call: create2Hash 0xff, 0x00000000000000000000000000000000deadbeef, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0x7e44f2066486f02244e17d57503863e329e8f7026fddfb425df323adae9a0ccb
//@ run-call: mixedHash 1, 0xabcdef, -2, 0x1234 => 0xb1cd30ad25f7d2be89dc9703ecf6b389b4cf04ecfa1caddee93b06a7859c9cd1
//@ run-call: fixedFirstHash 0xabcdef, 1, 2 => 0xa11ad7472875a4b0a030a317416ce8e0010dc12f63c2f47efcfef9026daa99d5
//@ run-call: selectorWord 0xdeadbeef, 7 => 0xdeadbeef00000007

// Narrow fixed-bytes values share packed words with their neighbours.
contract PackedFixedBytes {
    function create2Hash(bytes1 prefix, address deployer, bytes32 salt)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, deployer, salt));
    }

    function mixedHash(uint8 a, bytes3 b, int16 c, bytes2 d) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c, d));
    }

    function fixedFirstHash(bytes3 a, uint8 b, uint8 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    function selectorWord(bytes4 selector, uint32 value) external pure returns (bytes memory) {
        return abi.encodePacked(selector, value);
    }
}
