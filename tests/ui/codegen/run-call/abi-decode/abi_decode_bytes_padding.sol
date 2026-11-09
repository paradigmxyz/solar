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
    // A decoded bytes object owns its rounded allocation, including zero padding.
    // The incoming encoding need not have clean padding, and free memory need not be zero.
    // CHECK-LABEL: fn @padded
    // CHECK: abi_decode [bytes]
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
        bool clean;
        assembly {
            // Check that the returned object's padding is reserved as well as cleared.
            mstore(mload(0x40), not(0))
            let last := mload(add(add(decoded, 0x20), and(n, not(31))))
            clean := or(iszero(and(n, 31)), iszero(shl(mul(and(n, 31), 8), last)))
        }
        return clean && keccak256(decoded) == keccak256(value);
    }
}
