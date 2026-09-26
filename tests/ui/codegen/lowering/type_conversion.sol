//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract TypeConversion {
    // CHECK-LABEL: fn @narrowAddress(arg0: i160) -> i16
    // CHECK: [[RESULT:v[0-9]+]] = trunc i160 arg0 to i16
    // CHECK-NEXT: ret [[RESULT]]
    function narrowAddress(address asset) public pure returns (uint16) {
        return uint16(uint160(asset));
    }

    // CHECK-LABEL: fn @narrowUint(arg0: i256) -> i16
    // CHECK: [[RESULT:v[0-9]+]] = trunc i256 arg0 to i16
    // CHECK-NEXT: ret [[RESULT]]
    function narrowUint(uint256 value) public pure returns (uint16) {
        return uint16(value);
    }

    // CHECK-LABEL: fn @narrowSigned(arg0: i256) -> i8
    // CHECK: [[RESULT:v[0-9]+]] = trunc i256 arg0 to i8
    // CHECK-NEXT: ret [[RESULT]]
    function narrowSigned(int256 value) public pure returns (int8) {
        return int8(value);
    }

    // CHECK-LABEL: fn @widenUnsigned(arg0: i8) -> i256
    // CHECK: [[RESULT:v[0-9]+]] = zext i8 arg0 to i256
    // CHECK-NEXT: ret [[RESULT]]
    function widenUnsigned(uint8 value) public pure returns (uint256) {
        return uint256(value);
    }

    // CHECK-LABEL: fn @widenSigned(arg0: i8) -> i256
    // CHECK: [[RESULT:v[0-9]+]] = sext i8 arg0 to i256
    // CHECK-NEXT: ret [[RESULT]]
    function widenSigned(int8 value) public pure returns (int256) {
        return int256(value);
    }

    // CHECK-LABEL: fn @reinterpretUnsigned(arg0: i8) -> i8
    // CHECK: ret arg0
    function reinterpretUnsigned(int8 value) public pure returns (uint8) {
        return uint8(value);
    }

    // CHECK-LABEL: fn @reinterpretSigned(arg0: i8) -> i8
    // CHECK: ret arg0
    function reinterpretSigned(uint8 value) public pure returns (int8) {
        return int8(value);
    }
}
