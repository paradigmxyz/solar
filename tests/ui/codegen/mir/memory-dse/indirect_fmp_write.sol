//@ codegen-matrix: standard
//@ run-call: unknownWrite 64 => 7
//@ run-call: wrappedWrite => 768

contract Test {
    // An indirect store can reset the free memory pointer to a scratch address.
    function unknownWrite(uint256 offset) external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0, 7)
            mstore(offset, 0)
            result := mload(mload(0x40))
            mstore(0x40, free)
        }
    }

    function wrappedWrite() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            mstore(0x40, 0x400)
            mstore(0x38, not(0))
            mstore(add(mload(0x40), 0xfffffffffffffc40), 0x300)
            result := mload(0x40)
            mstore(0x40, free)
        }
    }
}
