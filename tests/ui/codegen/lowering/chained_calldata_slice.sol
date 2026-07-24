//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CHAIN

contract ChainedCalldataSlice {
    // A slice of a slice: the inner slice lowers to a calldata slice value,
    // and the outer slice re-slices it, staying lazy with the right byte
    // stride rather than the raw one-word fallback.
    function bytesChain(bytes calldata x) external pure returns (bytes memory) {
        return x[1:][1:];
    }

    // A word-strided array slice of a slice, consumed by indexing.
    function arrChain(uint256[] calldata a) external pure returns (uint256) {
        return a[1:][1:][0];
    }
}

// CHAIN-LABEL: fn @bytesChain
// CHAIN: make_calldata_slice
// CHAIN: make_calldata_slice
// CHAIN-LABEL: fn @arrChain
// CHAIN: make_calldata_slice
