//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract ChainedCalldataSlice {
    // A slice of a slice: the inner slice lowers to a calldata slice value,
    // and the outer slice re-slices it, staying lazy with the right byte
    // stride rather than the raw one-word fallback.
    // CHECK-LABEL: fn @bytesChain{{[( ]}}
    // CHECK: make_calldata_slice
    // CHECK: make_calldata_slice
    function bytesChain(bytes calldata x) external pure returns (bytes memory) {
        return x[1:][1:];
    }

    // A word-strided array slice of a slice, consumed by indexing.
    // CHECK-LABEL: fn @arrChain{{[( ]}}
    // CHECK: make_calldata_slice
    // CHECK: make_calldata_slice
    function arrChain(uint256[] calldata a) external pure returns (uint256) {
        return a[1:][1:][0];
    }
}
