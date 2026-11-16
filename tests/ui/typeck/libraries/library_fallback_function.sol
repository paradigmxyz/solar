// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/076_fallback_function_in_library.sol

library L {
    fallback() external {} //~ ERROR: libraries cannot have fallback functions
}

// A library `receive` gets its own error instead.
library L2 {
    receive() external payable {}
    //~^ ERROR: libraries cannot have receive ether functions
    //~| ERROR: library functions cannot be payable
}

// The restriction only applies to libraries.
contract C {
    fallback() external {}
}
