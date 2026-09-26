//@ run-call: toIntegers 0xffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffff => 340282366920938463463374607431768211455, -1, 1461501637330902918203684832716283019655932542975, 0xffffffffffffffffffffffffffffffffffffffff
//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract FixedBytesCanonical {
    // CHECK-LABEL: fn @fromUint{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = zext i8 arg0 to i256
    // CHECK: [[SHIFTED:v[0-9]+]] = shl 248, [[VALUE]]
    function fromUint(uint8 value) external pure returns (bytes1) {
        return bytes1(value);
    }

    // CHECK-LABEL: fn @fromHex{{[( ]}}
    // CHECK: ret 0x100000000000000000000000000000000000000000000000000000000000000
    function fromHex() external pure returns (bytes1) {
        return hex"01";
    }

    // CHECK-LABEL: fn @compareElement{{[( ]}}
    // CHECK: memory_object_load_byte memorybytes, arg0, 0
    // CHECK: shl 248,
    // CHECK: eq
    function compareElement(bytes memory data) external pure returns (bool) {
        return data[0] == bytes1(uint8(1));
    }

    // CHECK-LABEL: fn @narrow{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = and arg0, 0xffffffff00000000000000000000000000000000000000000000000000000000
    // CHECK: [[MASKED:v[0-9]+]] = and [[VALUE]], 0xffff000000000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[MASKED]]
    function narrow(bytes4 value) external pure returns (bytes2) {
        return bytes2(value);
    }
    function toIntegers(bytes16 a, bytes20 b) external pure returns (uint128, int128, uint160, address) {
        return (uint128(a), int128(uint128(a)), uint160(b), address(b));
    }
}
