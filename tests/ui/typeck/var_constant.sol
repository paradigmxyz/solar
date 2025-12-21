//@compile-flags: -Ztypeck

// Test constant validation with compile-time constant evaluation
contract TestConstants {
    // Constant with non-constant initializers - ERROR
    uint nonConstant;
    uint constant refNonConstant = nonConstant; //~ ERROR: failed to evaluate constant: only constant variables are allowed

    uint constant blockTime = block.timestamp; //~ ERROR: failed to evaluate constant: unsupported expression
    //~^ ERROR: mismatched types
}
