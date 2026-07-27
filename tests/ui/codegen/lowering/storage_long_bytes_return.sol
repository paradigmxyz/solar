//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract StorageLongBytesReturn {
    // CHECK-LABEL: fn @s{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    string public s;

    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 1
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    bytes public b;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: sstore 0, 65
    // CHECK: [[S_DATA:v[0-9]+]] = keccak256 0, 32
    // CHECK: sstore [[S_DATA]], 0x6162636465666768696a6b6c6d6e6f707172737475767778797a414243444546
    // CHECK: sstore 1, 67
    // CHECK: [[B_DATA:v[0-9]+]] = keccak256 0, 32
    // CHECK: sstore [[B_DATA]], 0x102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
    // CHECK: sstore {{v[0-9]+}}, 0x2100000000000000000000000000000000000000000000000000000000000000
    constructor() {
        assembly {
            sstore(s.slot, 0x41)
            mstore(0x00, s.slot)
            let sData := keccak256(0x00, 0x20)
            sstore(sData, 0x6162636465666768696a6b6c6d6e6f707172737475767778797a414243444546)

            sstore(b.slot, 0x43)
            mstore(0x00, b.slot)
            let bData := keccak256(0x00, 0x20)
            sstore(bData, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
            sstore(add(bData, 1), 0x2100000000000000000000000000000000000000000000000000000000000000)
        }
    }

    // CHECK-LABEL: fn @getS{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function getS() public view returns (string memory) {
        return s;
    }

    // CHECK-LABEL: fn @getB{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 1
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function getB() public view returns (bytes memory) {
        return b;
    }
}
