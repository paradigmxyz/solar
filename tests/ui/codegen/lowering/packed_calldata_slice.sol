//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=PACKED

contract PackedCalldataSlice {
    // A `base[low:high]` calldata bytes slice packs its data unpadded, copied
    // through the shared calldata-bytes materializer.
    // PACKED-LABEL: fn @slice{{[( ]}}
    // PACKED: calldatacopy
    function slice(bytes calldata x, uint256 a, uint256 b) external pure returns (bytes memory) {
        return abi.encodePacked(x[a:b], "!");
    }

    // `msg.data` packs the same way.
    // PACKED-LABEL: fn @all{{[( ]}}
    // PACKED: calldatacopy
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
