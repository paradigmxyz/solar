//@ codegen-matrix: standard parisgas parissize
//@[parisgas] compile-flags: -Ogas --evm-version paris --emit=abi,bin
//@[parissize] compile-flags: -Osize --evm-version paris --emit=abi,bin
//@[mir] filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: padded 0, false => true
//@ run-call: padded 1, false => true
//@ run-call: padded 2, false => true
//@ run-call: padded 31, false => true
//@ run-call: padded 32, false => true
//@ run-call: padded 33, false => true
//@ run-call: padded 63, false => true
//@ run-call: padded 64, false => true
//@ run-call: padded 65, false => true
//@ run-call: padded 0, true => true
//@ run-call: padded 1, true => true
//@ run-call: padded 31, true => true
//@ run-call: padded 32, true => true
//@ run-call: padded 33, true => true
//@ run-call: padded 65, true => true
//@ run-call: paddedCalldata 0, false => true
//@ run-call: paddedCalldata 1, false => true
//@ run-call: paddedCalldata 31, false => true
//@ run-call: paddedCalldata 32, false => true
//@ run-call: paddedCalldata 33, false => true
//@ run-call: paddedCalldata 65, false => true
//@ run-call: paddedCalldata 1, true => true
//@ run-call: paddedCalldata 31, true => true
//@ run-call: paddedCalldata 32, true => true
//@ run-call: paddedCalldata 33, true => true
//@ run-call: paddedCalldata 65, true => true

contract DecodeBytesPadding {
    // CHECK-LABEL: fn @padded
    // CHECK: abi_decode [bytes]
    function padded(uint256 n, bool misaligned) public pure returns (bool) {
        bytes memory value = input(n);
        bytes memory encoded = dirtyEncoding(value);
        dirtyFreeMemory(misaligned);
        bytes memory decoded = abi.decode(encoded, (bytes));
        return check(decoded, keccak256(value));
    }

    function paddedCalldata(uint256 n, bool misaligned) public view returns (bool) {
        bytes memory value = input(n);
        return this.fromCalldata(dirtyEncoding(value), keccak256(value), misaligned);
    }

    function fromCalldata(bytes calldata encoded, bytes32 expected, bool misaligned)
        external pure returns (bool)
    {
        dirtyFreeMemory(misaligned);
        bytes memory decoded = abi.decode(encoded, (bytes));
        return check(decoded, expected);
    }

    function input(uint256 n) private pure returns (bytes memory value) {
        require(n <= 65);
        value = new bytes(n);
        for (uint256 i; i < n; ++i) value[i] = bytes1(uint8(i + 1));
    }

    function dirtyEncoding(bytes memory value) private pure returns (bytes memory encoded) {
        encoded = abi.encode(value);
        assembly {
            // The ABI payload is unchanged; only its padding and subsequent memory are dirty.
            mstore(add(add(encoded, 0x60), mload(value)), not(0))
        }
    }

    function dirtyFreeMemory(bool misaligned) private pure {
        assembly {
            let free := add(mload(0x40), mul(misaligned, 3))
            mstore(0x40, free)
            for { let i := 0 } lt(i, 0x200) { i := add(i, 0x20) } {
                mstore(add(free, i), not(0))
            }
        }
    }

    function check(bytes memory decoded, bytes32 expected) private pure returns (bool) {
        bool clean;
        assembly {
            // Padding must be reserved as well as cleared, including at a misaligned base.
            mstore(mload(0x40), not(0))
            let n := mload(decoded)
            let last := mload(add(add(decoded, 0x20), and(n, not(31))))
            clean := or(iszero(and(n, 31)), iszero(shl(mul(and(n, 31), 8), last)))
        }
        bytes memory next = new bytes(33);
        next[0] = 0x7a;
        next[32] = 0x7b;
        return clean && keccak256(decoded) == expected
            && next.length == 33 && next[0] == 0x7a && next[32] == 0x7b;
    }
}
