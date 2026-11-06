//@ codegen-matrix: standard
//@ compile-flags: --evm-version byzantium
//@ run-call: layout 5, 40, 5 => 0x6ae76ef32a2e583db4df1f208a32cb3bba18e72c5e466a519d26ab93240a6e36

// Before Constantinople, a Solidity shift of calldata becomes `div` or `exp` arithmetic, which
// keeps calldata as the input of an address that assembly writes through. The backend still
// moves its spill area above the words that the assembly fills.
contract LegacyShiftLayout {
    function layout(uint256 a, uint256 words, uint256 shift) external pure returns (bytes32 result) {
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
        if (words != 0) {
            uint256 size;
            uint256 amount;
            assembly {
                size := calldataload(0x24)
                amount := calldataload(0x44)
            }
            size = size << amount;
            assembly {
                let end := add(0xa0, size)
                for { let ptr := 0xa0 } lt(ptr, end) { ptr := add(ptr, 0x20) } {
                    mstore(ptr, a)
                }
                mstore(end, a)
                result := keccak256(0xa0, add(sub(end, 0xa0), 0x20))
            }
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }
}
