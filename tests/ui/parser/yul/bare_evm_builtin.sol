// Instructional-style bare EVM builtins (no parentheses) in inline assembly,
// e.g. `id := chainid`, are accepted by older Solidity and parse as
// zero-argument calls (`chainid()`).
contract C {
    function f() external view returns (uint id, address a, uint g) {
        assembly {
            id := chainid
            a := caller
            g := gas
        }
    }
}
