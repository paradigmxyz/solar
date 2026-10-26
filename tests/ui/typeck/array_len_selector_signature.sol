// An array length cannot read a selector, even one whose signature depends on the length.
contract C {
    uint constant N = C.f.selector.length; //~ ERROR: initial value for constant variable has to be compile-time constant

    function f(uint[N] memory) external {} //~ ERROR: failed to evaluate constant: unsupported expression
}
