// ported-from: test/libsolidity/syntaxTests/inheritance/virtual/library_err.sol

library L {
    function f() internal pure virtual returns (uint) { return 0; } //~ ERROR: library functions cannot be `virtual`
    function g() public virtual {} //~ ERROR: library functions cannot be `virtual`
    function h() public {}

    // `virtual` with `private` is reported on its own.
    function i() private virtual {} //~ ERROR: `virtual` and `private` cannot be used together
}

// The restriction only applies to libraries.
contract C {
    function f() public virtual {}
}
