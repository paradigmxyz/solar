//@ compile-flags: -Osize -Zdump=mir
//@ filecheck:
//@ run-call: first 7, 0x0102 => 0x0000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000020102000000000000000000000000000000000000000000000000000000000000
//@ run-call: second 9, 0x => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000

// Two sites encode a copy of a `bytes calldata` argument. Both encode from the slice, so they
// pass the same argument types and share one `encode_abi_tuple` helper, which holds the only
// `calldatacopy`.
// CHECK-LABEL: fn @first
// CHECK-NOT: calldatacopy
// CHECK: icall @encode_abi_tuple
// CHECK-LABEL: fn @second
// CHECK-NOT: calldatacopy
// CHECK: icall @encode_abi_tuple
// CHECK-LABEL: fn @encode_abi_tuple
// CHECK: calldatacopy
contract EncodeCalldataCopyShared {
    function first(uint256 x, bytes calldata data) external pure returns (bytes memory) {
        bytes memory copy = data;
        return abi.encode(x, copy);
    }

    function second(uint256 x, bytes calldata data) external pure returns (bytes memory) {
        bytes memory copy = data;
        return abi.encode(x, copy);
    }
}
