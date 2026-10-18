//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeCalldataSlice {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: {{v[0-9]+}} = extract_value slice<calldata>, arg0, 0
    // CHECK: {{v[0-9]+}} = extract_value slice<calldata>, arg0, 1
    // CHECK: [[TAIL:v[0-9]+]] = make_calldata_slice {{v[0-9]+}}, {{v[0-9]+}}
    // The slice decodes in place, with no copy into memory first.
    // CHECK-NOT: calldatacopy
    // CHECK: abi_decode [u256], [[TAIL]]
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
