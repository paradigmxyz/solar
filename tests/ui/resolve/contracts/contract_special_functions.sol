contract C {
    constructor() {}
    constructor() {} //~ ERROR: constructor function already declared

    fallback() external {}
    fallback() external {} //~ ERROR: fallback function already declared

    receive() external payable {}
    receive() external payable {} //~ ERROR: receive function already declared
}

contract D {
    constructor() {}
    constructor() {} //~ ERROR: constructor function already declared
    constructor() {} //~ ERROR: constructor function already declared

    fallback() external {}
    fallback() external {} //~ ERROR: fallback function already declared
    fallback() external {} //~ ERROR: fallback function already declared

    receive() external payable {}
    receive() external payable {} //~ ERROR: receive function already declared
    receive() external payable {} //~ ERROR: receive function already declared
}
