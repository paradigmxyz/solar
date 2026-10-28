//@ codegen-matrix: standard
//@ run-call: cleanThenReset 1 => 0x4d566eccc8f71bf494f0b2fb6bb6640b026e75b222ecdc6a7855738f31eace50
//@ run-call: cleanThenReset 7 => 0x95e1d1c39b1d7bf71ff2df32fd354fba590f652deaf3d0f558596d19742386fd

// A helper that hashes a memory object and allocates from the free memory pointer is called with
// a clean slot first and then after assembly moves the pointer below the heap. The call after the
// reset runs the helper's copy that raises the pointer, while the first runs the one without.
contract DirtyEntryCopies {
    function cleanThenReset(uint256 a) external pure returns (bytes32 result) {
        bytes[] memory list = new bytes[](1);
        list[0] = new bytes(64);
        bytes32 clean = _digest(list[0], a);
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        assembly {
            mstore(0x40, 0x80)
        }
        bytes32 dirty = _digest(list[0], a);
        result = clean ^ dirty ^ h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    /// @custom:solar-noinline
    function _digest(bytes memory data, uint256 a) private pure returns (bytes32) {
        bytes memory copy = new bytes(data.length);
        copy[0] = bytes1(uint8(a));
        return keccak256(abi.encodePacked(copy, data.length));
    }
}
