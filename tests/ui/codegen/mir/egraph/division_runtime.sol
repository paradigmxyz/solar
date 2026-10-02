//@ codegen-matrix: standard
//@ run-call: scale 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f21 => 0xfffffffffffffffffffffffffffffffffffffffffffffffff7e52fe5afe40000
//@ run-call-fail: scale 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f22 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleLeft 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f21 => 0xfffffffffffffffffffffffffffffffffffffffffffffffff7e52fe5afe40000
//@ run-call-fail: scaleLeft 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f22 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleLeft 0 => 0
//@ run-call: fee 0x41bbb2f80a4553f6c19ad51e8e40314cc63a07b3fef911341fd6eab024f994 => 0x4189374bc6a7ef9db22d0e5604189374bc6a7ef9db22d0e5604189374bc6a7
//@ run-call-fail: fee 0x41bbb2f80a4553f6c19ad51e8e40314cc63a07b3fef911341fd6eab024f995 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: below 6999 => true
//@ run-call: below 7000 => false
//@ run-call: above 7999 => false
//@ run-call: above 8000 => true
//@ run-call: zero 999 => true
//@ run-call: zero 1000 => false
//@ run-call: nonzero 3599 => false
//@ run-call: nonzero 3600 => true
//@ run-call: bounds 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, false, false
//@ run-call: nested 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f21, 0
//@ run-call: split 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: split 86399 => 86399
//@ run-call: roundWeek 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8d900
//@ run-call: roundDown 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 7 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call-fail: roundDown 5, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012

// Boundaries of the division rules proved in Lean, through checked Solidity arithmetic.
contract DivisionRuntime {
    function scale(uint256 x) external pure returns (uint256) {
        return x * 1e18;
    }

    function scaleLeft(uint256 x) external pure returns (uint256) {
        return 1e18 * x;
    }

    function fee(uint256 x) external pure returns (uint256) {
        return x * 997 / 1000;
    }

    function below(uint256 x) external pure returns (bool) {
        return x / 1000 < 7;
    }

    function above(uint256 x) external pure returns (bool) {
        return x / 1000 > 7;
    }

    function zero(uint256 x) external pure returns (bool) {
        return x / 1000 == 0;
    }

    function nonzero(uint256 x) external pure returns (bool) {
        return x / 3600 != 0;
    }

    function bounds(uint256 x) external pure returns (bool, bool, bool) {
        return (x / 1e30 < 1.5e47, x / 1e30 > 1.5e47, x / 3 > type(uint256).max / 3);
    }

    function nested(uint256 x) external pure returns (uint256, uint256) {
        return (x / 1e9 / 1e9, x / 1e38 / 1.2e39);
    }

    function split(uint256 x) external pure returns (uint256) {
        return x / 1 days * 1 days + x % 1 days;
    }

    function roundWeek(uint256 x) external pure returns (uint256) {
        return x / 1 weeks * 1 weeks;
    }

    function roundDown(uint256 x, uint256 y) external pure returns (uint256) {
        return x / y * y;
    }
}
