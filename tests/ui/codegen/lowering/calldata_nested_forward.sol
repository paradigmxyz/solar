//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=NESTED

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
    // NESTED-LABEL: fn @forward{{[( ]}}
    // NESTED: abi_encode [memory_array<memory_bytes>]
    function forward(bytes[] calldata data, BytesSink sink) external {
        sink.consume(data);
    }

    // NESTED-LABEL: fn @forwardStructs{{[( ]}}
    // NESTED: abi_encode [memory_array<tuple<word, memory_bytes>>]
    function forwardStructs(NestedItem[] calldata data, StructSink sink) external {
        sink.consume(data);
    }
}
