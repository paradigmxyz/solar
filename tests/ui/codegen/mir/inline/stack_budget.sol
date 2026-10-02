//@ codegen-matrix: standard
//@ run-call: choose true, 4 => 15
//@ run-call: choose false, 4 => 23
//@ run-call: choose true, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 5
contract StackBudget {
    function choose(bool condition, uint256 x) external pure returns (uint256) {
        unchecked { return branch(condition, x) + x; }
    }
    function branch(bool condition, uint256 x) internal pure returns (uint256 y) {
        unchecked {
            if (condition) y = x + 7;
            else y = x ^ 23;
        }
    }
}
