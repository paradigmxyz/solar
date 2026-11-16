// Constant evaluation of an array length reports a division by zero, and type checking does not
// report it again. solc's evaluator gives a typed division by zero no value, so in a conditional,
// which it does not evaluate, only the conditional is reported.
contract C {
    uint constant Z = 0;
    uint constant X = 1;

    function f(uint[X / Z] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function g(uint[true ? 1 : X / Z] memory) public {} //~ ERROR: failed to evaluate constant: unsupported expression
}
