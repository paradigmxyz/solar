//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // A calldata range is decoded where it lies, as solc decodes it, with no
    // copy to memory first.
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: {{v[0-9]+}} = slice_ptr arg0
    // CHECK: {{v[0-9]+}} = slice_len arg0
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NOT: memory_object_copy_from_slice
    // CHECK: abi_decode [u256], [[TAIL]]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
