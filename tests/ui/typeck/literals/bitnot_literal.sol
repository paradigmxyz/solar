// `~` on an integer literal yields `-x - 1`, which flips the literal's sign.
contract C {
    function f(uint256 x) public pure {
        uint8 a = ~(~0 << 8);
        int8 b = ~0;
        uint256 c = x & ~(~0 << 160);
        uint256 d = ~~1;
        uint8 e = ~0; //~ ERROR: mismatched types
        uint256 g = x & ~0; //~ ERROR: cannot apply builtin operator `&` to `uint256` and `int_literal[1]`
        a; b; c; d; e; g;
    }
}
