pragma solidity ^0.8.20;
/// `--instrument` writes each `@custom:check` as a require; the result verifies alike.
contract Checked {
    uint256 public total;

    /// @custom:check amount <= 1000
    /// @custom:ensures total == old(total) + amount
    function ok_checked_pre(uint256 amount) external { require(amount <= 1000, "solverus check failed: amount <= 1000");
        require(total <= 1000000);
        total += amount;
    }

    /// @custom:ensures result < 1000
    function ok_check_decl(uint256 x) external pure returns (uint256) {
        /// @custom:check x < 1000
        require(x < 1000, "solverus check failed: x < 1000"); uint256 y = x;
        return y;
    }

    function ok_check_branch(uint256 x) external pure returns (uint256) {
        if (x > 5) {
            /// @custom:check x > 3
            { require(x > 3, "solverus check failed: x > 3"); return x; }
        }
        return 0;
    }

    // expect: postcondition not satisfied
    /// @custom:ensures result < 1000
    function bad_check_too_weak(uint256 x) external pure returns (uint256) {
        /// @custom:check x <= 1000
        require(x <= 1000, "solverus check failed: x <= 1000"); uint256 y = x;
        return y;
    }
}
