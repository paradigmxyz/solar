//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: loop 0 => 0
//@ run-call-fail: loop 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A loop index added to a constant far past the end of a memory array reaches an element address
// 28 MB below the array. The step back is longer than the start of the heap, so the guard
// reserves nothing for it, as for a constant index. Reserving it started the free memory pointer
// there, and every call ran out of gas instead of returning or panicking.
contract HeapPrefixLoopIndex {
    function loop(uint256 n) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](2);
        for (uint256 j; j < n; j++) {
            a[~(uint256(0x6c79) << 5) + j] ^= uint256(keccak256(abi.encode(j)));
        }
        return a[0];
    }
}
