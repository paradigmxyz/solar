// solc reports 3558 at the assembly read instead; see TYPECK-010 in docs/SOLC_DIVERGENCE.md.
contract C {
    bytes32 constant x = x; //~ ERROR: the value of the constant `x` has a cyclic dependency via `x`
    function f() public pure returns (uint t) {
        assembly {
            // Reference to a circular member
            t := x
        }
    }
}
