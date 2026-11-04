//@ revisions: mir run
//@[mir] compile-flags: -Ogas -Zdump=mir-final
//@[mir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: f 1; constructor=[9] => 28
// A runtime read of an immutable loads a code placeholder, not memory, so the array that the
// function never reads from is still removed, also when the constructor calls the same helper.

contract RuntimeReadDeadArray {
    uint256 immutable x;
    uint256 public total;

    constructor(uint256 v) {
        x = v;
        if (get() == 7) revert();
    }

    function get() internal view returns (uint256 r) {
        r = x;
        for (uint256 i; i < total; i++) r += i * x;
    }

    // CHECK-LABEL: fn @f(
    // CHECK-NOT: heap_floor
    // CHECK-NOT: mstore 64
    // CHECK: returndata 0, 32
    function f(uint256 a) external view returns (uint256 r) {
        uint256[2] memory t;
        t[0] = a;
        r = get() + t[0] + get();
        t[0] = 5;
        r += get();
    }
}
