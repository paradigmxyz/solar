//@ compile-flags: -Ztypeck

contract C {
    function f() public pure {
        uint(1, 2); //~ ERROR: expected exactly one unnamed argument
        uint(); //~ ERROR: expected exactly one unnamed argument
        bytes32(); //~ ERROR: expected exactly one unnamed argument
        address(1, 2); //~ ERROR: expected exactly one unnamed argument
    }
}
