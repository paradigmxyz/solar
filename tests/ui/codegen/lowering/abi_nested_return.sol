//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: alloc memorystruct<2>
    // CHECK: memory_object_store_field memorystruct<2>, [[PAIR:v[0-9]+]], 0
    // CHECK: memory_object_store_field memorystruct<2>, [[PAIR]], 1
    // CHECK: memory_object_store_element memoryarray<1>, {{v[0-9]+}}, 0
    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    // CHECK-LABEL: fn @nestedArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: set_memory_object_len memoryarray, {{v[0-9]+}}, arg0
    // CHECK: memory_object_store_element memoryarray<1>, {{v[0-9]+}}, 0
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
