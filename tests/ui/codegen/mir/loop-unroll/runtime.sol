//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: sum 0 => 0
//@ run-call: sum 1 => 1
//@ run-call: sum 2 => 5
//@ run-call: sum 3 => 12
//@ run-call: sum 4 => 22
//@ run-call: sum 5 => 35
//@ run-call: sum 1000 => 1499500
//@ run-call: stepped 5 => 0
//@ run-call: stepped 6 => 5
//@ run-call: stepped 8 => 5
//@ run-call: stepped 9 => 43
//@ run-call: stepped 12 => 312
//@ run-call: stepped 15 => 2198
//@ run-call: stepped 100 => 1012392034723593925779857584
//@ run-call: guarded 2, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaac
//@ run-call: squares 0 => 0
//@ run-call: squares 1 => 1
//@ run-call: squares 2 => 5
//@ run-call: squares 3 => 14
//@ run-call: squares 4 => 30
//@ run-call: squares 5 => 55
//@ run-call: squares 100 => 338350
//@ run-call: range 5, 4 => 0
//@ run-call: range 4, 4 => 4
//@ run-call: range 0, 5 => 15
//@ run-call: range 3, 10 => 52
//@ run-call: range 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7
//@ run-call-fail: range 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: range 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: guarded 3, 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: guarded 4, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: guarded 3, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Gas builds run these loops' bodies twice per test of their bound. The
// overflows that `guarded` reports happen in the second copy, the first copy,
// and the original loop.

// The dispatcher orders the bodies `stepped`, `range`, `guarded`, `squares`
// and `sum`. `stepped` starts at five and steps by three, four copies at a
// time.

// `range` runs while `i <= end` from any start, so its main loop tests
// `i < end` without an addition, two copies at a time.

// `squares` counts from one while `i <= n`, and at these runs four copies that
// add to the counter first pay more than two testing `i < n`.

contract LoopUnroll {
    function sum(uint256 n) external pure returns (uint256 s) {
        for (uint256 i; i < n; ++i) s += i * 3 + 1;
    }

    function stepped(uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 5; i < n; i += 3) {
            unchecked { s = s * 7 + i; }
        }
    }

    function guarded(uint256 n, uint256 x) external pure returns (uint256 s) {
        for (uint256 i; i < n; ++i) s += x;
    }

    function squares(uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 1; i <= n; ++i) s += i * i;
    }

    // The increment's overflow check panics once `i` passes the largest word.
    function range(uint256 start, uint256 end) external pure returns (uint256 total) {
        for (uint256 i = start; i <= end; ++i) {
            unchecked { total += i; }
        }
    }
}
