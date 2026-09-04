//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesPushPop {
    bytes data;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes
    // CHECK: icall @store_storage_bytes, 0, 0,
    // CHECK: memory_object_copy memorybytes
    // CHECK: icall @store_storage_bytes, 0, 0,
    constructor() {
        data.push(0x01);
        data.push(0x02);
    }

    // CHECK-LABEL: fn @pushValue{{[( ]}}
    // CHECK: [[BYTE:v[0-9]+]] = shr 248, arg0
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes
    // CHECK: memory_object_store_byte memorybytes
    // CHECK: icall @store_storage_bytes, 0, 0,
    function pushValue(bytes1 value) external {
        data.push(value);
    }

    // CHECK-LABEL: fn @pushZero{{[( ]}}
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes
    // CHECK: memory_object_store_byte memorybytes
    // CHECK: icall @store_storage_bytes, 0, 0,
    function pushZero() external {
        data.push();
    }

    // CHECK-LABEL: fn @popValue{{[( ]}}
    // CHECK: memory_object_len memorybytes
    // CHECK: icall @store_storage_bytes, 0, 0,
    function popValue() external {
        data.pop();
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: storage_array_data_slot 0
    // CHECK: ret {{v[0-9]+}}
    function get() external view returns (bytes memory) {
        return data;
    }
}
