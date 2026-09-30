//@ compile-flags: -Zdataflow=storage
//@ filecheck:
// Mutually recursive functions reach a joint fixed point: each summary includes the writes of
// the whole cycle, instantiated for its own storage-pointer parameter. A slot computed in a
// loop from the previous iteration's slot widens to the unknown path.

// CHECK: fn @even:
// CHECK: summary: writes={arg0, arg0.1}
// CHECK: fn @odd:
// CHECK: summary: writes={arg0, arg0.1}
// CHECK: fn @start:
// CHECK: summary: writes={slot(2), slot(3)}
// CHECK: fn @walk:
// CHECK: sstore {{.*}}  ; write=?
contract Recursion {
    struct Pair { uint256 left; uint256 right; }
    uint256 depth;
    uint256 limit;
    Pair pair;

    function even(Pair storage p, uint256 n) internal {
        p.left = n;
        if (n > 0) odd(p, n - 1);
    }

    function odd(Pair storage p, uint256 n) internal {
        p.right = n;
        if (n > 0) even(p, n - 1);
    }

    function start(uint256 n) external {
        even(pair, n);
    }

    function walk(uint256 n) external {
        assembly {
            let slot := 10
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                sstore(slot, i)
                slot := add(slot, 1)
            }
        }
    }
}
