//@compile-flags: -O none -Zdump=mir
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
    // CHECK: [[VIEW:v[0-9]+]] = memory_slice [[OUT]]
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint memptr [[PAIR]] to i256
    // CHECK: slice_store_element [[VIEW]], 0, [[ELEM]]
    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    // CHECK-LABEL: fn @nestedArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: [[INNER:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: [[INNER_HEAD:v[0-9]+]] = ptrtoint memptr [[INNER]] to i256
    // CHECK: mstore [[INNER_HEAD]], arg0
    // CHECK: [[VIEW:v[0-9]+]] = memory_slice [[OUT]]
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint memptr [[INNER]] to i256
    // CHECK: slice_store_element [[VIEW]], 0, [[ELEM]]
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
