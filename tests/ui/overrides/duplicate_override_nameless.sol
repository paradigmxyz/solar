// A duplicate base in the override list of a function without a name does not panic.

contract A {
    fallback() external virtual {}
}

contract B is A {
    fallback() external override(A, A) {} //~ ERROR: duplicate contract found in override list of `fallback`
}
