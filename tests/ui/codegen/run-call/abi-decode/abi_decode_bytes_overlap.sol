//@ codegen-matrix: standard
//@ run-call: rewound => true

contract DecodeBytesOverlap {
    // Assembly can move the free memory pointer back over the encoding, so the decoded object
    // overlaps its source. The padding clear must not run before the payload is copied.
    function rewound() external pure returns (bool) {
        bytes memory payload = new bytes(96);
        for (uint256 i; i < 96; ++i) payload[i] = bytes1(uint8(i + 1));
        bytes memory encoded = abi.encode(payload);
        assembly {
            mstore(0x40, encoded)
        }
        return keccak256(abi.decode(encoded, (bytes))) == keccak256(payload);
    }
}
