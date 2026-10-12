// Constant evaluation of an array length reports the operation it fails at, and type checking
// does not report it again. Type checking still reports failed operations that evaluation does
// not reach.
contract C {
    uint constant Z = 0;
    uint constant X = 1;

    function f(uint[(1 / 0) + 1] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function g(uint[X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function h(uint[true ? 1 : 1 / 0] memory) public {} //~ ERROR: cannot apply builtin operator `/`
    function i(uint[true ? 1 : X / Z] memory) public {} //~ ERROR: division by zero

    function j() public pure {
        uint[(1 / 0) + 1] memory a; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        uint[true ? 1 : 1 / 0] memory b; //~ ERROR: cannot apply builtin operator `/`
        a;
        b;
    }
}
