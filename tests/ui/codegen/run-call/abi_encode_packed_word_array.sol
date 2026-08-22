//@ run-call: pack() => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0, 64

contract AbiEncodePackedWordArray {
    function pack() external pure returns (bytes32 digest, uint256 len) {
        bytes32[] memory values = new bytes32[](2);
        values[0] = bytes32(uint256(1));
        values[1] = bytes32(uint256(2));
        bytes memory encoded = abi.encodePacked(values);
        return (keccak256(encoded), encoded.length);
    }
}
