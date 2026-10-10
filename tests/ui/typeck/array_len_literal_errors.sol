// Constant evaluation of an array length reports the operation it fails at, and type checking
// reports the failed operations it does not reach, each once.
contract C {
    uint constant Z = 0;
    uint constant X = 1;

    function f(uint[true ? 1 : 1 / 0] memory) public {} //~ ERROR: cannot apply builtin operator `/`
    function g(uint[true ? 1 : X / Z] memory) public {} //~ ERROR: division by zero
    function h(uint[(1 / 0) + 1] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function i(uint[X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero

    function j() public pure {
        uint[true ? 1 : 1 / 0] memory a; //~ ERROR: cannot apply builtin operator `/`
        uint[(1 / 0) + 1] memory b; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        a;
        b;
    }
}
