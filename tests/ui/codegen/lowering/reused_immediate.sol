//@ revisions: size gas
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck:
//@[gas] compile-flags: -O gas
//@ run-call: swapBytes 258 => 513
//@ run-call: swapBytes 2864434397 => 3148537292

contract ReusedImmediate {
    // The byte mask's shortest push, `push 257; push 0; not; div`, takes six
    // bytes. Size builds keep the copy the first `and` consumes when the
    // second `and` follows within a few instructions, and duplicate it.
    // CHECK-LABEL: @module ReusedImmediate_runtime
    // CHECK: push 257
    // CHECK-NOT: push 257
    // CHECK: dup
    // CHECK-NOT: push 257
    // CHECK: return
    function swapBytes(uint256 x) external pure returns (uint256) {
        uint256 mask = type(uint256).max / 257;
        return (mask & (x >> 8)) | ((mask & x) << 8);
    }
}
