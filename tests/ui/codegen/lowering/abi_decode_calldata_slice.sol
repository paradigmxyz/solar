//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: {{v[0-9]+}} = slice_ptr arg0
    // CHECK: {{v[0-9]+}} = slice_len arg0
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: [[TAIL_LEN:v[0-9]+]] = slice_len [[TAIL]]
    // CHECK: memory_object_copy_from_slice memorybytes, {{v[0-9]+}}, [[TAIL]]
    // CHECK: abi_decode [u256]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
