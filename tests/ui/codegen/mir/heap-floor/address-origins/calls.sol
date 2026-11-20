//@ revisions: none gas size dump
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[dump] compile-flags: -O none -Zdump=mir-final
//@[dump] filecheck:
//@[dump] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: returned 0 => true
//@ run-call: passed 0 => true
//@ run-call: second 0 => true

// A contract whose assembly is all marked memory-safe raises a constant word stored into the free
// memory pointer slot to the heap floor, also when the word reaches the store as a call's result,
// as the second result of a call, or as an argument, so the allocation after it lands above the
// floor and not at the address the word names.
contract HeapFloorAddressOrigins {
    function word() internal pure returns (uint256) {
        return 0x20;
    }

    function pair() internal pure returns (uint256, uint256) {
        return (1, 0x20);
    }

    // CHECK-LABEL: fn @setPointer(
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt arg0, [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], arg0
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function setPointer(uint256 p) internal pure {
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
    }

    // CHECK-LABEL: fn @returned(
    // CHECK: [[WORD:v[0-9]+]] = icall @word
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt [[WORD]], [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], [[WORD]]
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function returned(uint256) external pure returns (bool above) {
        uint256 p = word();
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    function passed(uint256) external pure returns (bool above) {
        setPointer(0x20);
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    // CHECK-LABEL: fn @second(
    // CHECK: icall @pair
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt {{v[0-9]+}}, [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], {{v[0-9]+}}
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function second(uint256) external pure returns (bool above) {
        (, uint256 p) = pair();
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }
}
