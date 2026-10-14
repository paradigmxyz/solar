// `type(E).max` of an enum without variants reports nothing beyond the enum's error.
enum E {} //~ ERROR: enum must have at least one variant

contract C {
    uint[uint(type(E).max) + 1] a;
}
