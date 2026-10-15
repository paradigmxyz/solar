//@compile-flags: -O none -Zdump=mir
//@filecheck: --implicit-check-not=set_fmp

interface Sink {
    function consume(uint256 value, bytes calldata data) external;
    function consumeNarrow(uint8[] calldata data) external;
}

contract AbiEncodeSemantic {
    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: abi_encode [word, calldata_bytes], selector {{.*}}, args {{.*}}
    // CHECK: extract_value slice<memory>, {{[^,]+}}, 0
    // CHECK: extract_value slice<memory>, {{[^,]+}}, 1
    function forward(Sink sink, uint256 value, bytes calldata data) external {
        sink.consume(value, data);
    }

    // A validated narrow calldata array stays in calldata through ABI encoding.
    // CHECK-LABEL: fn @forwardNarrow{{[( ]}}
    // CHECK: abi_encode [calldata_array<word<u8>>], selector {{.*}}, args {{.*}}
    function forwardNarrow(Sink sink, uint8[] calldata data) external {
        sink.consumeNarrow(data);
    }
}
