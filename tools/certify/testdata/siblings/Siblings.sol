// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// The certified contract is Target. Its siblings are an interface it takes an enum from, an
// abstract contract and a concrete contract it never deploys, and a library whose internal function
// it calls (inlined, both by name and through `using`) and whose looping function it never reaches.
interface IKinds {
    enum Kind { A, B, C }
}

abstract contract Base {
    function hook() internal virtual returns (uint256);
}

contract Other {
    uint256 y;

    function set(uint256 v) external {
        for (uint256 i = 0; i < v; i++) {
            y += i;
        }
    }
}

library Math {
    function twice(uint256 a) internal pure returns (uint256) {
        return a * 2;
    }

    function sum(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }
}

function inc(uint256 a) pure returns (uint256) {
    return a + 1;
}

contract Target {
    using Math for uint256;

    uint256 x;

    function put(uint256 v) external {
        x = Math.twice(v) + v.twice() + inc(v);
    }

    function kind(IKinds.Kind k) external pure returns (uint256) {
        return uint256(k);
    }
}
