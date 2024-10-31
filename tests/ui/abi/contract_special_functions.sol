//@ignore-host: windows
//@compile-flags: --emit=abi,hashes --pretty-json

// Abstract contracts don't emit constructors.
abstract contract A {
    constructor() payable {}
    fallback() external {}
}

abstract contract B is A {
    receive() external payable {}
}

contract C {
    constructor() {}
    fallback() external {}
}

contract D is C {
    constructor() payable {}
    receive() external payable {}
}

// Inherits `C.fallback`, but not the constructor.
contract E is C {}

// Inherits `C.fallback` and `D.receive`, but not any of the constructors.
contract F is D {}
