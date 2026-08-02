//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

struct InitInput {
    address asset;
    uint8 decimals;
    string name;
    bytes params;
}

struct StaticPair {
    uint256 x;
    address who;
}

contract DynamicStructParam {
    // A struct with dynamic members stays typed in built MIR. The ABI phase
    // rebuilds its fields recursively and keeps the source base for calldata
    // slices in a trailing word.
    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: calldataload 36
    // CHECK: memory_object_field_addr memorystruct<4>, arg0, 1
    // CHECK: gt {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffff
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // A static scalar struct stays typed until the ABI phase.
    // CHECK-LABEL: fn @flat{{[( ]}}
    // CHECK: memory_object_field_addr memorystruct<2>, arg0, 0
    // CHECK: mload
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }
}
