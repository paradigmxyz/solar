//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract TypeConversion {
    function narrowAddress(address asset) public pure returns (uint16) {
        return uint16(uint160(asset));
    }

    function narrowUint(uint256 value) public pure returns (uint16) {
        return uint16(value);
    }

    function narrowSigned(int256 value) public pure returns (int8) {
        return int8(value);
    }
}
