//@ codegen-matrix: standard
//@ run-call: run 3 => 7
//@ run-call: run 20 => 8
//@ run-call: run 0 => 7
//@ run-call-fail: failing
contract ReturningGuard {
    uint256 failures;
    event Failed(uint256 actual, uint256 expected);

    function check(uint256 a, uint256 b) internal {
        if (a == b) return;
        ++failures;
        emit Failed(a, b);
    }

    function run(uint256 x) external returns (uint256) {
        for (uint256 i; i < 8; ++i) {
            check(i, x);
            check(i, i);
        }
        return failures;
    }

    function requireEqual(uint256 a, uint256 b) internal pure {
        if (a == b) return;
        revert();
    }

    function failing() external pure {
        for (uint256 i; i < 8; ++i) requireEqual(i, 0);
    }
}
