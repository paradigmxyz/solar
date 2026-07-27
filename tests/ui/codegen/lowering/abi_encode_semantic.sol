//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=ABI --implicit-check-not=set_fmp

interface Sink {
    function consume(uint256 value, bytes calldata data) external;
}

contract AbiEncodeSemantic {
    // ABI-LABEL: fn @forward{{[( ]}}
    // ABI: abi_encode [word, calldata_bytes], selector {{.*}}, args {{.*}}
    // ABI: slice_ptr
    // ABI: slice_len
    function forward(Sink sink, uint256 value, bytes calldata data) external {
        sink.consume(value, data);
    }
}
