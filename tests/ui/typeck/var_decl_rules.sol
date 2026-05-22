//@compile-flags: -Ztypeck

contract C {
    // Immutable with non-value type (array)
    uint[] immutable IMMUT_ARRAY; //~ ERROR: immutable variables cannot have a non-value type

    struct S {
        uint x;
    }

    // Immutable with non-value type (struct)
    S immutable IMMUT_STRUCT; //~ ERROR: immutable variables cannot have a non-value type

    // Immutable with external function type
    function(uint) external immutable IMMUT_EXT_FN; //~ ERROR: immutable variables of external function type are not yet supported

    // Struct containing mapping
    struct WithMapping {
        mapping(uint => uint) m;
    }

    // Immutable struct with mapping triggers non-value type error
    WithMapping immutable IMMUT_WITH_MAPPING; //~ ERROR: immutable variables cannot have a non-value type

    // State variable with mapping cannot have initializer
    WithMapping stateWithMapping = WithMapping(); //~ ERROR: types in storage containing (nested) mappings cannot be assigned to

    // Valid mapping state variable (no initializer)
    mapping(uint => uint) validMapping;

    // Valid immutable
    uint immutable VALID_IMMUT;

    // Valid immutable with internal function type
    function(uint) internal immutable VALID_IMMUT_INT_FN;

    // Memory variable containing mapping is invalid
    function f() internal {
        WithMapping memory localWithMapping; //~ ERROR: is only valid in storage because it contains a (nested) mapping

        mapping(uint => uint) memory m; //~ ERROR: is only valid in storage because it contains a (nested) mapping
        //~^ ERROR: uninitialized mapping
    }

    // Calldata variable containing mapping is also invalid
    function g(WithMapping calldata w) internal {} //~ ERROR: is only valid in storage because it contains a (nested) mapping

    // Fixed arrays that exceed the memory/calldata type-size limit are invalid
    function maxMemory(uint[134217727] memory a) public pure {}
    function tooLargeMemory(uint[85678901234] memory a) public pure {} //~ ERROR: type too large for memory
    function justTooLargeMemory(uint[134217728] memory a) public pure {} //~ ERROR: type too large for memory
    function maxCalldata(uint[134217727] calldata a) external pure {}
    function tooLargeCalldata(uint[85678901234] calldata a) external pure {} //~ ERROR: type too large for calldata
    function justTooLargeCalldata(uint[134217728] calldata a) external pure {} //~ ERROR: type too large for calldata

    // Uninitialized storage mapping in local variable is invalid
    function h() internal {
        mapping(uint => uint) storage s; //~ ERROR: uninitialized mapping
    }
}
