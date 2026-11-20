//@ codegen-matrix: standard
//@ run-call-fail: f() => 0x
//@ run-call-fail: nearLimit() => 0x

// solc reverts with `Panic(0x41)`, also for lengths just below `2**64`; see CODEGEN-014 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    function f() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := shl(64, 1)
        }
        return b;
    }

    function nearLimit() external pure returns (bytes memory) {
        bytes calldata b;
        assembly {
            b.offset := 4
            b.length := sub(shl(64, 1), 1)
        }
        return b;
    }
}
