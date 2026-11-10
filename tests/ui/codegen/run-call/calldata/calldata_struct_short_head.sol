//@ codegen-matrix: standard
//@ run-call-fail: 0xd45754f8000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 0x

// solc returns `S(0, "")`; see CODEGEN-016 in docs/SOLC_DIVERGENCE.md.
contract C {
    struct S {
        uint256 value;
        bytes tail;
    }

    function f(bytes calldata input) external pure returns (S calldata s) {
        assembly {
            s := input.offset
        }
    }
}
