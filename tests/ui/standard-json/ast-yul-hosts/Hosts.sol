// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {OTHER} from "./auxiliary/Constants.sol";

uint256 constant LIMIT = 4;

// Blocks in every kind of function, each with Yul functions of the same name.
function free(uint256 x) pure returns (uint256 r) {
    assembly {
        function helper(a) -> b { b := add(a, LIMIT) }
        r := helper(x)
    }
    assembly {
        function helper(a) -> b { b := add(a, OTHER) }
        r := add(r, helper(x))
    }
}

contract Hosts {
    uint256 internal word;

    modifier guard() {
        assembly {
            function check() { if iszero(sload(word.slot)) { revert(0, 0) } }
            check()
        }
        _;
    }

    constructor() {
        assembly {
            function init() { sstore(word.slot, OTHER) }
            init()
        }
    }

    receive() external payable {
        assembly {
            function ping() -> p { p := LIMIT }
            pop(ping())
        }
    }

    function hosted(uint256 x) external guard returns (uint256 r) {
        unchecked {
            assembly {
                function one() -> o { o := LIMIT }
                r := add(x, one())
            }
        }
        assembly {
            function one() -> o { o := OTHER }
            function two() -> t { t := add(one(), word.slot) }
            r := add(r, two())
        }
    }
}
