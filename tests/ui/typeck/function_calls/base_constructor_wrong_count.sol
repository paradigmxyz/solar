//@ compile-flags: -Ztypeck

contract Base {
    constructor(uint, int) {}
}

contract Derived1 is Base() {} //~ ERROR: wrong number of arguments for base constructor: expected 2, found 0

contract Derived2 is Base(1) {} //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1

contract Derived3 is Base {
    constructor() Base(2) {} //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1
}
