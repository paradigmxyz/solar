// ported-from: test/libsolidity/syntaxTests/modifiers/definition_in_library.sol

library L {
    modifier m { _; }
}

// A library may use its modifiers, but its functions cannot be `virtual`.
library L2 {
    modifier m() { _; }
    function f() internal m returns (uint256) { return 1; }
    function g() internal virtual returns (uint256) { return 1; }
    //~^ ERROR: library functions cannot be `virtual`
}
