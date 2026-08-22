//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesPushPop {
    bytes data;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: sload 0
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes, {{v[0-9]+}}, memorybytes, {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sload 0
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes, {{v[0-9]+}}, memorybytes, {{v[0-9]+}}, {{v[0-9]+}}
    constructor() {
        data.push(0x01);
        data.push(0x02);
    }

    // CHECK-LABEL: fn @pushValue{{[( ]}}
    // CHECK: sload 0
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes, {{v[0-9]+}}, memorybytes, {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: memory_object_store_byte memorybytes
    function pushValue(bytes1 value) external {
        data.push(value);
    }

    // CHECK-LABEL: fn @pushZero{{[( ]}}
    // CHECK: sload 0
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_object_copy memorybytes, {{v[0-9]+}}, memorybytes, {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: memory_object_store_byte memorybytes
    function pushZero() external {
        data.push();
    }

    // CHECK-LABEL: fn @popValue{{[( ]}}
    // CHECK: sload 0
    // CHECK: memory_object_len memorybytes
    // CHECK: sub
    // CHECK: memory_object_copy memorybytes, {{v[0-9]+}}, memorybytes, {{v[0-9]+}}, {{v[0-9]+}}
    function popValue() external {
        data.pop();
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: sload 0
    // CHECK: ret
    function get() external view returns (bytes memory) {
        return data;
    }
}
