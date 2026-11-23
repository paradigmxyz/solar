pragma solidity ^0.8.20;
// An inlined callee's locals belong to its own scopes: a block of the caller that calls it keeps
// the caller's local of the same name after it ends.
contract CalleeLocalScope {
    function helper() private pure returns (uint256) {
        uint256 x = 1;
        return x;
    }

    /// @custom:ensures result == 7
    function ok_callee_local() external pure returns (uint256 result) {
        uint256 x = 7;
        {
            uint256 y = helper();
            y;
        }
        return x;
    }

    /// @custom:ensures result == 1
    function bad_callee_local() external pure returns (uint256 result) {
        uint256 x = 7;
        {
            uint256 y = helper();
            y;
        }
        return x;
    }
}
