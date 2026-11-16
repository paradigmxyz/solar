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

    // A calldata array of structs with dynamic members stays in calldata too.
    // CHECK-LABEL: fn @forwardStructs{{[( ]}}
    // CHECK: abi_encode [calldata_array<tuple<word, calldata_bytes>>]
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
