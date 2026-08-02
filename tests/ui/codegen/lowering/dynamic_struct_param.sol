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

struct WordList {
    uint256[] values;
    uint256 bias;
}

struct SignedList {
    int256[] values;
    uint256 bias;
}

struct NestedList {
    uint256[][] values;
}

contract DynamicStructParam {
    // A struct with dynamic members stays typed in built MIR. The ABI phase
    // rebuilds its fields recursively and keeps the source base for calldata
    // slices in a trailing word.
    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: calldataload 36
    // CHECK: memory_object_load_field memorystruct<4>, arg0, 1
    // CHECK: gt {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffff
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // A static scalar struct stays typed until the ABI phase.
    // CHECK-LABEL: fn @flat{{[( ]}}
    // CHECK: memory_object_load_field memorystruct<2>, arg0, 0
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }

    // A full-word dynamic array field is decoded by the ABI phase.
    // CHECK-LABEL: fn @words{{[( ]}}
    // CHECK: memory_object_load_field memorystruct<2>, arg0, 0
    // CHECK: mload
    function words(WordList calldata input) external pure returns (uint256) {
        return input.values.length + input.bias;
    }

    // Signed full-word arrays use the same bulk-copy ABI path.
    // CHECK-LABEL: fn @signedWords{{[( ]}}
    // CHECK: memory_object_load_field memorystruct<2>, arg0, 0
    function signedWords(SignedList calldata input) external pure returns (uint256) {
        return input.values.length + input.bias;
    }

    // Nested dynamic arrays are decoded as arrays of typed memory objects.
    // CHECK-LABEL: fn @nestedWords{{[( ]}}
    // CHECK: memory_object_load_element memoryarray<1>, {{v[0-9]+}}
    function nestedWords(NestedList calldata input) external pure returns (uint256) {
        return input.values.length + input.values[0].length;
    }
}
