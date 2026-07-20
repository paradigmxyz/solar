//@compile-flags: -Zcodegen --emit=mir
//@filecheck: --check-prefix=ABI

interface Sink {
    function consume(uint256 value, bytes calldata data) external;
}

contract AbiEncodeSemantic {
    function forward(Sink sink, uint256 value, bytes calldata data) external {
        sink.consume(value, data);
    }
}

// ABI-LABEL: fn @forward
// ABI: abi_encode [word, calldata_bytes], selector {{.*}}, args {{.*}}
// ABI: slice_ptr
// ABI: slice_len
// ABI-NOT: set_fmp
