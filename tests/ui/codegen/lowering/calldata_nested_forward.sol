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
    // CHECK: [[ARR_HEAD:v[0-9]+]] = ptrtoint memptr [[ARR]] to i256
    // CHECK: mstore [[ARR_HEAD]], {{v[0-9]+}}
    // CHECK-DAG: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK-DAG: [[BYTES_HEAD:v[0-9]+]] = ptrtoint memptr [[BYTES]] to i256
    // CHECK-DAG: mstore [[BYTES_HEAD]], {{v[0-9]+}}
    // CHECK-DAG: abi_encode [memory_array<tuple<word, memory_bytes>>]
    // CHECK: [[ARR_VIEW:v[0-9]+]] = memory_slice [[ARR]]
    // CHECK: slice_store_element [[ARR_VIEW]], {{v[0-9]+}}, {{v[0-9]+}}
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
