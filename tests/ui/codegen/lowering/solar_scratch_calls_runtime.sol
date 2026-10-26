//@ codegen-matrix: standard
//@ run-call: olderObject 101 => 101
//@ run-call: readBeforeCall 101 => 101
//@ run-call: readAfterCall 101 => 103
//@ run-call: repeatedCalls 101 => true
//@ run-call: harmlessStore 101 => 7
//@ run-call: earlyReturn 101 => 101
//@ run-call: stashAfterBlock 101 => 103
//@ run-call: restoredPointer 101 => true
//@ run-call: refreshedInLoop 101 => 203

contract Test {
    /// @custom:solar-noinline
    function temporary(uint256 x) internal pure returns (bytes32 digest) {
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            digest = keccak256(encoded);
        }
    }

    function olderObject(uint256 x) external pure returns (uint256) {
        bytes memory kept = abi.encode(x);
        require(temporary(x) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        return abi.decode(kept, (uint256));
    }

    function readBeforeCall(uint256 x) external pure returns (uint256 result) {
        assembly {
            let pointer := mload(0x40)
            mstore(pointer, x)
            result := mload(pointer)
            mstore(0x40, add(pointer, 0x20))
        }
        require(temporary(x) != bytes32(0));
    }

    function readAfterCall(uint256 x) external pure returns (uint256 result) {
        require(temporary(x) != bytes32(0));
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) }
    }

    function repeatedCalls(uint256 x) external pure returns (bool) {
        return temporary(x) == temporary(x);
    }

    /// @custom:solar-noinline
    function storeWord() internal pure {
        assembly { mstore(0, 7) }
    }

    function harmlessStore(uint256 x) external pure returns (uint256 result) {
        storeWord();
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        assembly { result := mload(0) }
    }

    /// @custom:solar-noinline
    function returnsEarly(uint256 x) internal pure returns (bytes32) {
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            return keccak256(encoded);
        }
    }

    function earlyReturn(uint256 x) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        require(returnsEarly(x) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) }
    }

    /// @custom:solar-noinline
    function stash() internal pure {
        assembly { mstore(0, mload(0x40)) }
    }

    function stashAfterBlock(uint256 x) external pure returns (uint256 result) {
        require(temporary(x) != bytes32(0));
        stash();
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(mload(0), 0x20)) }
    }

    /// @custom:solar-noinline
    function restorePointer() internal pure {
        assembly { mstore(add(0x20, 0x20), mload(0x40)) }
    }

    function restoredPointer(uint256 x) external pure returns (bool) {
        restorePointer();
        return temporary(x) != bytes32(0);
    }

    function refreshedInLoop(uint256 x) external pure returns (uint256 result) {
        for (uint256 i; i < 2; ++i) {
            uint256 word = x + i;
            uint256 loaded;
            assembly {
                let pointer := mload(0x40)
                mstore(pointer, word)
                loaded := mload(pointer)
            }
            require(temporary(word) != bytes32(0));
            result += loaded;
        }
    }
}
