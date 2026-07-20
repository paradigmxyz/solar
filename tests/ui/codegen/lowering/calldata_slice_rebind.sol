//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SLICE

// Rebinding calldata bytes to a slice materializes a memory `[length][data]`
// value. A later external-call encoder must not interpret the original ABI
// head offset as a memory pointer.
interface SliceSink {
    function consume(bytes calldata data) external;
}

contract CalldataSliceRebind {
    function forward(bytes calldata data, uint256 start, SliceSink sink) external {
        data = data[start:];
        sink.consume(data);
    }
}

// SLICE-LABEL: fn @forward
// SLICE: calldatacopy
// SLICE: mcopy
// SLICE: call
