//@ codegen-matrix: standard
//@ compile-flags: --no-warnings
//@ run-call: plain(uint256) 0 => 0
//@ run-call: spelled(uint256) 0 => 0x1111

// The heap-floor pass recognizes a read of the free memory pointer slot only when lowering folds
// its address to `0x40`. A load at `exp(0x40, 1)` is a data read at a computed address, so the
// forged pointer stays unraised. solc returns 0x1111 for both functions (CODEGEN-026, CODEGEN-010).
contract C {
    function plain(uint256 n) external pure returns (uint256) {
        assembly {
            mstore(0x00, 0x1111)
            mstore(0x40, n)
            let p := mload(0x40)
            return(p, 0x20)
        }
    }

    function spelled(uint256 n) external pure returns (uint256) {
        assembly {
            mstore(0x00, 0x1111)
            mstore(0x40, n)
            let p := mload(exp(0x40, 1))
            return(p, 0x20)
        }
    }
}
