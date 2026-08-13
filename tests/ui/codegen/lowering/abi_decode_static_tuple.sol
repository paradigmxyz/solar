//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDecodeStaticTuple {
    // CHECK-LABEL: fn @decode{{[( ]}}
    // CHECK: abi_decode [u256, bool, address], arg0
    // CHECK: ret
    function decode(bytes memory data) external pure returns (uint256 a, bool b, address c) {
        return abi.decode(data, (uint256, bool, address));
    }
}
