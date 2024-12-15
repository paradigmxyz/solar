contract C {
    constructor(uint b) {}
}

contract D is C(3) {
    constructor() {}
}

contract E is C {
    constructor() C(5) {}
}

contract F is E { //~ERROR: contract makes multiple calls to base constructor C
    constructor() C(2) {} 
}

contract G is F {} //~ERROR: contract makes multiple calls to base constructor C
