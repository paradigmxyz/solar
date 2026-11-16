//@ codegen-matrix: standard
//@ run-call-fail: hashed 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: plain 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: guarded false => 2
//@ run-call-fail: guarded true => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A constant index far past the end of a memory array makes its element
// address a heap pointer minus about 29 GB. The bounds check reverts before the
// access, so codegen must not keep that much room below the heap: doing so
// starts the free memory pointer there, and the first allocation of every call
// runs out of gas instead of panicking or returning.
contract HeapPrefixUnreachableStep {
    function hashed(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~(uint256(0x6c79) << 15)] ^= uint32(uint256(keccak256(abi.encode(x))));
        return a[0];
    }

    function plain(uint256 x) external pure returns (uint32) {
        uint32[] memory a = new uint32[](2);
        a[~(uint256(0x6c79) << 15)] ^= uint32(x);
        return a[0];
    }

    function guarded(bool outOfBounds) external pure returns (uint256) {
        uint32[] memory a = new uint32[](2);
        if (outOfBounds) {
            a[~(uint256(0x6c79) << 15)] = 1;
        }
        return a.length;
    }
}
