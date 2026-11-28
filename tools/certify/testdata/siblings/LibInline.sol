// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

library Deep {
    function loop(uint256 n) internal pure returns (uint256 s) {
        uint256 i = 0;
        while (i < n) {
            s += i;
            i++;
        }
    }

    function raw(uint256 a) internal pure returns (uint256 r) {
        assembly {
            r := add(a, 1)
        }
    }

    function rec(uint256 n) internal pure returns (uint256) {
        return n == 0 ? 0 : rec(n - 1) + 1;
    }

    function viaLoop(uint256 n) internal pure returns (uint256) {
        return loop(n);
    }

    function hash(address t) internal view returns (uint256) {
        return IBal(t).bal();
    }
}

interface IBal {
    function bal() external view returns (uint256);
}

// Each contract reaches one construct through inlined library code: a loop (two calls deep), inline
// assembly, recursion and an external call.
contract ReachesLoop {
    function f(uint256 n) external pure returns (uint256) {
        return Deep.viaLoop(n);
    }
}

contract ReachesAsm {
    function f(uint256 n) external pure returns (uint256) {
        return Deep.raw(n);
    }
}

contract ReachesRec {
    function f(uint256 n) external pure returns (uint256) {
        return Deep.rec(n);
    }
}

contract CtorLoop {
    uint256 x;

    constructor() {
        x = Deep.viaLoop(3);
    }

    function get() external view returns (uint256) {
        return x;
    }
}

contract ReachesCall {
    function f(address t) external view returns (uint256) {
        return Deep.hash(t);
    }
}
