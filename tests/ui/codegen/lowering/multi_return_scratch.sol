//@compile-flags: -O none -Zdump=mir
//@filecheck:
// Multi-return tails live at the free-memory pointer, and every tail word is
// loaded before the first tuple lvalue is evaluated. Computing `stored[key]`
// may use mapping-slot scratch during later lowering, so assigning it first
// used to corrupt `second`; the third return also used to overwrite the
// free-memory pointer at word 64.
contract MultiReturnScratch {
    mapping(uint256 => uint256) public stored;

    function triple(uint256 x) internal pure returns (uint256, uint256, uint256) {
        if (x == 0) return (1, 2, 3);
        return (x, x + 1, x + 2);
    }

    // CHECK-LABEL: fn @assign{{[( ]}}
    // CHECK: [[TRIPLE:v[0-9]+]] = icall
    // CHECK: extract_value {{struct[0-9]+}}, [[TRIPLE]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[TRIPLE]], 1
    // CHECK: extract_value {{struct[0-9]+}}, [[TRIPLE]], 2
    // CHECK: = mapping_slot
    // CHECK: sstore
    function assign(uint256 key, uint256 seed)
        external
        returns (uint256 second, uint256 third, uint256 beforePtr, uint256 afterPtr)
    {
        assembly ("memory-safe") {
            beforePtr := mload(0x40)
        }
        (stored[key], second, third) = triple(seed);
        assembly ("memory-safe") {
            afterPtr := mload(0x40)
        }
    }

    // CHECK-LABEL: fn @ternary{{[( ]}}
    // CHECK: phi [
    // CHECK: ret
    function ternary(bool pick, uint256 x) external pure returns (uint256, uint256) {
        return pick ? (x, x + 1) : (x + 2, x + 3);
    }
}
