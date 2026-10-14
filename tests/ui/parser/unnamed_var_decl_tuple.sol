contract C {
    function f() public {
        (uint a, uint) = (1, 2); //~ ERROR: expected identifier, found `)`
    }
}
