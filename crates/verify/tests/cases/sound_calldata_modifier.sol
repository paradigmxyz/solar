// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// A calldata parameter passed to an internal memory parameter inside the body
// of a function that has a modifier: the body runs under `_` with no name
// counted as calldata, so the callee's write is written back into `x`.
contract CdMod {
    function set(uint256[] memory a) internal pure {
        a[0] = 42;
    }

    modifier nop() {
        _;
    }

    /// @custom:requires x.length > 0 && x[0] != 42
    /// @custom:ensures result == 42
    function bad_mod(uint256[] calldata x) external pure nop returns (uint256 result) {
        set(x);
        result = x[0];
    }

    /// same body without the modifier: correctly fails
    /// @custom:requires x.length > 0 && x[0] != 42
    /// @custom:ensures result == 42
    function bad_nomod(uint256[] calldata x) external pure returns (uint256 result) {
        set(x);
        result = x[0];
    }

    /// @custom:requires x.length > 0 && x[0] != 42
    /// @custom:ensures result != 42
    function ok_mod(uint256[] calldata x) external pure nop returns (uint256 result) {
        set(x);
        result = x[0];
    }
}
