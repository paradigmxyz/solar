//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
// CHECK: @module
//@ run-call: allocate(uint256) 0x1000 => 1
//@ run-call-fail: allocate(uint256) 0xffffffffffffffc0 => Panic(0x41)
//@ run-call-fail: allocate(uint256) 0xffffffffffffffff => Panic(0x41)
//@ run-call-fail: allocate(uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0 => Panic(0x41)
//@ run-call-fail: allocate(uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x41)

// A constant-size allocation must reject both a large end and a wrapped end after assembly
// replaces the free-memory pointer. Keeping the array live makes the checked allocation observable.
contract ConstantAllocationPanic {
    function allocate(uint256 pointer) external pure returns (uint256) {
        assembly { mstore(0x40, pointer) }
        uint256[] memory words = new uint256[](1);
        words[0] = 1;
        return words[0];
    }
}
