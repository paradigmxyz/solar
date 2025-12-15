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
contract F is C() { //~HELP: remove parentheses if you do not want to provide arguments here
    uint constant x = 69;
    constructor() C(x) {} //~ERROR: base constructor arguments given here
}
