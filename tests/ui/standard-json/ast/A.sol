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

uint256 constant SLOT = 0x2000;

interface IToken {
    /// @dev A non-breaking space leads this comment.
    function balanceOf(address owner) external view returns (uint256);
}

interface IFallback {
    fallback() external;
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

contract Statements layout at SLOT + 1 {
    uint256 constant LEN = 2;
    uint256[LEN] arr;
    uint256 public value = 1 ether;
    Price price;

    enum Mode {
        Open,
        Closed
    }

    event Done(uint256 result);
    error Failed(string reason);

    modifier guard(uint256 min) {
        require(msg.value >= min, "low");
        _;
    }

    function pair() public pure returns (uint256, uint256) {
        return (1, 2);
    }

    function list() internal pure returns (uint256, address[] memory) {
        return (1, new address[](0));
    }

    function flow(uint256 x, bytes calldata data) external payable guard(1) returns (uint256 r) {
        uint256 a = x + 1;
        (uint256 b, uint256 c) = this.pair();
        {
            a++;
        }
        unchecked {
            a -= 1;
        }
        if (a > b) {
            r = a;
        } else {
            r = c;
        }
        for (uint256 i = 0; i < 3; ++i) {
            if (i == 1) continue;
            if (i == 2) break;
        }
        while (a > 0) {
            a--;
        }
        do {
            a += 1;
        } while (a < 3);
        bool ok = !(a == 0) && (b != 0 || c >= 1);
        r = ok ? a << 1 : ~a;
        delete arr;
        uint256[] memory m = new uint256[](3);
        m[0] = arr[0];
        bytes memory s = data[1:];
        (bool success,) = address(this).call{value: 1 wei}("");
        emit Done(r);
        try this.pair() returns (uint256 p, uint256) {
            r = p;
        } catch Error(string memory reason) {
            revert Failed(reason);
        } catch {
            revert();
        }
        address payable receiver = payable(msg.sender);
        r = type(uint256).max + uint256(uint160(address(receiver))) % 3;
        Point memory point = Point({x: 1, y: 2});
        uint256[2] memory literal = [uint256(1), 2];
        Price sum = price + price;
        (uint256 d, address[] memory addrs) = list();
        assembly ("memory-safe") {
            r := add(r, 1)
        }
        r += uint256(type(Mode).max) + d + addrs.length + m.length + s.length + literal[1] + point.x + (success ? 1 : 0) + uint256(Price.unwrap(sum));
        return r;
    }
}
