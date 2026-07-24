//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SLICE

interface SliceSink {
    function consume(bytes calldata data) external;
}

contract CalldataSliceEncode {
    // SLICE-LABEL: fn @encode
    // SLICE: make_calldata_slice
    // SLICE: calldatacopy
    function encode(bytes calldata data, uint256 start)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(data[start:]);
    }

    // SLICE-LABEL: fn @forward
    // SLICE: make_calldata_slice
    // SLICE: abi_encode [calldata_bytes]
    // SLICE: call
    function forward(bytes calldata data, uint256 start, SliceSink sink) external {
        data = data[start:];
        sink.consume(data);
    }
}
