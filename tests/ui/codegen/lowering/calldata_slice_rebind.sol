//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Rebinding calldata bytes keeps a lazy `(ptr, len)` slice. A later external
// call carries that slice into semantic ABI encoding without interpreting the
// original ABI head offset as a memory pointer. Late lowering emits the copy.
interface SliceSink {
    function consume(bytes calldata data) external;
}

contract CalldataSliceRebind {
    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: make_calldata_slice
    // CHECK-NOT: mcopy
    // CHECK: abi_encode [calldata_bytes]
    // CHECK: {{^.*[ =]call[[:space:]]}}
    function forward(bytes calldata data, uint256 start, SliceSink sink) external {
        data = data[start:];
        sink.consume(data);
    }
}
