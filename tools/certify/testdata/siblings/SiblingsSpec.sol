// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// The certified contract is Doubler, with a specification. Beside it: an interface whose enum it
// takes, an abstract contract and a concrete contract it never deploys, and a library whose
// internal function it runs inline, by name and through `using`.
interface IKinds {
    enum Kind { A, B, C }
}

abstract contract Base {
    function hook() internal virtual returns (uint256);
}

contract Other {
    uint256 y;

    function set(uint256 v) external {
        y = v;
    }
}

library Math {
    function twice(uint256 a) internal pure returns (uint256) {
        return a * 2;
    }
}

contract Doubler {
    using Math for uint256;

    /// @custom:succeeds-if msg.value == 0 && a < 1000
    /// @custom:ensures r == 4 * a
    function quad(uint256 a) external pure returns (uint256 r) {
        r = Math.twice(a).twice();
    }

    /// @custom:ensures r <= 2
    function kind(IKinds.Kind k) external pure returns (uint256 r) {
        r = uint256(k);
    }
}
