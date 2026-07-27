//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

interface SliceSink {
    function consume(bytes calldata data) external;
}

contract CalldataSliceEncode {
    // CHECK-LABEL: fn @encode{{[( ]}}
    // CHECK: make_calldata_slice
    // CHECK: calldatacopy
    function encode(bytes calldata data, uint256 start)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(data[start:]);
    }

    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: make_calldata_slice
    // CHECK: abi_encode [calldata_bytes]
    // CHECK: {{^.*[ =]call[[:space:]]}}
    function forward(bytes calldata data, uint256 start, SliceSink sink) external {
        data = data[start:];
        sink.consume(data);
    }
}
