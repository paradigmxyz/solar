//@compile-flags: -O none -Zdump=mir
//@filecheck:

struct NestedItem {
    uint256 id;
    bytes payload;
}

interface BytesSink {
    function consume(bytes[] calldata data) external;
}

interface StructSink {
    function consume(NestedItem[] calldata data) external;
}

contract NestedCalldataForward {
    // A calldata array of bytes stays in calldata while the encoder walks its
    // offsets and copies each element into the outgoing ABI payload.
    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: abi_encode [calldata_array<calldata_bytes>]
    function forward(bytes[] calldata data, BytesSink sink) external {
        sink.consume(data);
    }

    // CHECK-LABEL: fn @forwardStructs{{[( ]}}
    // CHECK: [[ARR:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: store [[ARR]], {{v[0-9]+}}
    // CHECK-DAG: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK-DAG: store [[BYTES]], {{v[0-9]+}}
    // CHECK-DAG: abi_encode [memory_array<tuple<word, memory_bytes>>]
    // CHECK: [[OFF1:v[0-9]+]] = shl 5, {{v[0-9]+}}
    // CHECK-NEXT: [[ELEM:v[0-9]+]] = ptradd [[ARR]], [[OFF1]]
    // CHECK-NEXT: store [[ELEM]]+32, {{v[0-9]+}}
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
