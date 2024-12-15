contract C {
    constructor(uint b) {}
}

contract D is C(3) {
    constructor() {}
}

contract E is C {
    constructor() C(5) {}
}
