//@ compile-flags: -Ztypeck

// A literal shifted by a non-constant amount is a runtime value, not a
// compile-time constant; it is typed `uint256` (matching solc), so e.g.
// `bytes32(1 << role)` type-checks (the role-bitmask idiom).
contract C {
    function f(uint8 role) public pure returns (bytes32 a, bytes32 b, uint256 c) {
        a = bytes32(1 << role);
        b = ~bytes32(1 << role);
        c = 1 << role;
    }
}
