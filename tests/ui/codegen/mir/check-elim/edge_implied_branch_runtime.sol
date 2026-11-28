//@ codegen-matrix: standard
//@ run-call: f 0, 338770000845734292533317652709722396295 => 0
//@ run-call: f 5, 338770000845734292533317652709722396295 => 7
//@ run-call: f 8, 338770000845734292533317652709722396295 => 135
//@ run-call: f 9, 338770000845734292533317652709722396295 => 208
//@ run-call: f 12, 338770000845734292533317652709722396295 => 720
//@ run-call: f 16, 338770000845734292533317652709722396295 => 45776
//@ run-call: f 17, 338770000845734292533317652709722396295 => 84781
//@ run-call: f 40, 338770000845734292533317652709722396295 => 710941428525

// Each arm decides the tests after it, so it jumps past them.
contract EdgeImpliedBranch {
    function f(uint256 n, uint256 w) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            if lt(n, 9) { r := and(w, sub(shl(n, 1), 1)) }
            if and(gt(n, 8), lt(n, 17)) { r := and(shr(3, w), sub(shl(n, 1), 1)) }
            if gt(n, 16) { r := and(shr(7, w), sub(shl(n, 1), 1)) }
        }
    }
}
