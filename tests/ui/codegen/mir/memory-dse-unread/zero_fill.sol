//@ revisions: standard unread run
//@[standard] compile-flags: -Zmir-pipeline=memory-dse -Zdump=mir
//@[standard] filecheck: --check-prefix=STANDARD
//@[unread] compile-flags: -Zmir-pipeline=memory-dse-unread -Zdump=mir
//@[unread] filecheck: --check-prefix=UNREAD
//@[run] compile-flags: -Zmir-pipeline=memory-dse-unread,lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: writeOnly 5 => 5
//@[run] run-call: indexedRead 7, 0 => 7
//@[run] run-call: indexedRead 7, 1 => 0
//@[run] run-call-fail: indexedRead 7, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[run] run-call: escaped 9 => 0x00000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000000
// Only `memory-dse-unread` removes the zero fill of an allocation nothing reads. A read of an
// element, or the allocation escaping into `abi.encode`, keeps it.

contract UnreadAllocations {
    // STANDARD-LABEL: fn @writeOnly(
    // STANDARD: memory_zero
    // UNREAD-LABEL: fn @writeOnly(
    // UNREAD-NOT: memory_zero
    function writeOnly(uint256 x) public pure returns (uint256) {
        uint256[3] memory a;
        a[0] = x;
        a[1] = x + 1;
        return x;
    }

    // STANDARD-LABEL: fn @indexedRead(
    // STANDARD: memory_zero
    // UNREAD-LABEL: fn @indexedRead(
    // UNREAD: memory_zero
    function indexedRead(uint256 x, uint256 i) public pure returns (uint256) {
        uint256[3] memory a;
        a[0] = x;
        return a[i];
    }

    // STANDARD-LABEL: fn @escaped(
    // STANDARD: memory_zero
    // UNREAD-LABEL: fn @escaped(
    // UNREAD: memory_zero
    function escaped(uint256 x) public pure returns (bytes memory) {
        uint256[2] memory a;
        a[0] = x;
        return abi.encode(a);
    }
}
