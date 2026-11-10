//@ codegen-matrix: standard
//@ run-call-fail: f() => 0x

// solc reverts with `Panic(0x41)`; see CODEGEN-014 in docs/SOLC_DIVERGENCE.md.
contract C {
    function f() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := shl(64, 1)
        }
        return b;
    }
}
