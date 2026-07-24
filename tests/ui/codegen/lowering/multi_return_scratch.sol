//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=ORDER
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

    // ORDER-LABEL: fn @assign
    // ORDER: internal_call
    // ORDER: = mload 32
    // ORDER: = add {{.*}}, 32
    // ORDER: = mload
    // ORDER: = add {{.*}}, 64
    // ORDER: = mload
    // ORDER: = mapping_slot
    // ORDER: sstore
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
}
