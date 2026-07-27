//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract PackedCalldataSlice {
    // A `base[low:high]` calldata bytes slice packs its data unpadded, copied
    // through the shared calldata-bytes materializer.
    // CHECK-LABEL: fn @slice{{[( ]}}
    // CHECK: calldatacopy
    function slice(bytes calldata x, uint256 a, uint256 b) external pure returns (bytes memory) {
        return abi.encodePacked(x[a:b], "!");
    }

    // `msg.data` packs the same way.
    // CHECK-LABEL: fn @all{{[( ]}}
    // CHECK: calldatacopy
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
