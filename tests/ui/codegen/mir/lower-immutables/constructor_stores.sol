//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=lower-immutables -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-immutables,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: get; constructor=[41] => 42
//@[run] run-call: getb; constructor=[0] => -3
// Immutable assignments in the constructor become stores to its staging memory. Reads in
// runtime code stay `loadimmutable`.

contract LowerImmutablesSol {
    uint256 immutable a;
    int8 immutable b;

    // CHECK-LABEL: fn @constructor(
    // CHECK-NOT: storeimmutable
    // CHECK: mstore 192, {{v[0-9]+}}
    // CHECK-NEXT: mstore 224, 253
    constructor(uint256 x) {
        a = x + 1;
        b = -3;
    }

    // CHECK-LABEL: fn @get(
    // CHECK: loadimmutable a
    function get() public view returns (uint256) {
        return a;
    }

    // CHECK-LABEL: fn @getb(
    // CHECK: loadimmutable b
    function getb() public view returns (int256) {
        return b;
    }
}
