//@compile-flags: -O none -Zdump=mir
//@filecheck: --implicit-check-not=internal_call

contract StorageBytesPushPop {
    bytes data;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: sload 0
    // CHECK: sstore 0,
    // CHECK: sload 0
    // CHECK: sstore 0,
    constructor() {
        data.push(0x01);
        data.push(0x02);
    }

    // The push decodes the header slot and appends the byte in place: the short
    // case rewrites the header, crossing 32 bytes moves the data into the data
    // area, and a long value only bumps the header and writes one data word.
    // CHECK-LABEL: fn @pushValue{{[( ]}}
    // CHECK: sload 0
    // CHECK: gt [[LEN:v[0-9]+]], 31
    // CHECK: eq [[LEN]], 31
    // CHECK: byte 0, arg0
    // CHECK: sstore 0,
    // CHECK: storage_array_data_slot 0
    // CHECK: sstore 0, 65
    // CHECK: storage_array_data_slot 0
    // CHECK: sload
    // CHECK: sstore
    function pushValue(bytes1 value) external {
        data.push(value);
    }

    // CHECK-LABEL: fn @pushZero{{[( ]}}
    // CHECK: sload 0
    // CHECK: gt [[LEN:v[0-9]+]], 31
    // CHECK: eq [[LEN]], 31
    // CHECK: sstore 0,
    // CHECK: storage_array_data_slot 0
    // CHECK: sstore 0,
    // CHECK: storage_array_data_slot 0
    function pushZero() external {
        data.push();
    }

    // The pop rewrites the header, clears the popped byte of a long value, and
    // moves the data back into the header when the value shrinks to 31 bytes.
    // CHECK-LABEL: fn @popValue{{[( ]}}
    // CHECK: sload 0
    // CHECK: eq [[LEN:v[0-9]+]], 0
    // CHECK: eq [[LEN]], 32
    // CHECK: storage_array_data_slot 0
    // CHECK: sstore
    // CHECK: sstore 0,
    // CHECK: storage_array_data_slot 0
    // CHECK: sstore {{v[0-9]+}}, 0
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
