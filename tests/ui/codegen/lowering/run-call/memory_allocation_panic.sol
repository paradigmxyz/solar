//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: makeBytes 0 => 0
//@[none, gas, size] run-call: makeBytes 31 => 31
//@[none, gas, size] run-call: makeArray 0 => 0
//@[none, gas, size] run-call: makeArray 1 => 1
//@[none, gas, size] run-call: makeNestedArray 0 => 0
//@[none, gas, size] run-call: makeNestedArray 2 => 2
//@[none, gas, size] run-call: makeStructArray 0 => 0
//@[none, gas, size] run-call: makeStructArray 1 => 1
//@[none, gas, size] run-call-fail: makeBytes 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@[none, gas, size] run-call-fail: makeArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@[none, gas, size] run-call-fail: makeNestedArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@[none, gas, size] run-call-fail: makeStructArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

contract MemoryAllocationPanicRuntime {
    struct Pair {
        uint256 value;
        bytes data;
    }

    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }

    function makeNestedArray(uint256 n) external pure returns (uint256) {
        uint256[][] memory a = new uint256[][](1);
        a[0] = new uint256[](n);
        return a[0].length;
    }

    function makeStructArray(uint256 n) external pure returns (uint256) {
        Pair[] memory a = new Pair[](n);
        return a.length;
    }
}
