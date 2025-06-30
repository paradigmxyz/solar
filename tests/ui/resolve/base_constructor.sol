abstract contract C {
    constructor(uint, bool) {}
}

abstract contract D is C {}

abstract contract E is C {
    constructor() {}
}

abstract contract F is C {
    constructor() C {} //~ERROR: modifier-style base constructor call without arguments
}

contract J is C(1337, false) { //~HELP: previous declaration
    constructor() C(1337, false) {} //~ERROR: base constructor arguments given twice
}

contract G is C {
    constructor(uint x) C(x, false) {}
}

// ok
contract K is C(1337, false) {
    constructor() {}
}
