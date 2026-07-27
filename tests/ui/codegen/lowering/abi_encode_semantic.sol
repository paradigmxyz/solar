//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck: --implicit-check-not=set_fmp

interface Sink {
    function consume(uint256 value, bytes calldata data) external;
}

contract AbiEncodeSemantic {
    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: abi_encode [word, calldata_bytes], selector {{.*}}, args {{.*}}
    // CHECK: slice_ptr
    // CHECK: slice_len
    function forward(Sink sink, uint256 value, bytes calldata data) external {
        sink.consume(value, data);
    }
}
