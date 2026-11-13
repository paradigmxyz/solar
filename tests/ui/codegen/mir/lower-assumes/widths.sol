//@ codegen-matrix: standard checked
//@[checked] compile-flags: -Ogas -Zcheck-analyses=true -Zdump=mir-final
//@[checked] filecheck: --check-prefix=CHECKED
//@ run-call: divided 255 => 0
//@ run-call: masked 200 => 200
//@ run-call: power 8 => 256
//@ run-call: power 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: bounded_power 4 => 81
//@ run-call-fail: bounded_power 100
//@ run-call: product 5 => 120
//@ run-call: product 0 => 1
//@ run-call: both true, true => true
//@ run-call: both true, false => false

// The widths the e-graph reads from definitions and the word bounds integer
// cleanup fuses tests on are recorded with `-Zcheck-analyses` and tested at run
// time; every fact holds, so each call behaves as it does unchecked. The fact
// words name the predicate and the pass: `0x308` is `ult` (3) from the e-graph
// (8), `0x408` `ule` from the e-graph, and `0x402` `ule` from check elimination.
contract Widths {
    // a / 300 => 0, since a < 300.
    // CHECKED-LABEL: fn @divided(
    // CHECKED: phi [{{.*}}: {{0x308[0-9a-f]+}}]
    function divided(uint8 a) external pure returns (uint256) {
        return uint256(a) / 300;
    }

    // a & 0xffff => a, since a <= 0xffff.
    // CHECKED-LABEL: fn @masked(
    // CHECKED: phi [{{.*}}: {{0x408[0-9a-f]+}}]
    function masked(uint8 a) external pure returns (uint256) {
        return uint256(a) & 0xffff;
    }

    // A constant base checks its exponent against its largest power that fits, 255
    // for 2: the e-graph folds the test of a `uint8` exponent, relying on e <= 255.
    // CHECKED-LABEL: fn @power(
    // CHECKED: phi [{{.*}}: {{0x408[0-9a-f]+}}]
    function power(uint8 e) external pure returns (uint256) {
        return 2 ** e;
    }

    // Here check elimination folds the exponent test under e < 100 and records the
    // branch it folded.
    // CHECKED-LABEL: fn @bounded_power(
    // CHECKED: phi [{{.*}}: {{0x102[0-9a-f]+}}]
    function bounded_power(uint256 e) external pure returns (uint256) {
        require(e < 100);
        return 3 ** e;
    }

    // CHECKED-LABEL: fn @product(
    // CHECKED: mstore 0, 0xed572b27
    function product(uint256 n) external pure returns (uint256 p) {
        p = 1;
        for (uint256 i = 2; i <= n; i++) {
            p *= i;
        }
    }

    // Two checked booleans fuse into one test of the `and` of their words.
    // CHECKED-LABEL: fn @both(
    // CHECKED: phi [{{.*}}: {{0x402[0-9a-f]+}}]
    function both(bool a, bool b) external pure returns (bool) {
        return a && b;
    }
}
