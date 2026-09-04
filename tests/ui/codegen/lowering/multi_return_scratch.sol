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
    // CHECK: icall
    // CHECK: frame_load multi_return, word, 0
    // CHECK: mload
    // CHECK: mload
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
