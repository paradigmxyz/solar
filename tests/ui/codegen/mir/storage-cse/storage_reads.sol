//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=storage-cse -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=storage-cse,lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: reuse; constructor=[7] => 14
//@[run] run-call: aliased 3, 3; constructor=[7] => 9
//@[run] run-call: aliased 3, 5; constructor=[7] => 8
//@[run] run-call: forward; constructor=[7] => 18
//@[run] run-call: other; constructor=[7] => 7
// A repeated `sload` of a slot no store can have changed reuses the first value, and a read
// after a store of the same slot takes the stored word. A store through an unrelated symbolic
// slot may alias, so the read after it stays.

contract StorageCse {
    uint256 a;
    uint256 b;
    mapping(uint256 => uint256) m;

    constructor(uint256 v) {
        a = v;
        m[3] = 4;
    }

    // The store to slot 1 cannot change slot 0.
    // CHECK-LABEL: fn @reuse(
    // CHECK: [[A:v[0-9]+]] = sload 0
    // CHECK-NOT: sload
    // CHECK: checked_add uint256, [[A]], [[A]]
    function reuse() public returns (uint256) {
        uint256 x = a;
        b = 5;
        return x + a;
    }

    // A store to `m[j]` may hit `m[k]`.
    // CHECK-LABEL: fn @aliased(
    // CHECK: sload
    // CHECK: sstore
    // CHECK: sload
    function aliased(uint256 k, uint256 j) public returns (uint256) {
        uint256 x = m[k];
        m[j] = 5;
        return x + m[k];
    }

    // CHECK-LABEL: fn @forward(
    // CHECK-NOT: sload
    // CHECK: checked_add uint256, 9, 9
    function forward() public returns (uint256) {
        a = 9;
        return a + a;
    }

    // CHECK-LABEL: fn @other(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[A:v[0-9]+]] = sload 0
    // CHECK-NEXT: ret [[A]]
    function other() public view returns (uint256) {
        return a;
    }
}
