//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Memory `bytes` uses the packed `[length][data...]` layout: `new bytes(n)`
// allocates 32 + pad32(n) zeroed bytes (not one word per byte), element reads
// extract single bytes left-aligned as `bytes1`, and element stores are
// single-byte `mstore8` writes at `data + i`.
contract BytesMemoryElements {
    // CHECK-LABEL: fn @alloc{{[( ]}}
    // CHECK: [[PADDED:v[0-9]+]] = add 96, 63
    // CHECK: {{v[0-9]+}} = lt [[PADDED]], 96
    // CHECK: [[MASK:v[0-9]+]] = not 31
    // CHECK: [[ALLOC_SIZE:v[0-9]+]] = and [[PADDED]], [[MASK]]
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[ALLOC_SIZE]]
    // CHECK: set_memory_object_len memorybytes, [[BUF]], 96
    // CHECK: memory_object_store_byte memorybytes, {{.*}}, {{.*}}, {{.*}}
    // CHECK: memory_object_store_byte memorybytes, {{.*}}, {{.*}}, {{.*}}
    // CHECK: keccak256_bytes [[BUF]]
    function alloc() external pure returns (bytes32) {
        bytes memory buf = new bytes(96);
        buf[5] = 0xAA;
        buf[95] = hex"ff";
        return keccak256(buf);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 64
    // CHECK: set_memory_object_len memorybytes, [[BUF]], 10
    // CHECK: memory_object_store_byte memorybytes, {{.*}}, {{.*}}, {{.*}}
    // CHECK: keccak256_bytes [[BUF]]
    function literal() external pure returns (bytes32) {
        bytes memory buf = hex"00010203040506070809";
        buf[5] = 0xAA;
        return keccak256(buf);
    }

    // CHECK-LABEL: fn @allocDynamic{{[( ]}}
    // CHECK: [[PADDED:v[0-9]+]] = add arg0, 63
    // CHECK: {{v[0-9]+}} = lt [[PADDED]], arg0
    // CHECK: [[MASK:v[0-9]+]] = not 31
    // CHECK: [[ALLOC_SIZE:v[0-9]+]] = and [[PADDED]], [[MASK]]
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[ALLOC_SIZE]]
    // CHECK: set_memory_object_len memorybytes, [[BUF]], arg0
    function allocDynamic(uint n) external pure returns (uint) {
        bytes memory buf = new bytes(n);
        return buf.length;
    }

    // CHECK-LABEL: fn @readWrite{{[( ]}}
    // CHECK: memory_object_store_byte memorybytes, arg0, arg1, {{v[0-9]+}}
    // CHECK: memory_object_load_byte memorybytes, arg0, arg1
    function readWrite(bytes memory b, uint i, bytes1 v) external pure returns (bytes1) {
        b[i] = v;
        return b[i];
    }
}
