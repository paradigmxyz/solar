// Constant evaluation of an array length reports the operation it fails at, and type checking
// does not report it again. Like solc, evaluation also reaches the branch a conditional does not
// take.
contract C {
    function f(uint[(1 / 0) + 1] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function g(uint[true ? 1 : 1 / 0] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero

    function h() public pure {
        uint[(1 / 0) + 1] memory a; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        uint[true ? 1 : 1 / 0] memory b; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        a;
        b;
    }
}
