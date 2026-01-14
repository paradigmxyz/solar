// Mutability not allowed in local variables
contract C {
    function f() public pure {
        uint constant x;  //~ ERROR: mutability is not allowed here
        uint immutable y; //~ ERROR: mutability is not allowed here
    }
}
