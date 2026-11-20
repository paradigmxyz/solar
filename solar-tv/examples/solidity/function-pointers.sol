// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract FunctionPointers {
    function plus(uint256 x) internal pure returns (uint256) {
        unchecked { return x + 1; }
    }

    function minus(uint256 x) internal pure returns (uint256) {
        unchecked { return x - 1; }
    }

    function choose(bool b, uint256 x) internal pure returns (uint256) {
        function(uint256) internal pure returns (uint256) pointer = b ? plus : minus;
        return pointer(x);
    }

    function run(bool b) internal pure returns (uint256) {
        return choose(b, 0);
    }

    function f(bool b) external pure returns (uint256) {
        return run(b);
    }
}
