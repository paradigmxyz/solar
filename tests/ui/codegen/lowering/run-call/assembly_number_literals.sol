//@ codegen-matrix: standard
//@ run-call: f => 1036

// Assembly accepts integer literals with underscores or exponents; see PARSE-005 in
// docs/SOLC_DIVERGENCE.md.
contract AssemblyNumberLiterals {
    function f() public pure returns (uint256 r) {
        assembly {
            r := add(add(1_000, 0x1_0), 2e1)
        }
    }
}
