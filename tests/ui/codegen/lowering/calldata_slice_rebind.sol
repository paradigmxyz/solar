//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SLICE

// Rebinding calldata bytes keeps a lazy `(ptr, len)` slice. A later external
// call carries that slice into semantic ABI encoding without interpreting the
// original ABI head offset as a memory pointer. Late lowering emits the copy.
interface SliceSink {
    function consume(bytes calldata data) external;
}

contract CalldataSliceRebind {
    // SLICE-LABEL: fn @forward{{[( ]}}
    // SLICE: make_calldata_slice
    // SLICE-NOT: mcopy
    // SLICE: abi_encode [calldata_bytes]
    // SLICE: call
    function forward(bytes calldata data, uint256 start, SliceSink sink) external {
        data = data[start:];
        sink.consume(data);
    }
}
