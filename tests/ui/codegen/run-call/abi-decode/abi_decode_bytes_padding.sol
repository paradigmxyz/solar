//@ codegen-matrix: standard
//@ filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: padded 0 => true
//@ run-call: padded 1 => true
//@ run-call: padded 2 => true
//@ run-call: padded 31 => true
//@ run-call: padded 32 => true
//@ run-call: padded 33 => true
//@ run-call: padded 63 => true
//@ run-call: padded 64 => true
//@ run-call: padded 65 => true

contract DecodeBytesPadding {
    // A decoded bytes object does not clear its padding, so assembly can read the dirty bytes of
    // the encoding or of free memory there. Solidity reads only the length, so every consumer
    // below sees the same result as with clean padding.
    // CHECK-LABEL: fn @padded
    // CHECK: abi_decode [bytes]
    // CHECK-NOT: memory_zero
    function padded(uint256 n) public pure returns (bool) {
        require(n <= 65);
        bytes memory value = new bytes(n);
        for (uint256 i; i < n; ++i) value[i] = bytes1(uint8(i + 1));
        bytes memory encoded = abi.encode(value);
        assembly {
            // Dirty the encoded padding without changing the declared payload.
            mstore(add(add(encoded, 0x60), n), not(0))
            let free := mload(0x40)
            for { let i := 0 } lt(i, 0x200) { i := add(i, 0x20) } {
                mstore(add(free, i), not(0))
            }
        }
        bytes memory decoded = abi.decode(encoded, (bytes));
        return decoded.length == n && keccak256(decoded) == keccak256(value)
            && bytes4(decoded) == bytes4(value) && bytes32(decoded) == bytes32(value)
            && keccak256(abi.encode(decoded)) == keccak256(abi.encode(value))
            && keccak256(bytes.concat(decoded, hex"01")) == keccak256(bytes.concat(value, hex"01"));
    }
}
