contract Base {
    constructor(uint, int) {}
}
contract Derived is Base(2, 3) { }
contract Derived1 is Base {
    constructor() Base(3, 4) {}
}
contract Derived2 is Base {
    constructor() Base(5, 6) {}
}
contract Derived3 is Base() { } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 0
contract Derived4 is Base(1) { } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1
contract Derived5 is Base { constructor() Base(2) {} } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1
contract Derived6 is Base { constructor() Base("a", 1) {} } //~ ERROR: mismatched types

contract NamedBase {
    constructor(uint8 a, uint256 b) {}
}
contract Named1 is NamedBase({b: 1, a: 2}) { }
contract Named2 is NamedBase {
    constructor(uint8 x, uint256 y) NamedBase({b: y + 1, a: x + 1}) {}
}
contract Named3 is NamedBase({nope: 1, b: 2}) { } //~ ERROR: named argument `nope` does not match function declaration
contract Named4 is NamedBase({a: 1, a: 2}) { } //~ ERROR: duplicate named argument `a`
contract Named5 is NamedBase({a: 1}) { } //~ ERROR: wrong number of arguments for base constructor: expected 2, found 1
