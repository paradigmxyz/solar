//@compile-flags: -Ztypeck

contract BytesStringTests {
    // Valid: string memory -> bytes memory (via unlocated cast).
    function testStringToBytes(string memory s) public pure returns (bytes memory) {
        return bytes(s);
    }

    // Valid: bytes memory -> string memory (via unlocated cast).
    function testBytesToString(bytes memory b) public pure returns (string memory) {
        return string(b);
    }

    // Valid: string calldata -> bytes calldata.
    function testCalldataStringToBytes(string calldata s) external pure returns (bytes calldata) {
        return bytes(s);
    }

    // Valid: bytes calldata -> string calldata.
    function testCalldataBytesToString(bytes calldata b) external pure returns (string calldata) {
        return string(b);
    }

    // Valid: same location Ref -> Ref conversion.
    function testRefToRef(string memory s, bytes memory b) public pure {
        bytes memory b1 = bytes(s);
        string memory s1 = string(b);
    }
}

contract BytesStringErrors {
    // Invalid: cannot convert bytes memory to string calldata (location mismatch).
    function testLocationMismatch(bytes memory b) external pure {
        string calldata s = string(b); //~ ERROR: mismatched types
    }

    // Invalid: cannot convert uint256 to string.
    function testInvalidToString(uint256 x) public pure {
        string memory s = string(x); //~ ERROR: invalid explicit type conversion
    }

    // Invalid: cannot convert string to uint256.
    function testInvalidFromString(string memory s) public pure {
        uint256 x = uint256(s); //~ ERROR: invalid explicit type conversion
    }
}
