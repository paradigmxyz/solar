//@ compile-flags: -Ztypeck

contract Base {
    constructor(uint, int) {}
}
contract Derived3 is Base() { } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 0
contract Derived4 is Base(1) { } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1
contract Derived5 is Base { constructor() Base(2) {} } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1

// TODO: uncomment this when we have implicit conversions for integer literals
// contract Derived is Base(2, 3) { }
// contract Derived1 is Base { 
//     constructor() Base(3, 4) {}
// }
// contract Derived2 is Base {
//     constructor() Base(5, 6) {}
// }
// contract Derived6 is Base { constructor() Base("a", 1) {} }
