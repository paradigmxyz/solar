//@ codegen-matrix: standard checked
//@[checked] compile-flags: -Ogas -Zcheck-analyses=true -Zdump=mir-final
//@[checked] filecheck: --check-prefix=CHECKED
//@ run-call: masked 5 => 5
//@ run-call: masked 100 => 0
//@ run-call: bounded_sum 3, 4 => 12
//@ run-call: bounded_sum 255, 100 => 8160
//@ run-call: bounded_sum 0, 0 => 0

// `-Zcheck-analyses=true` tests each fact a pass relied on, here the loop's
// overflow tests check elimination drops, with a branch to one shared trap,
// which reverts with `AnalysisFactViolated(uint256,uint256,uint256)`. Every
// fact holds, so each call behaves as it does unchecked.

// CHECKED-LABEL: fn @entry()
// CHECKED: mstore 0, 0xed572b27
// CHECKED-NEXT: revert 28, 100
contract Ranges {
    function masked(uint256 x) external pure returns (uint256) {
        if (x < 10) {
            return x & 0xff;
        }
        return 0;
    }

    function bounded_sum(uint8 a, uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 0; i < n && i < 32; i++) {
            s += a;
        }
    }
}
