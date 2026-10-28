//@ codegen-matrix: standard
//@ run-call: decodedAddress 0x0000000000000000000000000000000000000000000000000000000000000000 => 0x1234
//@ run-call: bumpedBeforeEncode => 0x1131
//@ run-call: copiedFreePointer 0 => true

// Reads through words that only look like heap pointers keep the low memory they reach, and an
// encoding keeps the free memory pointer it allocates at.
contract UntrustedBases {
    // A decoded `uint256` is no pointer, and may address the scratch words.
    function decodedAddress(bytes memory data) external pure returns (uint256) {
        uint256 x = abi.decode(data, (uint256));
        assembly {
            mstore(0, 0x1234)
            let y := mload(x)
            mstore(0, 0)
            mstore(0x80, y)
            return(0x80, 0x20)
        }
    }

    // An encoding allocates at the free memory pointer that assembly bumped over its buffer.
    function bumpedBeforeEncode() external pure returns (uint256) {
        uint256 buf;
        assembly {
            buf := mload(0x40)
            mstore(buf, 0x1111)
            mstore(0x40, add(buf, 0x20))
        }
        bytes memory e = abi.encode(uint256(7));
        assembly {
            mstore(0, add(mload(buf), mload(e)))
            return(0, 0x20)
        }
    }

    // A free memory pointer that assembly copies in below the heap reads what lies there.
    function copiedFreePointer(uint256) external pure returns (bool) {
        bytes32 r;
        assembly {
            mstore(0, 0xabc)
            calldatacopy(0x40, 4, 0x20)
            // Keep the slot's word as data across heap-floor pointer clamping.
            mstore(0x20, mload(0x40))
            r := keccak256(mload(0x20), 0x20)
            mstore(0, 0)
            mstore(0x40, 0x80)
        }
        return r == keccak256(abi.encode(uint256(0xabc)));
    }
}
