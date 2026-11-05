// ported-from: test/libsolidity/syntaxTests/constructor/library_constructor.sol

library Lib {
    constructor() {} //~ ERROR: constructor cannot be defined in libraries
}

// The restriction only applies to libraries.
contract C {
    constructor() {}
}
