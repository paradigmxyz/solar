//@ compile-flags: -Ztypeck

contract Base1 {
    constructor(uint x) {}
}

contract Base2 {
    constructor(int y) {}
}

// OK: abstract contracts can skip base constructor args
abstract contract AbstractDerived is Base1 {}

// Not OK: non-abstract contract must provide args
contract DerivedMissing is Base1 { } //~ ERROR: no arguments passed to the base constructor

// OK: args provided via inheritance specifier
contract DerivedWithArgs is Base1(42) { }

// OK: args provided via constructor modifier
contract DerivedWithCtorArgs is Base1 {
    constructor() Base1(100) {}
}

// === Multiple Inheritance ===

// ERROR: Missing args for Base2
contract Bad2 is Base1(1), Base2 {} //~ ERROR: no arguments passed to the base constructor

// OK: All args provided for multiple bases
contract Good2 is Base1(1), Base2(2) {}

// OK: Multiple bases with constructor modifiers
contract Good3 is Base1, Base2 {
    constructor() Base1(1) Base2(2) {}
}

// ERROR: Missing one base in multiple inheritance
contract Bad3 is Base1, Base2 { //~ ERROR: no arguments passed to the base constructor
    constructor() Base1(1) {}
}

// OK: Abstract can skip all args even with multiple bases
abstract contract AbstractMulti is Base1, Base2 {}
