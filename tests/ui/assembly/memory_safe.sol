// Test memory-safe assembly flag

contract MemorySafe {
    function memorySafe() public pure returns (uint256 result) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, 42)
            mstore(0x40, add(ptr, 32))
            result := mload(ptr)
        }
    }

    function withDialect() public pure returns (uint256 result) {
        assembly "evmasm" {
            result := 42
        }
    }

    function dialectAndFlag() public pure returns (uint256 result) {
        assembly "evmasm" ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, 100)
            mstore(0x40, add(ptr, 32))
            result := mload(ptr)
        }
    }
}
