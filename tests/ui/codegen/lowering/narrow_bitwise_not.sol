//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:
//@ normalize-stdout-test: "\n(\n)$" -> "$1"

contract NarrowBitwiseNot {
    // CHECK-LABEL: fn @notUint8{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[NOT]], 255
    // CHECK-NEXT: ret [[CLEAN]]
    function notUint8(uint8 value) external pure returns (uint8) {
        return ~value;
    }

    // CHECK-LABEL: fn @notUint16{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[NOT]], 0xffff
    // CHECK-NEXT: ret [[CLEAN]]
    function notUint16(uint16 value) external pure returns (uint16) {
        return ~value;
    }

    // CHECK-LABEL: fn @notUint256{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: ret [[NOT]]
    function notUint256(uint256 value) external pure returns (uint256) {
        return ~value;
    }

    // CHECK-LABEL: fn @notInt8{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: ret [[NOT]]
    function notInt8(int8 value) external pure returns (int8) {
        return ~value;
    }

    // CHECK-LABEL: fn @notBytes1{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[NOT]], 0xff00000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: ret [[CLEAN]]
    function notBytes1(bytes1 value) external pure returns (bytes1) {
        return ~value;
    }

    // CHECK-LABEL: fn @notBytes2{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[NOT]], 0xffff000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: ret [[CLEAN]]
    function notBytes2(bytes2 value) external pure returns (bytes2) {
        return ~value;
    }

    // CHECK-LABEL: fn @notBytes32{{[( ]}}
    // CHECK: [[NOT:v[0-9]+]] = not arg0
    // CHECK-NEXT: ret [[NOT]]
    function notBytes32(bytes32 value) external pure returns (bytes32) {
        return ~value;
    }
}
