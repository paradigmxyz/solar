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
    // CHECK: store [[PAIR]], arg0
    // CHECK-NEXT: [[FIELD1:v[0-9]+]] = ptradd [[PAIR]], 32
    // CHECK-NEXT: store [[FIELD1]], {{v[0-9]+}}
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint memptr [[PAIR]] to i256
    // CHECK-NEXT: [[SLOT:v[0-9]+]] = ptradd [[OUT]], 32
    // CHECK-NEXT: store [[SLOT]], [[ELEM]]
    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    // CHECK-LABEL: fn @nestedArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: [[INNER:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: store [[INNER]], arg0
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint memptr [[INNER]] to i256
    // CHECK-NEXT: [[SLOT:v[0-9]+]] = ptradd [[OUT]], 32
    // CHECK-NEXT: store [[SLOT]], [[ELEM]]
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
