// `type(E).max` is not an array length solc's evaluator computes, even of an enum without variants.
enum E {} //~ ERROR: enum must have at least one variant

contract C {
    uint[uint(type(E).max) + 1] a; //~ ERROR: failed to evaluate constant: unsupported expression
}
