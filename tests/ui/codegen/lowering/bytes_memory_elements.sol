//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Memory `bytes` uses the packed `[length][data...]` layout: `new bytes(n)`
// allocates 32 + pad32(n) zeroed bytes (not one word per byte), element reads
// extract single bytes left-aligned as `bytes1`, and element stores are
// single-byte `mstore8` writes at `data + i`.
contract BytesMemoryElements {
    // CHECK-LABEL: fn @alloc{{[( ]}}
    // CHECK: [[ALLOC_SIZE:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[ALLOC_SIZE]]
    // CHECK: set_memory_object_len memorybytes, [[BUF]], 96
    // CHECK: mstore8 {{v[0-9]+}}, 170
    // CHECK: mstore8 {{v[0-9]+}}, 255
    // CHECK: keccak256_bytes [[BUF]]
    function alloc() external pure returns (bytes32) {
        bytes memory buf = new bytes(96);
        buf[5] = 0xAA;
        buf[95] = hex"ff";
        return keccak256(buf);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[BUF]], 10
    // CHECK: mstore8 {{v[0-9]+}}, 170
    // CHECK: keccak256_bytes [[BUF]]
    function literal() external pure returns (bytes32) {
        bytes memory buf = hex"00010203040506070809";
        buf[5] = 0xAA;
        return keccak256(buf);
    }

    // CHECK-LABEL: fn @allocDynamic{{[( ]}}
    // CHECK: [[ALLOC_SIZE:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[ALLOC_SIZE]]
    // CHECK: set_memory_object_len memorybytes, [[BUF]], arg0
    function allocDynamic(uint n) external pure returns (uint) {
        bytes memory buf = new bytes(n);
        return buf.length;
    }

    // CHECK-LABEL: fn @readWrite{{[( ]}}
    // CHECK: [[BYTE:v[0-9]+]] = shr 248, arg2
    // CHECK: mstore8 {{v[0-9]+}}, [[BYTE]]
    // CHECK: [[LOADED:v[0-9]+]] = mload
    // CHECK: and [[LOADED]], 0xff00000000000000000000000000000000000000000000000000000000000000
    function readWrite(bytes memory b, uint i, bytes1 v) external pure returns (bytes1) {
        b[i] = v;
        return b[i];
    }
}
