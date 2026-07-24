//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract FixedBytesCanonical {
    // CHECK-LABEL: fn @fromUint
    // CHECK: [[SHIFTED:v[0-9]+]] = shl 248, arg0
    // CHECK: and [[SHIFTED]], 0xff00000000000000000000000000000000000000000000000000000000000000
    function fromUint(uint8 value) external pure returns (bytes1) {
        return bytes1(value);
    }

    // CHECK-LABEL: fn @fromHex
    // CHECK: mstore 128, 0x100000000000000000000000000000000000000000000000000000000000000
    function fromHex() external pure returns (bytes1) {
        return hex"01";
    }

    // CHECK-LABEL: fn @compareElement
    // CHECK: [[ELEMENT:v[0-9]+]] = and {{v[0-9]+}}, 0xff00000000000000000000000000000000000000000000000000000000000000
    // CHECK: {{v[0-9]+}} = shl 248, {{v[0-9]+}}
    // CHECK: eq [[ELEMENT]], {{v[0-9]+}}
    function compareElement(bytes memory data) external pure returns (bool) {
        return data[0] == bytes1(uint8(1));
    }

    // CHECK-LABEL: fn @narrow
    // CHECK: and arg0, 0xffff000000000000000000000000000000000000000000000000000000000000
    function narrow(bytes4 value) external pure returns (bytes2) {
        return bytes2(value);
    }
}
