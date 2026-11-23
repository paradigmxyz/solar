// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract FallthroughEntry {
    function a() external pure {
        halt();
    }

    function b() external pure {
        halt();
    }

    function halt() internal pure {
        assembly {
            stop()
        }
    }
}
