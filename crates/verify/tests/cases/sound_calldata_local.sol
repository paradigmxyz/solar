// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// A calldata *local* (not a parameter) passed to an internal function's memory
// parameter: Solidity copies it into fresh memory, so the callee's write never
// reaches `y`. The verifier writes the callee's array back into `y`.
contract CdLocal {
    function set(uint256[] memory a) internal pure {
        a[0] = 42;
    }

    /// @custom:requires x.length > 0 && x[0] != 42
    /// @custom:ensures result == 42
    function bad_local(uint256[] calldata x) external pure returns (uint256 result) {
        uint256[] calldata y = x;
        set(y);
        result = y[0];
    }

    /// @custom:requires x.length > 0 && x[0] != 42
    /// @custom:ensures result != 42
    function ok_local(uint256[] calldata x) external pure returns (uint256 result) {
        uint256[] calldata y = x;
        set(y);
        result = y[0];
    }
}
