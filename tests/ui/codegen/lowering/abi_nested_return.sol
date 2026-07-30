//@ignore-host: windows
//@run-call: staticPair 7 => (7, 8)
//@run-call: callStaticPair 11 => 23
//@run-call: callNestedArray 3 => 3
//@run-call: verifyDirectDelegatecall() => 42

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structArray{{[( ]}}
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
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
    // CHECK: [[OUT:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: [[INNER:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: set_memory_object_len memoryarray, [[INNER]], arg0
    // CHECK: memory_object_element_addr memoryarray<1>, [[OUT]], 0
    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }

    function staticPair(uint256 x) public pure returns (Pair memory) {
        return Pair(x, x + 1);
    }

    function callStaticPair(uint256 x) external view returns (uint256) {
        Pair memory pair = this.staticPair(x);
        return pair.a + pair.b;
    }

    function callNestedArray(uint256 n) external view returns (uint256) {
        uint256[][] memory out = this.nestedArray(n);
        return out[0].length;
    }

    function answer() external pure returns (uint256) {
        return 42;
    }

    function directDelegatecall() external returns (bool, bytes memory) {
        return address(this).delegatecall(abi.encodeWithSignature("answer()"));
    }

    function verifyDirectDelegatecall() external returns (uint256) {
        (bool success, bytes memory data) = this.directDelegatecall();
        require(success);
        return abi.decode(data, (uint256));
    }
}
