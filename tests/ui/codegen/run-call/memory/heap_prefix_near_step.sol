//@ codegen-matrix: standard native
//@[native] compile-flags: -Ogas -Zdump=mir-final
//@[native] filecheck: --check-prefix=NATIVE
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[native] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: f 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Assembly steps back one word from the array, and the checked index `~1` reaches the same
// address. The step is within the start of the heap, so the guard keeps a word of room below it.
// NATIVE: backend-entry {{[0-9]+}} 0xa0
contract HeapPrefixNearStep {
    function f(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~uint256(1)] ^= uint32(uint256(keccak256(abi.encode(x))));
        assembly {
            mstore(sub(a, 32), x)
        }
        return a[0];
    }
}
