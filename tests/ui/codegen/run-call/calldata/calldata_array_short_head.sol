//@ codegen-matrix: standard
//@ run-call-fail: 0xd45754f8000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007 => 0x

// solc returns `[7, 0]`; see CODEGEN-016 in docs/SOLC_DIVERGENCE.md.
contract C {
    function f(bytes calldata input) external pure returns (uint256[2] calldata s) {
        assembly {
            s := input.offset
        }
    }
}
