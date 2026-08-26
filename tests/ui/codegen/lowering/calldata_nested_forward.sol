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
    // CHECK: set_memory_object_len memoryarray
    // CHECK-DAG: set_memory_object_len memorybytes
    // CHECK-DAG: abi_encode [memory_array<tuple<word, memory_bytes>>]
    // CHECK: memory_object_store_element memoryarray
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
