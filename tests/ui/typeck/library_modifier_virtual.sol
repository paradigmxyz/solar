// ported-from: test/libsolidity/syntaxTests/modifiers/definition_in_library.sol

library L {
    modifier mv virtual { _; } //~ ERROR: modifiers in a library cannot be `virtual`
}

// The library function error has its own code and wording.
library L2 {
    modifier m() virtual { _; } //~ ERROR: modifiers in a library cannot be `virtual`
    function f() internal virtual returns (uint256) { return 1; }
    //~^ ERROR: library functions cannot be `virtual`
}

// A non-`virtual` modifier in a library is fine, and so is a `virtual` modifier anywhere else.
library L3 {
    modifier m() { _; }
    function f() internal m returns (uint256) { return 1; }
}

contract C {
    modifier m() virtual { _; }
}
