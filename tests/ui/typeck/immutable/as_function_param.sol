contract C {
    function f(uint immutable) public pure {} //~ ERROR: mutability is not allowed here
}
