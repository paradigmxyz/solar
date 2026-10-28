//@ codegen-matrix: standard
//@ run-call: clearedHighBytes => 7

contract Test {
    // A pointer's full word can exceed the heap floor while its low eight bytes are zero.
    function clearedHighBytes() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0x40, shl(64, 1))
            mstore(0, 7)
            mstore(0x38, 0)
            // Keep the slot's word as data across heap-floor pointer clamping.
            mstore(0x20, mload(0x40))
            result := mload(mload(0x20))
            mstore(0x40, free)
        }
    }
}
