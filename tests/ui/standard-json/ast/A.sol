// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
pragma abicoder v2;

import {Math as M, Num} from "./B.sol";
import "./B.sol" as B;
import "./C.sol";

/// A fixed-point price.
type Price is uint128;

struct Point {
    uint256 x;
    uint256 y;
}

enum Side {
    Buy,
    Sell
}

error Unauthorized(address caller);

function add(Price a, Price b) pure returns (Price) {
    return Price.wrap(Price.unwrap(a) + Price.unwrap(b));
}

using {add as +} for Price global;
using M for uint256;

interface IToken {
    function balanceOf(address owner) external view returns (uint256);
}

abstract contract Base {
    modifier onlyOwner() virtual {
        _;
    }

    /// Dropped: a blank line ends the group.

    /// @dev Kept.
    function hook() internal virtual;
}

/// @title A vault
contract Vault is Base, IToken {
    struct Position {
        uint256[] amounts;
        mapping(address => uint256) owed;
        function(uint256) external callback;
        Side side;
    }

    enum Mode {
        Open,
        Closed
    }

    event Deposited(address indexed from, uint256 amount, Price price);

    uint256 public constant FEE = 3;
    address public immutable owner;
    mapping(address => Position) positions;
    Point[2] points;
    string name;

    constructor(address owner_) {
        owner = owner_;
    }

    receive() external payable {}

    fallback() external {}

    /// @notice Returns the amount of positions.
    function balanceOf(address who) external view override returns (uint256) {
        return positions[who].amounts.length;
    }

    function hook() internal override {}

    function position(address who) internal view returns (Position storage) {
        return positions[who];
    }

    function deposit(uint256[] calldata amounts, Price price)
        external
        payable
        onlyOwner
        returns (uint256 total)
    {
        total = amounts.length.twice();
        emit Deposited(msg.sender, total, price);
    }
}
