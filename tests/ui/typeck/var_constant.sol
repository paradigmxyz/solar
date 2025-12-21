//@compile-flags: -Ztypeck

// Test constant validation with compile-time constant evaluation
contract TestConstants {
    // Constant with non-constant initializers - ERROR
    uint nonConstant;
    uint constant refNonConstant = nonConstant; //~ ERROR: initial value for constant variable has to be compile-time constant

    uint constant blockTime = block.timestamp; //~ ERROR: initial value for constant variable has to be compile-time constant
}
