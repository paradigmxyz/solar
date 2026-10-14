//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Memory `bytes` uses the packed `[length][data...]` layout: `new bytes(n)`
// allocates 32 + pad32(n) zeroed bytes (not one word per byte), element reads
// extract single bytes left-aligned as `bytes1`, and element stores are
// single-byte `mstore8` writes at `data + i`.
contract BytesMemoryElements {
    // CHECK-LABEL: fn @alloc{{[( ]}}
    // CHECK: [[MASK:v[0-9]+]] = not 31
    // CHECK: [[ALLOC_SIZE:v[0-9]+]] = and 159, [[MASK]]
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[ALLOC_SIZE]]
    // CHECK: store [[BUF]], 96
    // CHECK: store8 [[BUF]]+37, {{v[0-9]+}}
    // CHECK: store8 [[BUF]]+127, {{v[0-9]+}}
    // CHECK: [[ADDR:v[0-9]+]] = ptrtoint memptr [[BUF]] to i256
    // CHECK-NEXT: [[DATA:v[0-9]+]] = add [[ADDR]], 32
    // CHECK-NEXT: [[LEN:v[0-9]+]] = load [[BUF]]
    // CHECK-NEXT: keccak256 [[DATA]], [[LEN]]
    function alloc() external pure returns (bytes32) {
        bytes memory buf = new bytes(96);
        buf[5] = 0xAA;
        buf[95] = hex"ff";
        return keccak256(buf);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 64
    // CHECK: store [[BUF]], 10
    // CHECK: store8 [[BUF]]+37, {{v[0-9]+}}
    // CHECK: [[ADDR:v[0-9]+]] = ptrtoint memptr [[BUF]] to i256
    // CHECK-NEXT: [[DATA:v[0-9]+]] = add [[ADDR]], 32
    // CHECK-NEXT: [[LEN:v[0-9]+]] = load [[BUF]]
    // CHECK-NEXT: keccak256 [[DATA]], [[LEN]]
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
    // CHECK: store [[BUF]], arg0
    function allocDynamic(uint n) external pure returns (uint) {
        bytes memory buf = new bytes(n);
        return buf.length;
    }

    // CHECK-LABEL: fn @readWrite{{[( ]}}
    // CHECK: icall panic_if<0x32>
    // CHECK: [[BYTE:v[0-9]+]] = byte 0, arg2
    // CHECK-NEXT: [[STORE_PTR:v[0-9]+]] = ptradd arg0, arg1
    // CHECK-NEXT: store8 [[STORE_PTR]]+32, [[BYTE]]
    // CHECK: icall panic_if<0x32>
    // CHECK-NEXT: [[LOAD_PTR:v[0-9]+]] = ptradd arg0, arg1
    // CHECK-NEXT: [[WORD:v[0-9]+]] = load [[LOAD_PTR]]+32
    // CHECK-NEXT: {{v[0-9]+}} = byte 0, [[WORD]]
    function readWrite(bytes memory b, uint i, bytes1 v) external pure returns (bytes1) {
        b[i] = v;
        return b[i];
    }
}
