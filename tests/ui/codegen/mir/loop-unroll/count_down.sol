//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: countDown 0 => 0
//@ run-call: countDown 1 => 1
//@ run-call: countDown 2 => 3
//@ run-call: countDown 7 => 28
//@ run-call: countDown 100 => 5050
//@ run-call: squaresDown 0 => 0
//@ run-call: squaresDown 1 => 1
//@ run-call: squaresDown 2 => 5
//@ run-call: squaresDown 3 => 14
//@ run-call: squaresDown 10 => 385
//@ run-call-fail: squaresDown 0x100000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: squaresDown 0x100000000000000000000000000000001 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: stepDown 0 => 0
//@ run-call: stepDown 2 => 2
//@ run-call: stepDown 4 => 6
//@ run-call: stepDown 6 => 12
//@ run-call: stepDown 100 => 2550
//@ run-call-fail: stepDown 3; gas=200000
//@ run-call: downTo 5, 5 => 0
//@ run-call: downTo 9, 4 => 35
//@ run-call: downTo 10, 4 => 45
//@ run-call: downTo 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1

// Loops that count down until their counter reaches a bound, zero by default,
// peel one iteration when an odd number remain, then test the bound once per
// two iterations. A counter that never reaches its bound runs out of gas in
// every build.
contract CountDown {
    // Counting down by one to zero, the parity is the counter's low bit.

    function countDown(uint256 n) external pure returns (uint256 s) {
        assembly {
            for { let i := n } i { i := sub(i, 1) } { s := add(s, i) }
        }
    }

    // CHECK-COUNT-2: mul
    function squaresDown(uint256 n) external pure returns (uint256 s) {
        for (uint256 i = n; i > 0; i--) s += i * i;
    }

    // Stepping down by two, the parity is bit 1 of the counter.

    function stepDown(uint256 n) external pure returns (uint256 s) {
        assembly {
            for { let i := n } gt(i, 0) { i := sub(i, 2) } { s := add(s, i) }
        }
    }

    function downTo(uint256 start, uint256 end) external pure returns (uint256 s) {
        assembly {
            for { let i := start } iszero(eq(i, end)) { i := sub(i, 1) } { s := add(s, i) }
        }
    }
}
