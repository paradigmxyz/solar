//@ run-call: makeBytes 0 => 0
//@ run-call: makeBytes 31 => 31
//@ run-call: makeArray 0 => 0
//@ run-call: makeArray 1 => 1
//@ run-call-fail: makeBytes 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: makeArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

contract MemoryAllocationPanicRuntime {
    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
