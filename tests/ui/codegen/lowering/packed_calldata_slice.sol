//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract PackedCalldataSlice {
    // A `base[low:high]` calldata bytes slice packs its data unpadded, copied
    // through the shared calldata-bytes materializer.
    // CHECK-LABEL: fn @slice{{[( ]}}
    // CHECK: abi_encode_packed (bytes {{v[0-9]+}}
    function slice(bytes calldata x, uint256 a, uint256 b) external pure returns (bytes memory) {
        return abi.encodePacked(x[a:b], "!");
    }

    // `msg.data` packs the same way.
    // CHECK-LABEL: fn @all{{[( ]}}
    // CHECK: abi_encode_packed (bytes {{v[0-9]+}}
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
