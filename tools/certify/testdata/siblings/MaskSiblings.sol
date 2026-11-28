// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// The certified contract is Masked. Beside it: an interface whose type it never uses, an abstract
// contract and a concrete contract with a loop that it never deploys, a library with an internal
// function it runs inline (through `using`) and a loop it never reaches, and a free function it
// runs inline.
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

library Bits {
    function low(uint256 a) internal pure returns (uint256) {
        return a & 0x0f;
    }

    function sum(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }
}

function high(uint256 a) pure returns (uint256) {
    return a & 0xf0;
}

contract Masked {
    using Bits for uint256;

    uint256 x;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures x == (v & 15)
    function put(uint256 v) external {
        x = v.low();
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures r == (a & 240)
    function hi(uint256 a) external pure returns (uint256 r) {
        r = high(a);
    }

    function get() external view returns (uint256) {
        return x;
    }
}
