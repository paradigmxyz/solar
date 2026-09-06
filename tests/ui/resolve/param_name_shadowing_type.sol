// ported-from: test/libsolidity/syntaxTests/errors/error_param_type_shadowed_by_param_name.sol
// ported-from: test/libsolidity/syntaxTests/events/event_param_type_shadowed_by_param_name.sol
// ported-from: test/libsolidity/syntaxTests/scoping/name_shadowing_function_type_parameter.sol
// ported-from: test/libsolidity/syntaxTests/scoping/name_shadowing_function_parameter_vs_struct_enum.sol

// The names of struct fields and of event, error, and function type parameters are invisible to
// unqualified name resolution, so they never shadow a type used by a sibling's type. The same
// holds for the parameters of a function without a body.

abstract contract C {
    enum EnumType {A, B, C}

    struct StructType {
        uint x;
    }

    error Er1(StructType StructType);
    error Er2(EnumType EnumType);
    error Er3(EnumType StructType, StructType EnumType);

    event Ev1(StructType StructType);
    event Ev2(EnumType EnumType);
    event Ev3(EnumType StructType, StructType EnumType);
    event Ev4(StructType indexed StructType) anonymous;

    struct S1 {
        EnumType StructType;
        StructType EnumType;
    }

    function (StructType memory StructType) external ext1; //~ WARN: named function type parameters are deprecated
    function (EnumType EnumType) external ext2; //~ WARN: named function type parameters are deprecated
    function (EnumType StructType, StructType memory EnumType) external ext3; //~ WARN: named function type parameters are deprecated
    //~^ WARN: named function type parameters are deprecated

    function unimplemented(EnumType StructType, StructType memory EnumType) external virtual;

    function unimplementedReturns() external virtual returns (EnumType StructType, StructType memory EnumType);
}

abstract contract D {
    enum EnumType {A, B, C}

    struct StructType {
        uint x;
    }

    // A function with a body declares its parameter names before resolving any of the types, so
    // every name shadows, including its own type's.
    function implemented(EnumType StructType, StructType memory EnumType) external {}
    //~^ ERROR: name has to refer to a valid user-defined type
    //~| ERROR: name has to refer to a valid user-defined type

    function implementedReturns() external returns (EnumType StructType, StructType memory EnumType) {}
    //~^ ERROR: name has to refer to a valid user-defined type
    //~| ERROR: name has to refer to a valid user-defined type

    function selfShadowing(StructType memory StructType) external {} //~ ERROR: name has to refer to a valid user-defined type

    function returnShadowsParameter(EnumType e) external returns (uint256 EnumType) { e; EnumType = 1; } //~ ERROR: name has to refer to a valid user-defined type
}

abstract contract E {
    enum EnumType {A, B, C}

    struct StructType {
        uint x;
    }

    // A modifier's parameters are visible even when it has no body, since solc's rule only
    // covers functions.
    modifier m(EnumType StructType, StructType memory EnumType) virtual;
    //~^ ERROR: name has to refer to a valid user-defined type
    //~| ERROR: name has to refer to a valid user-defined type

    function g() external virtual returns (EnumType, StructType memory);

    // The variables of a try/catch clause are visible too.
    function f() external {
        try this.g() returns (EnumType StructType, StructType memory EnumType) {}
        //~^ ERROR: name has to refer to a valid user-defined type
        //~| ERROR: name has to refer to a valid user-defined type
        catch {}
    }
}
