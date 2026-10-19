contract C {
    function f(uint a, ufixed8x80 b) public pure {} //~ ERROR: fixed-point types are not supported
}
