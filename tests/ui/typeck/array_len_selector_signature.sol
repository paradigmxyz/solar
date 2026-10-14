// A selector whose signature depends on a constant does not fold: the signature here depends on
// the constant being evaluated.
contract C {
    uint constant N = C.f.selector.length; //~ ERROR: initial value for constant variable has to be compile-time constant

    function f(uint[N] memory) external {} //~ ERROR: failed to evaluate constant: unsupported expression
}
