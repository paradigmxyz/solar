//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // CHECK-LABEL: fn @decode
    // CHECK: {{v[0-9]+}} = slice_ptr arg0
    // CHECK: {{v[0-9]+}} = slice_len arg0
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: [[TAIL_LEN:v[0-9]+]] = slice_len [[TAIL]]
    // CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, [[TAIL_LEN]]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
