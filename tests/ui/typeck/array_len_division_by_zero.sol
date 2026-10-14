// Constant evaluation of an array length reports a division by zero, and type checking does not
// report it again. Like solc, evaluation also reaches the branch a conditional does not take.
contract C {
    uint constant Z = 0;
    uint constant X = 1;

    function f(uint[X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function g(uint[true ? 1 : X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
}
