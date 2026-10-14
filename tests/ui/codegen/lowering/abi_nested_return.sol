//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc zeroed checked
    // CHECK: [[PAIR:v[0-9]+]] = alloc 64
    // CHECK: mstore [[PAIR]], arg0
    // CHECK-NEXT: mstore [[PAIR]]+32, {{v[0-9]+}}
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint ptr<memory> [[PAIR]] to i256
    // CHECK-NEXT: mstore [[OUT]]+32, [[ELEM]]
    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    // CHECK-LABEL: fn @nestedArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc zeroed checked
    // CHECK: [[INNER:v[0-9]+]] = alloc zeroed checked
    // CHECK: mstore [[INNER]], arg0
    // CHECK: [[ELEM:v[0-9]+]] = ptrtoint ptr<memory> [[INNER]] to i256
    // CHECK-NEXT: mstore [[OUT]]+32, [[ELEM]]
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
