// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {B as Base} from "./B.sol";
import "./B.sol" as M;

using {add as +} for Money global;

type Money is uint256;

enum Kind {
    One,
    Two
}

struct S {
    uint a;
    mapping(uint => address) owners;
    function(uint) external returns (bool) callback;
}

error Failed(uint code);

function add(Money a, Money b) pure returns (Money) {
    return Money.wrap(Money.unwrap(a) + Money.unwrap(b));
}

/// @title The contract
contract C is Base(1) {
    using M.B for uint;

    event Done(uint indexed amount, string note);

    uint immutable x;
    uint constant K = 2;
    S s;

    constructor() {
        x = f(1);
    }

    modifier positive(uint v) {
        require(v > 0, "zero");
        _;
    }

    function g(uint[] calldata xs) public view positive(1) returns (uint r) {
        uint[2] memory pair = [uint(1), 2];
        (uint first, ) = (x, 3);
        for (uint i = 0; i < xs.length; i++) {
            if (xs[i] > first) {
                r += xs[i];
            } else {
                continue;
            }
        }
        while (r > 100) {
            r--;
        }
        do {
            r++;
        } while (r < 5);
        try this.g(xs) returns (uint v) {
            r = v;
        } catch Error(string memory) {
            revert Failed(1);
        } catch {}
        unchecked {
            r = r + pair[0];
        }
        assembly {
            let t := add(r, 1)
            mstore(0, t)
        }
        emit Done(r, "done");
        r = r > 1 ? payable(msg.sender).balance : type(uint).max;
        delete r;
    }
}
