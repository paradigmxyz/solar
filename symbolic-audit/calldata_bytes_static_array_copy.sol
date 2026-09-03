// Copying a `bytes[2] calldata` argument to memory. solc returns normally;
// solar reverts with Panic(0x32). Agreed on builds before 1738b4454.
// Source: testdata/solidity/test/libsolidity/semanticTests/array/copying/calldata_2d_bytes_to_memory_2.sol
contract CalldataBytesStaticArrayCopy {
    function copyAll(bytes[2] calldata c) external pure returns (uint256, uint256) {
        bytes[2] memory m = c;
        return (m[0].length, m[1].length);
    }

    function copyIndex(bytes[2] calldata c) external pure returns (bytes1) {
        bytes[2] memory m = c;
        return m[1][0];
    }

    function copySecondByte(bytes[2] calldata c) external pure returns (bytes1) {
        bytes[2] memory m = c;
        return m[0][1];
    }

    function original(bytes[2] calldata c) external pure returns (bool) {
        bytes[2] memory m = c;
        return m[0].length > 1 && m[1].length > 1 && m[0][0] == m[1][0] && m[0][1] == m[1][1];
    }

    function directLength(bytes[2] calldata c) external pure returns (uint256) {
        return c[1].length;
    }

    function directIndex(bytes[2] calldata c) external pure returns (bytes1) {
        return c[1][0];
    }

    function passToInternal(bytes[2] calldata c) external pure returns (uint256) {
        return inner(c);
    }

    function inner(bytes[2] memory m) internal pure returns (uint256) {
        return m[0].length + m[1].length;
    }
}
