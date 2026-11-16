//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir
//@ filecheck:

// A `uint256[]` argument has no invalid element, so its decoder checks the array's range and
// emits no loop over the words. A `uint8[]` argument that ABI decoding copies to memory keeps its
// loop, which rejects elements above 255. A calldata `uint8[]` that is only returned has no loop
// either: the encoder validates the words it reads.
contract WordArray {
    // CHECK-LABEL: fn @words()
    // CHECK-NOT: phi
    // CHECK: ret
    function words(uint256[] calldata a) external pure returns (uint256[] calldata) {
        return a;
    }

    // CHECK-LABEL: fn @narrow()
    // CHECK: phi
    function narrow(uint8[] memory a) external pure returns (uint8[] memory) {
        return a;
    }
}
