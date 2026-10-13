// ported-from: test/libsolidity/syntaxTests/dataLocations/data_location_in_function_type_fail.sol

library L {
    struct Nested { uint y; }
    function b(function(Nested calldata) external returns (uint)[] storage) external pure {}
    function d(function(Nested storage) external returns (uint)[] storage) external pure {} //~ ERROR: invalid data location `storage`
    function f(function(Nested transient) external returns (uint)[] storage) external pure {}
    //~^ WARN: named function type parameters are deprecated
    //~| ERROR: expected data location
}
