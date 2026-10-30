//@ codegen-matrix: standard paris
//@[paris] compile-flags: -Ogas --evm-version paris
//@ run-call: dirtyTail => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call: emptyBytes => 0x
//@ run-call: emptyString => ""
//@ run-call: freshWords => [7, 8, 9]
//@ run-call: emptyWords => []
//@ run-call: builtString 0 => ""
//@ run-call: builtString 1 => "A"
//@ run-call: builtString 33 => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefg"
//@ run-call: builtString 40 => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn"
//@ run-call: builtString 64 => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz012345678901"
//@ run-call: builtWords 0 => []
//@ run-call: builtWords 1 => [17]
//@ run-call: builtWords 3 => [17, 34, 51]
//@ run-call: zeroBytes => 0x
//@ run-call: zeroString => ""
//@ run-call: zeroWords => []
//@ run-call: lowBytes 0 => 0x68656c6c6f
//@ run-call: lowBytes 32 => 0x
//@ run-call: lowBytes 256 => 0x
//@ run-call: lowWords 0 => [2748]
//@ run-call: lowWords 32 => []
//@ run-call: lowWords 256 => []

// A terminal external return of a memory byte string or word array writes the
// tuple offset before the object and returns it in place, without copying the
// payload. Fresh objects take that form directly; others, such as an object
// assembly builds or places, test their pointer first and copy when it lies
// below one word. The bytes after a byte string's data must read as zero in
// every case, including stale ones after a length that assembly shortened.
contract ZeroCopyReturns {
    function dirtyTail() external pure returns (bytes memory b) {
        b = new bytes(33);
        for (uint256 i; i < 33; ++i) {
            b[i] = bytes1(uint8(i + 1));
        }
        // The last data word holds the final byte, then dirty padding.
        assembly ("memory-safe") {
            mstore(add(b, 64), or(shl(248, 0x21), shr(8, not(0))))
        }
    }

    function emptyBytes() external pure returns (bytes memory) {
        return new bytes(0);
    }

    function emptyString() external pure returns (string memory) {
        return string(new bytes(0));
    }

    function freshWords() external pure returns (uint256[] memory values) {
        values = new uint256[](3);
        values[0] = 7;
        values[1] = 8;
        values[2] = 9;
    }

    function emptyWords() external pure returns (uint256[] memory) {
        return new uint256[](0);
    }

    function builtString(uint256 length) external pure returns (string memory) {
        require(length <= 64);
        return buildString(length);
    }

    function buildString(uint256 length) internal pure returns (string memory s) {
        assembly ("memory-safe") {
            s := mload(0x40)
            mstore(0x40, add(s, 0x80))
            // The length can end inside a data word that holds more characters.
            mstore(s, length)
            mstore(add(s, 0x20), "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef")
            mstore(add(s, 0x40), "ghijklmnopqrstuvwxyz012345678901")
            mstore(add(s, 0x60), not(0))
        }
    }

    function builtWords(uint256 length) external pure returns (uint256[] memory) {
        require(length <= 3);
        return buildWords(length);
    }

    function buildWords(uint256 length) internal pure returns (uint256[] memory values) {
        assembly ("memory-safe") {
            values := mload(0x40)
            mstore(0x40, add(values, 0x80))
            mstore(values, length)
            mstore(add(values, 0x20), 0x11)
            mstore(add(values, 0x40), 0x22)
            mstore(add(values, 0x60), 0x33)
        }
    }

    // The default objects are the zero slot, the word before which is the free
    // memory pointer.
    function zeroBytes() external pure returns (bytes memory b) {
        return b;
    }

    function zeroString() external pure returns (string memory s) {
        return s;
    }

    function zeroWords() external pure returns (uint256[] memory values) {
        return values;
    }

    // Assembly may place an object anywhere. Below one word, the word before it
    // wraps, so the return must copy; from one word up, the offset goes into
    // scratch space, and an object far above the heap is returned in place.
    function lowBytes(uint256 at) external pure returns (bytes memory b) {
        assembly ("memory-safe") {
            b := at
            switch lt(at, 32)
            case 1 {
                mstore(at, 5)
                mstore(add(at, 32), shl(216, 0x68656c6c6f))
            }
            default { mstore(at, 0) }
        }
    }

    function lowWords(uint256 at) external pure returns (uint256[] memory values) {
        assembly ("memory-safe") {
            values := at
            switch lt(at, 32)
            case 1 {
                mstore(at, 1)
                mstore(add(at, 32), 0xabc)
            }
            default { mstore(at, 0) }
        }
    }
}
