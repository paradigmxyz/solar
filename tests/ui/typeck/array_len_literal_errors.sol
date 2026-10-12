// Constant evaluation of an array length reports the operation it fails at, and type checking
// does not report it again.
contract C {
    uint constant Z = 0;
    uint constant X = 1;

    function f(uint[(1 / 0) + 1] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function g(uint[X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero

    function h() public pure {
        uint[(1 / 0) + 1] memory a; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        a;
    }
}
