//@compile-flags: -Zcodegen -O none -Zdump=mir
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
    // A calldata array of reference elements re-encodes through a memory
    // rebuild: each element materializes as a memory pointer, and the encode
    // layout keeps the dynamic element type instead of collapsing it to one
    // word.
    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: abi_encode [memory_array<memory_bytes>]
    function forward(bytes[] calldata data, BytesSink sink) external {
        sink.consume(data);
    }

    // CHECK-LABEL: fn @forwardStructs{{[( ]}}
    // CHECK: abi_encode [memory_array<tuple<word, memory_bytes>>]
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
