//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract TypeConversion {
    // CHECK-LABEL: fn @narrowAddress{{[( ]}}
    // CHECK: [[ADDRESS:v[0-9]+]] = and arg0, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: and [[ADDRESS]], 0xffff
    function narrowAddress(address asset) public pure returns (uint16) {
        return uint16(uint160(asset));
    }

    // CHECK-LABEL: fn @narrowUint{{[( ]}}
    // CHECK: and arg0, 0xffff
    function narrowUint(uint256 value) public pure returns (uint16) {
        return uint16(value);
    }

    // CHECK-LABEL: fn @narrowSigned{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl 248, arg0
    // CHECK: sar 248, [[SHIFTED]]
    function narrowSigned(int256 value) public pure returns (int8) {
        return int8(value);
    }
}
