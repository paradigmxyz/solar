//@ codegen-matrix: standard
//@ run-call: hashTracksWrites => true

contract KeccakBytesLoopMutation {
    function hashTracksWrites() external pure returns (bool) {
        bytes memory data = new bytes(1);
        bytes32 hash;
        for (uint256 i; i < 2; ++i) {
            data[0] = bytes1(uint8(i));
            hash = keccak256(data);
        }
        return hash == keccak256(hex"01");
    }
}
