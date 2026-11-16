//@ codegen-matrix: standard
//@ run-call: bytesOf 0 => []
//@ run-call: bytesOf 3 => [1, 8, 15]
//@ run-call: signedOf 3 => [-3, 0, 3]
//@ run-call: checkBytes 0 => true
//@ run-call: checkBytes 1 => true
//@ run-call: checkBytes 31 => true
//@ run-call: checkBytes 32 => true
//@ run-call: checkBytes 33 => true
//@ run-call: checkBytes 65 => true
//@ run-call: checkSigned 17 => true
//@ run-call: checkWide 5 => true
//@ run-call: checkTriples 21 => true
//@ run-call: checkFlags 40 => true
//@ run-call: checkSides 33 => true
//@ run-call: checkAddresses 3 => true
//@ run-call: checkAfterPop 34 => true
//@ run-call-fail: dirtyEnum() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: checkFixed() => true

// A storage array of elements that share a word is copied to memory one
// storage word at a time, unpacking every element the word holds, the last
// word only up to the length. Each check copies the array and compares it
// with the same elements built in memory.
contract PackedArrayCopy {
    enum Side {
        Buy,
        Sell,
        Hold
    }

    uint8[] bytes_;
    int16[] signed_;
    uint128[] wide;
    bytes3[] triples;
    bool[] flags;
    Side[] sides;
    address[] addresses;
    uint8[40] fixedBytes;
    int24[7] fixedSigned;
    bytes4[9] fixedWords;

    function bytesOf(uint256 n) external returns (uint8[] memory) {
        for (uint256 i; i < n; ++i) bytes_.push(uint8(i * 7 + 1));
        return bytes_;
    }

    function signedOf(uint256 n) external returns (int16[] memory) {
        for (uint256 i; i < n; ++i) signed_.push(int16(int256(i) * 3 - 3));
        return signed_;
    }

    function checkBytes(uint256 n) external returns (bool) {
        uint8[] memory expected = new uint8[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = uint8(i * 7 + 1);
            bytes_.push(expected[i]);
        }
        return keccak256(abi.encode(bytes_)) == keccak256(abi.encode(expected));
    }

    function checkSigned(uint256 n) external returns (bool) {
        int16[] memory expected = new int16[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = int16(int256(i) * -2011);
            signed_.push(expected[i]);
        }
        return keccak256(abi.encode(signed_)) == keccak256(abi.encode(expected));
    }

    function checkWide(uint256 n) external returns (bool) {
        uint128[] memory expected = new uint128[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = type(uint128).max - uint128(i);
            wide.push(expected[i]);
        }
        return keccak256(abi.encode(wide)) == keccak256(abi.encode(expected));
    }

    function checkTriples(uint256 n) external returns (bool) {
        bytes3[] memory expected = new bytes3[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = bytes3(uint24(0xa0b0c0 + i));
            triples.push(expected[i]);
        }
        return keccak256(abi.encode(triples)) == keccak256(abi.encode(expected));
    }

    function checkFlags(uint256 n) external returns (bool) {
        bool[] memory expected = new bool[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = i % 3 == 1;
            flags.push(expected[i]);
        }
        return keccak256(abi.encode(flags)) == keccak256(abi.encode(expected));
    }

    function checkSides(uint256 n) external returns (bool) {
        Side[] memory expected = new Side[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = Side(i % 3);
            sides.push(expected[i]);
        }
        return keccak256(abi.encode(sides)) == keccak256(abi.encode(expected));
    }

    // One address fills its word, so each element keeps its own load.
    function checkAddresses(uint256 n) external returns (bool) {
        address[] memory expected = new address[](n);
        for (uint256 i; i < n; ++i) {
            expected[i] = address(uint160(0xbeef + i));
            addresses.push(expected[i]);
        }
        return keccak256(abi.encode(addresses)) == keccak256(abi.encode(expected));
    }

    // Popping zeroes the element, and the copy stops at the length.
    function checkAfterPop(uint256 n) external returns (bool) {
        for (uint256 i; i < n; ++i) bytes_.push(uint8(i + 1));
        bytes_.pop();
        bytes_.pop();
        uint8[] memory expected = new uint8[](n - 2);
        for (uint256 i; i < n - 2; ++i) expected[i] = uint8(i + 1);
        return keccak256(abi.encode(bytes_)) == keccak256(abi.encode(expected));
    }

    // Fixed-size arrays are copied the same way.
    function checkFixed() external returns (bool) {
        uint8[40] memory bytesExpected;
        int24[7] memory signedExpected;
        bytes4[9] memory wordsExpected;
        for (uint256 i; i < 40; ++i) {
            bytesExpected[i] = uint8(255 - i);
            fixedBytes[i] = bytesExpected[i];
        }
        for (uint256 i; i < 7; ++i) {
            signedExpected[i] = int24(int256(i) * -1_000_003);
            fixedSigned[i] = signedExpected[i];
        }
        for (uint256 i; i < 9; ++i) {
            wordsExpected[i] = bytes4(uint32(0xdead0000 + i));
            fixedWords[i] = wordsExpected[i];
        }
        uint8[40] memory bytesCopy = fixedBytes;
        int24[7] memory signedCopy = fixedSigned;
        bytes4[9] memory wordsCopy = fixedWords;
        return keccak256(abi.encode(bytesCopy)) == keccak256(abi.encode(bytesExpected))
            && keccak256(abi.encode(signedCopy)) == keccak256(abi.encode(signedExpected))
            && keccak256(abi.encode(wordsCopy)) == keccak256(abi.encode(wordsExpected));
    }

    // An enum word that inline assembly put out of range fails the copy's range check.
    function dirtyEnum() external returns (bytes32) {
        sides.push(Side.Sell);
        sides.push(Side.Hold);
        uint256 slot;
        assembly {
            slot := sides.slot
        }
        bytes32 data = keccak256(abi.encode(slot));
        assembly {
            sstore(data, or(sload(data), shl(8, 7)))
        }
        return keccak256(abi.encode(sides));
    }
}
