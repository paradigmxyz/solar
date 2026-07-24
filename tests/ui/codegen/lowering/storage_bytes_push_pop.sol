//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract StorageBytesPushPop {
    bytes data;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: [[FIRST:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: [[FIRST_LEN:v[0-9]+]] = memory_object_len memorybytes, [[FIRST]]
    // CHECK: mcopy
    // CHECK: [[SECOND:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: [[SECOND_LEN:v[0-9]+]] = memory_object_len memorybytes, [[SECOND]]
    // CHECK: mcopy
    constructor() {
        data.push(0x01);
        data.push(0x02);
    }

    // CHECK-LABEL: fn @pushValue
    // CHECK: [[OLD:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: [[OLD_LEN:v[0-9]+]] = memory_object_len memorybytes, [[OLD]]
    // CHECK: mcopy
    // CHECK: [[BYTE:v[0-9]+]] = shr 248, arg0
    // CHECK: mstore8 {{v[0-9]+}}, [[BYTE]]
    function pushValue(bytes1 value) external {
        data.push(value);
    }

    // CHECK-LABEL: fn @pushZero
    // CHECK: [[OLD:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: [[OLD_LEN:v[0-9]+]] = memory_object_len memorybytes, [[OLD]]
    // CHECK: mcopy
    // CHECK: mstore8 {{v[0-9]+}}, 0
    function pushZero() external {
        data.push();
    }

    // CHECK-LABEL: fn @popValue
    // CHECK: [[OLD:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: [[OLD_LEN:v[0-9]+]] = memory_object_len memorybytes, [[OLD]]
    // CHECK: mstore 4, 49
    // CHECK: [[NEW_LEN:v[0-9]+]] = sub [[OLD_LEN]], 1
    // CHECK: mcopy
    function popValue() external {
        data.pop();
    }

    // CHECK-LABEL: fn @get
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function get() external view returns (bytes memory) {
        return data;
    }
}
