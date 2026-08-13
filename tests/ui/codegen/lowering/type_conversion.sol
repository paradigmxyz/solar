//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract TypeConversion {
    // CHECK-LABEL: fn @narrowAddress{{[( ]}}
    // CHECK: [[NARROW:v[0-9]+]] = and arg0, 0xffff
    // CHECK: ret [[NARROW]]
    function narrowAddress(address asset) public pure returns (uint16) {
        return uint16(uint160(asset));
    }

    // CHECK-LABEL: fn @narrowUint{{[( ]}}
    // CHECK: [[NARROW:v[0-9]+]] = and arg0, 0xffff
    // CHECK: ret [[NARROW]]
    function narrowUint(uint256 value) public pure returns (uint16) {
        return uint16(value);
    }

    // CHECK-LABEL: fn @narrowSigned{{[( ]}}
    // CHECK: [[NARROW:v[0-9]+]] = signextend 0, arg0
    // CHECK: ret [[NARROW]]
    function narrowSigned(int256 value) public pure returns (int8) {
        return int8(value);
    }

    // CHECK-LABEL: fn @widenUnsigned{{[( ]}}
    // CHECK: ret arg0
    function widenUnsigned(uint8 value) public pure returns (uint256) {
        return uint256(value);
    }

    // CHECK-LABEL: fn @widenSigned{{[( ]}}
    // CHECK: ret arg0
    function widenSigned(int8 value) public pure returns (int256) {
        return int256(value);
    }

    // CHECK-LABEL: fn @reinterpretUnsigned{{[( ]}}
    // CHECK: [[CLEAN:v[0-9]+]] = and arg0, 255
    // CHECK: ret [[CLEAN]]
    function reinterpretUnsigned(int8 value) public pure returns (uint8) {
        return uint8(value);
    }

    // CHECK-LABEL: fn @reinterpretSigned{{[( ]}}
    // CHECK: [[CLEAN:v[0-9]+]] = signextend 0, arg0
    // CHECK: ret [[CLEAN]]
    function reinterpretSigned(uint8 value) public pure returns (int8) {
        return int8(value);
    }
}
