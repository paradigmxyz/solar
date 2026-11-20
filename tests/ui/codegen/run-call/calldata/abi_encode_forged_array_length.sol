//@ codegen-matrix: standard
//@ run-call-fail: 0xdc0aa2af00000000000000000000000000000000000000000000000000000000000000040400000000000000000000000000000000000000000000000000000000000000 => Panic(0x41)

// Assembly sets the length of a calldata array of two-word structs to `2**250`. `abi.encode`
// reverts with `Panic(0x41)` when it allocates the buffer, where solc reverts with empty data
// (CODEGEN-028).
contract AbiEncodeForgedArrayLength {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    function encode(uint256 offset, uint256 length) external pure returns (bytes memory) {
        Pair[] calldata p;
        assembly {
            p.offset := offset
            p.length := length
        }
        return abi.encode(p);
    }
}
