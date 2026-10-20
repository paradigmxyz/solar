//@ codegen-matrix: standard
//@ run-call: 0xf50377750000000000000000000000000000000000000000000000000000000000000005000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000040 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: 0xf50377750000000000000000000000000000000000000000000000000000000000000005000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000040 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: 0xf50377750000000000000000000000000000000000000000000000000000000000000005000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000041 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: addressed 5, 1, 0x40 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: addressed 5, 0, 0x40 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: addressed 5, 1, 0x41 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
// An edge that proves a calldata-derived address or size equal to a constant must not
// turn it into a fixed memory image: spill slots live below the heap, and only a dynamic
// raw memory access moves them out of the way of the allocation after the guard.
contract EdgeEqualityMemory {
    function sized(uint256 a, uint256 flag, uint256) external pure returns (bytes32 result) {
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
            let o := calldataload(0x44)
            let n := calldataload(0x64)
            if and(eq(o, 0x80), eq(n, 0x40)) {
                calldatacopy(o, 0, n)
                calldatacopy(o, 0, n)
            }
        }
        if (flag != 0) {
            bytes memory c = new bytes(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function addressed(uint256 a, uint256 flag, uint256) external pure returns (bytes32 result) {
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
            let o := calldataload(0x44)
            let n := calldataload(0x64)
            if eq(o, 0x40) {
                mstore(o, 0x100)
                mstore(o, 0x80)
            }
        }
        if (flag != 0) {
            bytes memory c = new bytes(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }
}
