// args: shapes/=tests/cases/auxiliary/imports/shapes/
pragma solidity ^0.8.20;

import "./auxiliary/imports/Base.sol";
import "./auxiliary/imports/Words.sol";
import {Bounded} from "shapes/Bounded.sol";

/// Code from a relative import (a base contract, a struct, a file-level constant that two files
/// define alike, and a free function) and from a remapped one (a library).
contract Imports is Base {
    /// @custom:ensures result == x + 1
    function ok_baseHelper(uint256 x) external pure returns (uint256 result) {
        require(x < LIMIT);
        result = inc(x);
    }

    /// @custom:ensures result == x
    function bad_baseHelper(uint256 x) external pure returns (uint256 result) {
        require(x < LIMIT);
        result = inc(x);
    }

    /// @custom:ensures result <= 1000
    function ok_library(uint256 x) external pure returns (uint256 result) {
        result = Bounded.clamp(x);
    }

    /// @custom:ensures result < 1000
    function bad_library(uint256 x) external pure returns (uint256 result) {
        result = Bounded.clamp(x);
    }

    /// @custom:ensures result == a + b
    function ok_struct(uint256 a, uint256 b) external pure returns (uint256 result) {
        require(a < LIMIT && b < LIMIT);
        Pair memory p = Pair(a, b);
        result = p.a + p.b;
    }

    /// @custom:ensures total == old(total) + x
    function ok_inheritedState(uint256 x) external {
        require(x < LIMIT && total < LIMIT);
        total += x;
    }

    /// @custom:ensures result <= x
    function ok_freeFunction(uint256 x) external pure returns (uint256 result) {
        result = halve(x);
    }

    /// @custom:ensures result < LIMIT
    function bad_freeFunction(uint256 x) external pure returns (uint256 result) {
        result = halve(x);
    }
}
