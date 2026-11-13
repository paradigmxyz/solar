pragma solidity ^0.8.20;

import {SHARED} from "./auxiliary/shared/K2.sol";
import {UsesK1} from "./auxiliary/shared/UsesK1.sol";

/// A constant two files declare with the same text resolves to the same value from either.
/// `Outer` holds a struct declared after it, so its zero value names a later struct's zero.
struct Outer {
    Inner inner;
    uint256 n;
}

struct Inner {
    uint256 x;
}

contract SharedNames is UsesK1 {
    mapping(uint256 => Outer) internal outers;

    /// @custom:ensures result == 7
    function ok_shared() external pure returns (uint256 result) {
        result = k1() + SHARED - 7;
    }

    /// @custom:ensures result == 8
    function bad_shared() external pure returns (uint256 result) {
        result = k1();
    }

    /// @custom:ensures result == outers[k].inner.x
    function ok_nested(uint256 k) external view returns (uint256 result) {
        result = outers[k].inner.x;
    }

    /// @custom:ensures result == outers[k].n
    function bad_nested(uint256 k) external view returns (uint256 result) {
        result = outers[k].inner.x;
    }
}
