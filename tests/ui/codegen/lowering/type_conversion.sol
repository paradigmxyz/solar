//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract TypeConversion {
    // CHECK-LABEL: fn @narrowAddress{{[( ]}}
    // CHECK: ret arg0
    function narrowAddress(address asset) public pure returns (uint16) {
        return uint16(uint160(asset));
    }

    // CHECK-LABEL: fn @narrowUint{{[( ]}}
    // CHECK: ret arg0
    function narrowUint(uint256 value) public pure returns (uint16) {
        return uint16(value);
    }

    // CHECK-LABEL: fn @narrowSigned{{[( ]}}
    // CHECK: ret arg0
    function narrowSigned(int256 value) public pure returns (int8) {
        return int8(value);
    }
}
