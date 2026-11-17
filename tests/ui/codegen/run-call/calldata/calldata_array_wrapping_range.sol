//@ codegen-matrix: standard
//@ run-call-fail: f 0

// An array slice that assembly gave a start and a length whose sum wraps past `2**256` passes the
// range check and runs out of gas in the copy, where solc panics with `Panic(0x41)` at the
// allocation. See CODEGEN-024 in docs/SOLC_DIVERGENCE.md.
contract C {
    function f(uint256) external pure returns (uint256[] memory r) {
        uint256[] calldata x;
        assembly {
            x.offset := add(shl(255, 1), 36)
            x.length := sub(shl(250, 1), 1)
        }
        r = x;
    }
}
