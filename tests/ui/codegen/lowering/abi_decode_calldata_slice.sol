//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: {{v[0-9]+}} = extract_value calldataslice, arg0, 0
    // CHECK: {{v[0-9]+}} = extract_value calldataslice, arg0, 1
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: [[TAIL_LEN:v[0-9]+]] = extract_value calldataslice, [[TAIL]], 1
    // CHECK: [[BUF:v[0-9]+]] = alloc
    // CHECK-NEXT: store [[BUF]], {{v[0-9]+}}
    // CHECK-NEXT: [[DATA:v[0-9]+]] = ptradd [[BUF]], 32
    // CHECK-NEXT: [[SRC:v[0-9]+]] = extract_value calldataslice, [[TAIL]], 0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value calldataslice, [[TAIL]], 1
    // CHECK-NEXT: copy calldata [[DATA]], [[SRC]], [[LEN]]
    // CHECK: abi_decode [u256], [[BUF]]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
