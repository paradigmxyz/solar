//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: [[PAIR:v[0-9]+]] = alloc memorystruct<2>
    // CHECK: memory_object_field_addr memorystruct<2>, [[PAIR]], 0
    // CHECK: memory_object_field_addr memorystruct<2>, [[PAIR]], 1
    // CHECK: memory_object_element_addr memoryarray<1>, [[OUT]], 0
    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    // CHECK-LABEL: fn @nestedArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: [[INNER:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: set_memory_object_len memoryarray, [[INNER]], arg0
    // CHECK: memory_object_element_addr memoryarray<1>, [[OUT]], 0
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
