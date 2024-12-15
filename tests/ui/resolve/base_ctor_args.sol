contract C {
    constructor(uint b) {}
}

contract D is C(3) {
    constructor() {}
}

contract E is C {
    constructor() C(5) {}
}

contract F is E {
    constructor() C(2) {} //~ERROR: in F, the base contract C's constructor arguments are called multiple times
    //~^ERROR: in G, the base contract C's constructor arguments are called multiple times
}

contract G is F {}
