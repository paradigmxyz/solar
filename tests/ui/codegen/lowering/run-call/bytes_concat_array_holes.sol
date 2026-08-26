//@ codegen-matrix: standard
//@ run-call: BytesConcatArrayHoles::join() => 0x554bfe425930cf6cbbc9d3c69c4728b876b7e7817f343f397841b1628f69d10f, 0x0000000000000000000000000000000000000000000000000000000000000007

contract BytesConcatArrayHoles {
    function chunksAndHash() internal pure returns (bytes[] memory chunks, bytes32 hash) {
        chunks = new bytes[](20);
        chunks[0] = bytes("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN");
        chunks[15] = bytes("tail");
        hash = bytes32(uint256(7));
    }

    function join() external pure returns (bytes32 result, bytes32 hash) {
        bytes[] memory chunks;
        (chunks, hash) = chunksAndHash();
        bytes memory joined;
        for (uint256 i; i < chunks.length; ++i) {
            joined = bytes.concat(joined, chunks[i]);
        }
        result = keccak256(joined);
    }
}
