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
    // Calldata structs stay as slices until code reads a field. Dynamic fields
    // keep their source base and are decoded only when accessed.
    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: calldata_slice_load_word calldata
    // CHECK: {{v[0-9]+}} = and {{v[0-9]+}}, 255
    // CHECK: gt {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffff
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // Static scalar structs load fields directly from calldata.
    // CHECK-LABEL: fn @flat{{[( ]}}
    // CHECK: calldata_slice_load_word calldata
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }

    // A full-word dynamic array field stays a calldata slice.
    // CHECK-LABEL: fn @words{{[( ]}}
    // CHECK: calldata_slice_load_word calldata
    // CHECK: slice_len
    function words(WordList calldata input) external pure returns (uint256) {
        return input.values.length + input.bias;
    }

    // Signed full-word arrays use the same calldata slice path.
    // CHECK-LABEL: fn @signedWords{{[( ]}}
    // CHECK: calldata_slice_load_word calldata
    // CHECK: slice_len
    function signedWords(SignedList calldata input) external pure returns (uint256) {
        return input.values.length + input.bias;
    }

    // Nested dynamic arrays remain lazy until their accessed element.
    // CHECK-LABEL: fn @nestedWords{{[( ]}}
    // CHECK: calldata_slice_load_word calldata
    // CHECK: slice_len
    function nestedWords(NestedList calldata input) external pure returns (uint256) {
        return input.values.length + input.values[0].length;
    }
}
