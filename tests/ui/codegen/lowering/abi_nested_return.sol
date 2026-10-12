//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: [[PAIR:v[0-9]+]] = alloc memorystruct<2>
    // CHECK: [[FIELDS:v[0-9]+]] = make_memory_slice [[PAIR]], 2
    // CHECK-NEXT: slice_store_element [[FIELDS]], 0
    // CHECK-NEXT: slice_store_element [[FIELDS]], 1
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
    // CHECK: mstore [[INNER]], arg0
    // CHECK: [[VIEW:v[0-9]+]] = memory_slice [[OUT]]
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint memptr [[INNER]] to i256
    // CHECK: slice_store_element [[VIEW]], 0, [[ELEM]]
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
