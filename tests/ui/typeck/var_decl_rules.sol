//@compile-flags: -Ztypeck

contract C {
    // Constant with non-compile-time initializer
    uint constant NON_CONST = block.timestamp;
    //~^ ERROR: mismatched types
    //~| ERROR: initial value for constant variable has to be compile-time constant

    // Valid constant
    uint constant VALID_CONST = 42;

    // Immutable with non-value type (array)
    uint[] immutable IMMUT_ARRAY; //~ ERROR: immutable variables cannot have a non-value type

    struct S {
        uint x;
    }

    // Immutable with non-value type (struct)
    S immutable IMMUT_STRUCT; //~ ERROR: immutable variables cannot have a non-value type

    // Struct containing mapping
    struct WithMapping {
        mapping(uint => uint) m;
    }

    // Valid mapping state variable (no initializer)
    mapping(uint => uint) validMapping;

    // Valid immutable
    uint immutable VALID_IMMUT;

    // Memory variable containing mapping is invalid
    function f() public {
        WithMapping memory localWithMapping; //~ ERROR: is only valid in storage because it contains a (nested) mapping
    }
}
