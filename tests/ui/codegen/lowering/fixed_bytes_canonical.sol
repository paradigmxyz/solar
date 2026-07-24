//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract FixedBytesCanonical {
    function fromUint(uint8 value) external pure returns (bytes1) {
        return bytes1(value);
    }

    function fromHex() external pure returns (bytes1) {
        return hex"01";
    }

    function compareElement(bytes memory data) external pure returns (bool) {
        return data[0] == bytes1(uint8(1));
    }

    function narrow(bytes4 value) external pure returns (bytes2) {
        return bytes2(value);
    }
}
