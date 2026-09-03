//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: makeBytes 0 => 0
//@ run-call: makeBytes 31 => 31
//@ run-call: makeArray 0 => 0
//@ run-call: makeArray 1 => 1
//@ run-call: makeNestedArray 0 => 0
//@ run-call: makeNestedArray 2 => 2
//@ run-call: makeStructArray 0 => 0
//@ run-call: makeStructArray 1 => 1
//@ run-call-fail: makeBytes 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x41)
//@ run-call-fail: makeArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x41)
//@ run-call-fail: makeNestedArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x41)
//@ run-call-fail: makeStructArray 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x41)
//@ run-call-fail: makeArrayTooLarge => Panic(0x41)

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

    function makeArrayTooLarge() external pure returns (uint256) {
        uint256 n = 2**256 / 32;
        uint256[] memory a = new uint256[](n);
        a[1] = 42;
        return a[1];
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
