//@ codegen-matrix: standard
//@ run-call: fullWord() => 4660
//@ run-call: joinedWords() => 0x11223344ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: overlappingWords() => 18
//@ run-call-fail: partialByte() => 0x1200000000000000000000000000000000000000000000000000000000000080
//@ run-call-fail: partialPair() => 0x1234000000000000000000000000000000000000000000000000000000000080
//@ run-call: beforeCall 3 => 4660

// Separate contracts prevent the partial-read control from forcing initialization for the
// fully-defined-word cases. All terminal reads return exactly one ABI word.
contract DefinedTerminalWord {
    function fullWord() external pure returns (uint256) {
        assembly {
            mstore(64, 0x1234)
            return(64, 32)
        }
    }

    function joinedWords() external pure returns (uint256) {
        assembly {
            mstore(36, 0x11223344)
            mstore(68, not(0))
            return(64, 32)
        }
    }

    function overlappingWords() external pure returns (uint256) {
        assembly {
            mstore(63, 0)
            mstore(65, 0x1234)
            return(64, 32)
        }
    }
}

contract PartialTerminalWord {
    function partialByte() external pure {
        assembly {
            mstore8(64, 0x12)
            revert(64, 32)
        }
    }

    function partialPair() external pure {
        assembly {
            mstore8(64, 0x12)
            mstore8(65, 0x34)
            revert(64, 32)
        }
    }
}

contract DefinedWordAcrossCall {
    uint256 state;

    function beforeCall(uint256 n) external returns (uint256) {
        assembly { mstore(64, 0x1234) }
        marker(n);
        assembly { return(64, 32) }
    }

    function marker(uint256 n) internal {
        for (uint256 i; i < n; ++i) {
            state += i;
        }
    }
}
