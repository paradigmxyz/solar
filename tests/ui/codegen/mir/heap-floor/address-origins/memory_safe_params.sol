//@ revisions: none gas size dump
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[dump] compile-flags: -O none -Zdump=mir-final
//@[dump] filecheck:
//@[dump] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: direct 0 => true
//@ run-call: direct 1 => true
//@ run-call: viaHelper 0 => true
//@ run-call: viaHelper 1 => true
//@ run-call: offset [1, 2] => true
//@ run-call: length [] => true
//@ run-call: reslice [1, 2] => true

// A contract whose assembly is all marked memory-safe still raises a constant or calldata word
// stored into the free memory pointer slot to the heap floor. An external function's integer
// parameter is such a word, and so are the offset and length of a calldata array parameter or of a slice of it, stored
// directly or passed to a helper that stores it, so the allocation after it lands above the floor
// and not at the address the argument names.
contract HeapFloorMemorySafeParams {
    // CHECK-LABEL: fn @direct(
    // CHECK: [[WORD:v[0-9]+]] = calldataload 4
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt [[WORD]], [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], [[WORD]]
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function direct(uint256 p) external pure returns (bool above) {
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    // CHECK-LABEL: fn @viaHelper(
    // CHECK: icall @setPointer
    function viaHelper(uint256 p) external pure returns (bool above) {
        setPointer(p);
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    // CHECK-LABEL: fn @offset(
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt {{v[0-9]+}}, [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], {{v[0-9]+}}
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function offset(uint256[] calldata a) external pure returns (bool above) {
        assembly ("memory-safe") {
            mstore(0x40, a.offset)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    // CHECK-LABEL: fn @length(
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt {{v[0-9]+}}, [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], {{v[0-9]+}}
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function length(uint256[] calldata a) external pure returns (bool above) {
        assembly ("memory-safe") {
            mstore(0x40, a.length)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    // CHECK-LABEL: fn @reslice(
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[BELOW:v[0-9]+]] = lt {{v[0-9]+}}, [[FLOOR]]
    // CHECK-NEXT: [[CLAMPED:v[0-9]+]] = select [[BELOW]], [[FLOOR]], {{v[0-9]+}}
    // CHECK-NEXT: mstore 64, [[CLAMPED]]
    function reslice(uint256[] calldata a) external pure returns (bool above) {
        uint256[] calldata rest = a[1:];
        assembly ("memory-safe") {
            mstore(0x40, rest.offset)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
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
}
