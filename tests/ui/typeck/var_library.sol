//@compile-flags: -Ztypeck

library L {
    uint x;                //~ ERROR: library cannot have non-constant state variable
    uint constant c = 1;   // OK - constant
    uint immutable i;      //~ ERROR: library cannot have non-constant state variable
}

contract C {
    uint x;                // OK - not a library
    uint constant c = 1;   // OK
    uint immutable i;      // OK - not a library
}
