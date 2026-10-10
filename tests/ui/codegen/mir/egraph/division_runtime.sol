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
//@ run-call: afterFee 10000 => 9970
//@ run-call: afterFee 0x888888888888888888888888888888888888888888888888888888888888888 => 0x881facfcdc177bd5f29ea6d7fee861363427de23c59050d3e654ec79c9a8e45
//@ run-call-fail: afterFee 0x888888888888888888888888888888888888888888888888888888888888889 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: cut 10, 3 => 7
//@ run-call: cut 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0
//@ run-call-fail: cut 5, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: remainders 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, false
//@ run-call: intoDay 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 36735
//@ run-call: intoDay 86400 => 0
//@ run-call: isMultiple 5000 => true
//@ run-call: isMultiple 5001 => false
//@ run-call: isMultiple 0 => true
//@ run-call: isMultiple 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false

//@ run-call: narrowBounds 249 => true, false, true, false
//@ run-call: narrowBounds 250 => false, true, true, false
//@ run-call: narrowBounds 255 => false, true, true, false
//@ run-call: narrowProduct 0 => true, true
//@ run-call: narrowProduct 25 => true, true
//@ run-call: narrowProduct 26 => false, false
//@ run-call: narrowProduct 255 => false, false

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

    function afterFee(uint256 x) external pure returns (uint256) {
        return x - x * 30 / 10_000;
    }

    function cut(uint256 x, uint256 y) external pure returns (uint256) {
        return x - x / y;
    }

    function remainders(uint256 x) external pure returns (bool, bool) {
        return (x % 1000 < 1000, x % 7 > 6);
    }

    function intoDay(uint256 t) external pure returns (uint256) {
        return t - t / 1 days * 1 days;
    }

    function isMultiple(uint256 x) external pure returns (bool) {
        return x / 1000 * 1000 == x;
    }
    function narrowBounds(uint8 x) external pure returns (bool, bool, bool, bool) {
        return (x / 10 < 25, x / 10 > 24, x / 10 < 26, x / 10 > 25);
    }

    function narrowProduct(uint8 x) external pure returns (bool, bool) {
        unchecked {
            return (x * 10 / 10 == x, x == 0 || x * 10 / x == 10);
        }
    }
}
