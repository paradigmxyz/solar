//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: pick [10, 20, 30], 7 => 20
//@ run-call: pick [10, 20, 30], 3 => 10
//@ run-call-fail: pick [], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: pickDown [1, 2, 3, 4], 5 => 4
//@ run-call: pickDown [7], 5 => 0

// A remainder is below its divisor once the divisor is above zero, so an index drawn as
// `r % n` from a length `n` needs no bounds check: a checked remainder has already failed on a
// zero divisor, and a loop running while `n > 1` keeps its remainders below `n`.
contract RemainderBounds {
    // CHECK-LABEL: fn @pick(
    // CHECK-NOT: mstore 32, 50
    // CHECK: returndata
    function pick(uint256[] calldata a, uint256 r) external pure returns (uint256) {
        return a[r % a.length];
    }

    // CHECK-LABEL: fn @pickDown(
    // CHECK-NOT: mstore 32, 50
    // CHECK: returndata
    function pickDown(uint256[] memory a, uint256 r) external pure returns (uint256 s) {
        uint256 n = a.length;
        while (n > 1) {
            s += a[(r >> n) % n];
            --n;
        }
    }
}
