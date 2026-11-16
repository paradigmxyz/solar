//@ check-pass

// solc rejects underscores in Yul number literals; we accept them. See PARSE-005 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    function f() public pure returns (uint256 x) {
        assembly {
            x := 1__0
            x := 1_
            x := 0x1__0
            x := 1_0
        }
    }
}
