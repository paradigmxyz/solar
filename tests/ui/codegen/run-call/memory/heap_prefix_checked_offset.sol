//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: hashed 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: plain 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: guarded false => 2
//@ run-call-fail: guarded true => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A constant index far past the end of a memory array wraps to an element address 28 MB below
// the array, within the 4 GiB that `heap_prefix_unreachable_step` covers. The bounds check
// reverts before the access, so the compiler marks the address as a checked object offset and the
// heap prefix guard must not read its wrapped constant as a step back that reserves room below the
// heap: doing so starts the free memory pointer there, and the first allocation of every call runs
// out of gas instead of panicking or returning.
contract HeapPrefixCheckedOffset {
    function hashed(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~(uint256(0x6c79) << 5)] ^= uint32(uint256(keccak256(abi.encode(x))));
        return a[0];
    }

    function plain(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~(uint256(0x6c79) << 5)] ^= uint32(x);
        return a[0];
    }

    function guarded(bool outOfBounds) external pure returns (uint256) {
        uint32[] memory a = new uint32[](2);
        if (outOfBounds) {
            a[~(uint256(0x6c79) << 5)] = 1;
        }
        return a.length;
    }
}
