contract C {
    constructor(uint x) {
    }
}

contract D is C(x) { //~ ERROR: unresolved symbol
    uint x;
}

// OK
contract E is C {
    uint constant x = 69;
    constructor() C(x) {}
}
contract F is C() { // TODO: ~ERROR: mismatched base constructor arguments
    uint constant x = 69;
    constructor() C(x) {}
}
