// A calldata `bytes` slice (`data[i:j]`) converts like `bytes`: to fixed-bytes,
// `bytes`, or `string`. Converting to `bytes`/`string` keeps the calldata
// location (the result is a view), so it assigns to a calldata variable.
contract C {
    function toBytes32(bytes calldata d) external pure returns (bytes32) {
        return bytes32(d[0:32]);
    }
    function toBytes4(bytes calldata d) external pure returns (bytes4) {
        return bytes4(d[0:4]);
    }
    function toBytesCalldata(bytes calldata d) external pure {
        bytes calldata b = bytes(d[0:5]);
        b;
    }
    function toStringCalldata(bytes calldata d) external pure {
        string calldata s = string(d[0:5]);
        s;
    }
    function toStringMemory(bytes calldata d) external pure returns (string memory) {
        return string(d[0:5]);
    }

    function arraySlice(uint256[] calldata values) external pure {
        uint256[] calldata a = uint256[](values[1:3]);
        uint256[] memory b = uint256[](values[1:3]);
        a;
        b;
    }

    function invalidArraySlice(uint8[] calldata values) external pure {
        uint256[](values[1:3]); //~ ERROR: invalid explicit type conversion
    }
}
