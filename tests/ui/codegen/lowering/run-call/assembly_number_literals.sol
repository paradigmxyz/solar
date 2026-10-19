//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: f => 1036

// Assembly accepts integer literals with underscores or exponents; see PARSE-005 in
// docs/SOLC_DIVERGENCE.md.
contract AssemblyNumberLiterals {
    // CHECK-LABEL: fn @f()
    // CHECK: [[V:v[0-9]+]] = add 0x3e8, 16
    // CHECK: add [[V]], 20
    function f() public pure returns (uint256 r) {
        assembly {
            r := add(add(1_000, 0x1_0), 2e1)
        }
    }
}
