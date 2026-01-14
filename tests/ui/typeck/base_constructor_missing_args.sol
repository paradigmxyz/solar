//@ compile-flags: -Ztypeck

contract Base {
    constructor(uint, int) {}
}

// OK: abstract contracts can skip base constructor args
abstract contract AbstractDerived is Base {}

// Not OK: non-abstract contract must provide args
contract DerivedMissing is Base { } //~ ERROR: no arguments passed to the base constructor

// OK: args provided via inheritance specifier
contract DerivedWithArgs is Base(1, 2) { }

// OK: args provided via constructor modifier
contract DerivedWithCtorArgs is Base {
    constructor() Base(1, 2) {}
}
