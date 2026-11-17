//@ codegen-matrix: standard native
//@[native] compile-flags: -Ogas -Zdump=mir-final
//@[native] filecheck: --check-prefix=NATIVE
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[native] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: f 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// NATIVE: backend-entry {{[0-9]+}} 0xa0

// Assembly steps back one word from the array, to the address the checked index `~1` reaches.
// Both computations are one value after value numbering, which a checked offset may not stay: the
// guard must still keep a word of room below the heap for the assembly store.
contract HeapPrefixUncheckedMerge {
    function f(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~uint256(1)] ^= uint32(uint256(keccak256(abi.encode(x))));
        assembly {
            mstore(sub(a, 32), x)
        }
        return a[0];
    }
}
