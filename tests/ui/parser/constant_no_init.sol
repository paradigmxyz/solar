// Test: constant variable must have an initializer

uint constant BAD; //~ ERROR: constant variable must be initialized

contract C {
    uint constant ALSO_BAD; //~ ERROR: constant variable must be initialized
    
    // Valid constants
    uint constant GOOD = 42;
}
