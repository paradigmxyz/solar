//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard
//@ run-call: words => true
//@ run-call: bytesTail => true
//@ run-call: concat => true

// Without `MCOPY`, memory copies lower to word loops: whole-word lengths skip
// the partial-word merge, and a partial tail changes exactly the copied bytes.
contract PreCancunMemoryCopies {
    function words() external pure returns (bool) {
        uint256[] memory values = new uint256[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
        return keccak256(abi.encode(values))
            == keccak256(abi.encodePacked(uint256(32), uint256(3), uint256(1), uint256(2), uint256(3)));
    }

    function bytesTail() external pure returns (bool) {
        bytes memory payload = new bytes(33);
        for (uint256 i; i < 33; ++i) {
            payload[i] = bytes1(uint8(i + 1));
        }
        bytes memory encoded = abi.encode(payload);
        uint256 last;
        assembly {
            last := mload(add(encoded, 128))
        }
        return encoded.length == 128 && encoded[64] == 0x01 && last == uint256(33) << 248;
    }

    function concat() external pure returns (bool) {
        bytes memory first = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        bytes memory second = hex"2223";
        bytes memory joined = bytes.concat(first, second);
        return joined.length == 35
            && keccak256(joined)
                == keccak256(hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223");
    }
}
