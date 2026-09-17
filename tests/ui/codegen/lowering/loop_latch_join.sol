//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: search [], 1 => false, 0
//@ run-call: search [1, 3, 5, 7], 0 => false, 0
//@ run-call: search [1, 3, 5, 7], 1 => true, 0
//@ run-call: search [1, 3, 5, 7], 4 => false, 1
//@ run-call: search [1, 3, 5, 7], 5 => true, 2
//@ run-call: search [1, 3, 5, 7], 7 => true, 3
//@ run-call: search [1, 3, 5, 7], 9 => false, 3
//@ run-call: contains [1, 3, 5, 7], 5 => true
//@ run-call: contains [1, 3, 5, 7], 4 => false

contract LoopLatchJoin {
    function search(uint256[] memory a, uint256 needle) external pure returns (bool, uint256) {
        return _search(a, needle);
    }

    function contains(uint256[] memory a, uint256 needle) external pure returns (bool found) {
        (found,) = _search(a, needle);
    }

    // Two joins in this loop choose their stack order from more than the first edge.
    //
    // The join after `if (index != 0)` merges the load with the branch that skips it. The
    // branch reaches it on both of its arms, so the order it already holds is the cheap one,
    // and the middle index is tested where the shift left it.
    //
    // The latch merges the arms that move `h` and `l` and holds nothing but their phis. Given
    // the header's own order it is empty, so both arms jump to the header and no block of
    // swaps runs between them on every iteration.
    // CHECK-LABEL: @module LoopLatchJoin_runtime
    // CHECK: [loop]:
    // CHECK: push [[RAISE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: jump [[HEADER:bb[0-9]+]]
    // CHECK-NEXT: [[HEADER]] [loop]:
    // CHECK: shr
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: iszero
    // CHECK: [[RAISE]] [loop]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: pop
    // CHECK-NEXT: jump [[HEADER]]
    function _search(uint256[] memory a, uint256 needle)
        private
        pure
        returns (bool found, uint256 index)
    {
        uint256 l = 1;
        uint256 h = a.length;
        uint256 t;
        while (true) {
            index = (l + h) / 2;
            if (index != 0) t = a[index - 1];
            if (l > h || (index != 0 && t == needle)) break;
            if (needle <= t) {
                h = index - 1;
            } else {
                l = index + 1;
            }
        }
        found = index != 0 && t == needle;
        if (index != 0) index -= 1;
    }
}
