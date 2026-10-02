//@ codegen-matrix: standard
//@ run-call: check 0 => true
//@ run-call: check 1 => true
//@ run-call: check 31 => true
//@ run-call: check 32 => true
//@ run-call: check 33 => true
//@ run-call: check 63 => true
//@ run-call: check 64 => true
//@ run-call: check 65 => true
//@ run-call-fail: check 66

contract BytePadding {
    function check(uint256 length) external pure returns (bool) {
        require(length <= 65);
        bytes memory value = new bytes(length);
        for (uint256 i; i < length; ++i) value[i] = bytes1(uint8(i * 37 + 11));
        // Dirty the future encoder's header and padding without allocating it.
        assembly {
            let end := add(mload(0x40), 512)
            for { let p := mload(0x40) } lt(p, end) { p := add(p, 32) } {
                mstore(p, not(0))
            }
        }
        bytes memory encoded = abi.encode(value, value);
        uint256 padded = (length + 31) & ~uint256(31);
        require(encoded.length == 128 + padded * 2);
        uint256 first;
        uint256 second;
        assembly {
            first := mload(add(encoded, 32))
            second := mload(add(encoded, 64))
        }
        require(first == 64 && second == 96 + padded);
        for (uint256 tail; tail < 2; ++tail) {
            uint256 offset = tail == 0 ? first : second;
            uint256 storedLength;
            assembly { storedLength := mload(add(add(encoded, 32), offset)) }
            require(storedLength == length);
            for (uint256 i; i < padded; ++i) {
                require(encoded[offset + 32 + i] == (i < length ? value[i] : bytes1(0)));
            }
        }
        return true;
    }
}
