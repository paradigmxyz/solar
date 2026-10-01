//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard shared
//@[shared] compile-flags: -Osize -Zdump=mir
//@[shared] filecheck: --check-prefix=SHARED
//@ run-call: words => true
//@ run-call: bytesTail => true
//@ run-call: concat => true
//@ run-call: assembled => true
//@ run-call: fresh => [1, 2, 3]
//@ run-call: freshEmpty => []

// Without `MCOPY`, memory copies lower to word loops: whole-word lengths skip
// the partial-word merge, and a partial tail changes exactly the copied bytes.
// Returning a fresh array encodes it in place, a backward whole-word copy.
// In size mode, the forward byte copies of ABI encoding share one helper, which
// needs no runtime direction check, even for an object that assembly allocates.
contract PreCancunMemoryCopies {
    function words() external pure returns (bool) {
        uint256[] memory values = new uint256[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
        return keccak256(abi.encode(values))
            == keccak256(abi.encodePacked(uint256(32), uint256(3), uint256(1), uint256(2), uint256(3)));
    }

    // SHARED-LABEL: fn @bytesTail(
    // SHARED: icall @[[FORWARD:mcopy_words[.0-9]*]],
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

    // SHARED-LABEL: fn @concat(
    // SHARED: icall @[[FORWARD]],
    function concat() external pure returns (bool) {
        bytes memory first = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        bytes memory second = hex"2223";
        bytes memory joined = bytes.concat(first, second);
        return joined.length == 35
            && keccak256(joined)
                == keccak256(hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223");
    }

    // SHARED-LABEL: fn @assembled(
    // SHARED: icall @[[FORWARD]],
    function assembled() external pure returns (bool) {
        bytes memory payload;
        bytes32 expected;
        assembly ("memory-safe") {
            payload := mload(0x40)
            mstore(payload, 70)
            mstore(add(payload, 32), 0x0101010101010101010101010101010101010101010101010101010101010101)
            mstore(add(payload, 64), 0x0202020202020202020202020202020202020202020202020202020202020202)
            mstore(add(payload, 96), 0x0303030303030303030303030303030303030303030303030303030303030303)
            mstore(0x40, add(payload, 128))
            expected := keccak256(add(payload, 32), 70)
        }
        bytes memory encoded = abi.encode(payload, uint256(7));
        bytes32 actual;
        assembly {
            actual := keccak256(add(encoded, 128), 70)
        }
        return actual == expected;
    }

    function fresh() external pure returns (uint256[] memory values) {
        values = new uint256[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
    }

    function freshEmpty() external pure returns (uint256[] memory) {
        return new uint256[](0);
    }

    // SHARED: {{^}}fn @[[FORWARD]](
    // SHARED-NOT: lt arg1, arg0
    // SHARED: ret
}
