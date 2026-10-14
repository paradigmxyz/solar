//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: {{v[0-9]+}} = slice_ptr arg0
    // CHECK: {{v[0-9]+}} = slice_len arg0
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: [[TAIL_LEN:v[0-9]+]] = slice_len [[TAIL]]
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes
    // CHECK-NEXT: store [[BUF]], {{v[0-9]+}}
    // CHECK-NEXT: [[DATA:v[0-9]+]] = ptradd [[BUF]], 32
    // CHECK-NEXT: [[SRC:v[0-9]+]] = slice_ptr [[TAIL]]
    // CHECK-NEXT: [[LEN:v[0-9]+]] = slice_len [[TAIL]]
    // CHECK-NEXT: copy calldata [[DATA]], [[SRC]], [[LEN]]
    // CHECK: abi_decode [u256], [[BUF]]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
