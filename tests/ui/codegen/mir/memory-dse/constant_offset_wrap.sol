//@ codegen-matrix: standard
//@ run-call: wrappedRead => 7
//@ run-call: wrappedCommutedRead => 7
//@ run-call: wrappedCopy => 7

contract ConstantOffsetWrap {
    // The high 24 bytes of the free memory pointer do not affect its heap floor, but a constant
    // addition can wrap the resulting word to the scratch address. Restore it before returning.
    function wrappedRead() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0x40, 0x400)
            mstore(0, 7)
            mstore(0x38, not(0))
            result := mload(add(mload(0x40), 0xfffffffffffffc00))
            mstore(0x40, free)
        }
    }

    function wrappedCommutedRead() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0x40, 0x400)
            mstore(0, 7)
            mstore(0x38, not(0))
            result := mload(add(0xfffffffffffffc00, mload(0x40)))
            mstore(0x40, free)
        }
    }

    function wrappedCopy() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0x40, 0x400)
            mstore(0, 7)
            mstore(0x38, not(0))
            mcopy(0x80, add(mload(0x40), 0xfffffffffffffc00), 0x20)
            result := mload(0x80)
            mstore(0x40, free)
        }
    }
}
