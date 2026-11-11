pragma solidity ^0.8.20;

uint256 constant LIMIT = 100;

struct Pair {
    uint256 a;
    uint256 b;
}

/// Imported: not verified itself, but the functions a contract inherits from it are verified as
/// that contract's.
contract Base {
    uint256 internal total;

    /// @custom:requires x < LIMIT
    /// @custom:ensures result == x + 1
    function inc(uint256 x) internal pure returns (uint256 result) {
        result = x + 1;
    }

    /// Wrong on purpose: it fails as `Imports.bad_inherited`, not as `Base.bad_inherited`.
    /// @custom:ensures result == 0
    function bad_inherited(uint256 x) external pure returns (uint256 result) {
        result = x;
    }
}
