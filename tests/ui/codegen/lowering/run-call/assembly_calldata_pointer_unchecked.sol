//@ codegen-matrix: standard
//@ run-call-fail: 0x6f37ecea => 0x
//@ run-call-fail: 0x531102fd => 0x
//@ run-call: 0xba83eae4 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0x3ba102e0 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0xe95a0beb => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000041000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0x43234fee => 0xf5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b
//@ run-call: 0xd2549072 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0x3af46c3b => 0x
//@ run-call: 0x83d6f579 => 0x0000000000000000000000000000000000000000000000000000000000000003
//@ run-call-fail: 0x450b33f5 => Panic(0x41)

// A calldata `bytes` or array whose offset and length assembly set can lie
// past the end of the calldata. As with solc's IR pipeline, copying it to
// memory or hashing it with `keccak256` checks the range and reverts, while
// encoding it, packing or concatenating it, hashing it with `sha256`, logging
// it, passing it to an external call, and decoding it read what lies there,
// past the end as zeros. A length whose bytes do not fit in a word fails with
// `Panic(0x41)`, where solc runs out of gas (CODEGEN-009). Every other result
// is solc 0.8.37's with --via-ir.
contract AssemblyCalldataPointerUnchecked {
    event Logged(bytes data);

    function b() internal pure returns (bytes calldata r) {
        assembly {
            r.offset := 4
            r.length := 64
        }
    }

    function words(uint256 n) internal pure returns (uint256[] calldata r) {
        assembly {
            r.offset := 4
            r.length := n
        }
    }

    function sink(bytes calldata data) external pure returns (uint256) {
        return data.length;
    }

    function hashIt() external pure returns (bytes32) {
        return keccak256(b());
    }

    function copyIt() external pure returns (bytes memory) {
        return b();
    }

    function encodeIt() external pure returns (bytes memory) {
        return abi.encode(b());
    }

    function packIt() external pure returns (bytes memory) {
        return abi.encodePacked(b(), words(2));
    }

    function concatIt() external pure returns (bytes memory) {
        return bytes.concat(b(), hex"01");
    }

    function shaIt() external pure returns (bytes32) {
        return sha256(b());
    }

    function decodeIt() external pure returns (uint256) {
        return abi.decode(b(), (uint256));
    }

    function eventIt() external {
        emit Logged(b());
    }

    function callIt() external view returns (uint256) {
        return this.sink(b()[1:4]);
    }

    function hugeWords() external pure returns (uint256) {
        return abi.encode(words(2 ** 251 + 1)).length;
    }
}
