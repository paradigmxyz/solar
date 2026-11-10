//@ codegen-matrix: standard
//@ run-call: padding 0x346d4ccbe8d52269524e442c43b00b08584f8bd9 => true
//@ run-call: fromCalldata 0x5af43d3d93803e602a57fd5bf3 => true
//@ run-call: wordAligned 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => true

// The bytes after a packed encoding's end are not cleared, so assembly can read
// whatever the memory held there. Solidity reads only the length, so every
// consumer below sees the same result as with an encoding of clean memory.
// Each case first fills the free memory with ones.

contract EncodePackedPadding {
    bytes private constant HEAD = hex"602c3d8160093d39f33d3d3d3d363d3d37363d73";
    bytes private constant TAIL = hex"5af43d3d93803e602a57fd5bf3";

    function padding(address implementation) external pure returns (bool) {
        _dirty();
        return _matches(
            abi.encodePacked(HEAD, implementation, TAIL),
            bytes.concat(HEAD, bytes20(implementation), TAIL)
        );
    }

    function fromCalldata(bytes calldata data) external pure returns (bool) {
        _dirty();
        return _matches(abi.encodePacked(uint8(1), data), bytes.concat(bytes1(0x01), data));
    }

    function wordAligned(bytes calldata data) external pure returns (bool) {
        _dirty();
        return _matches(abi.encodePacked(data, uint8(7)), bytes.concat(data, bytes1(0x07)));
    }

    function _dirty() private pure {
        assembly ("memory-safe") {
            let m := mload(0x40)
            for { let i := 0 } lt(i, 0x400) { i := add(i, 0x20) } { mstore(add(m, i), not(0)) }
        }
    }

    function _matches(bytes memory b, bytes memory expected) private pure returns (bool) {
        return b.length == expected.length && keccak256(b) == keccak256(expected)
            && bytes4(b) == bytes4(expected) && bytes32(b) == bytes32(expected)
            && keccak256(abi.encode(b)) == keccak256(abi.encode(expected))
            && keccak256(bytes.concat(b, hex"01")) == keccak256(bytes.concat(expected, hex"01"));
    }
}
