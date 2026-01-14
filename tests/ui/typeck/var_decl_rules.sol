//@compile-flags: -Ztypeck

contract C {
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
    function f() internal {
        WithMapping memory localWithMapping; //~ ERROR: is only valid in storage because it contains a (nested) mapping

        mapping(uint => uint) memory m; //~ ERROR: is only valid in storage because it contains a (nested) mapping
    }

    // Calldata variable containing mapping is also invalid
    function g(WithMapping calldata w) internal {} //~ ERROR: is only valid in storage because it contains a (nested) mapping
}
