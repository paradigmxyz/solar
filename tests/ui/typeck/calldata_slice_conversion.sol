//@ compile-flags: -Ztypeck

// A calldata `bytes` slice (`data[i:j]`) converts like `bytes`: to fixed-bytes,
// `bytes`, or `string`.
contract C {
    function toBytes32(bytes calldata d) external pure returns (bytes32) {
        return bytes32(d[0:32]);
    }
    function toBytes4(bytes calldata d) external pure returns (bytes4) {
        return bytes4(d[0:4]);
    }
    function toBytes(bytes calldata d) external pure returns (bytes memory) {
        return bytes(d[0:5]);
    }
    function toString(bytes calldata d) external pure returns (string memory) {
        return string(d[0:5]);
    }
}
