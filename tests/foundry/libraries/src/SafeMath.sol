// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library SafeMath {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    function sub(uint256 a, uint256 b) internal pure returns (uint256) {
        return a - b;
    }

    function mul(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * b;
    }

    function combine(uint256 a, uint256 b) public pure returns (uint256) {
        return a * 10 + b;
    }
}

contract TestLibrary {
    using SafeMath for uint256;

    uint256 public next;

    // Direct library call: SafeMath.add(a, b)
    function safeAddDirect(uint256 a, uint256 b) external pure returns (uint256) {
        return SafeMath.add(a, b);
    }

    function safeSubDirect(uint256 a, uint256 b) external pure returns (uint256) {
        return SafeMath.sub(a, b);
    }

    function safeMulDirect(uint256 a, uint256 b) external pure returns (uint256) {
        return SafeMath.mul(a, b);
    }

    // Chain multiple library calls
    function chainedOps(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        uint256 sum = SafeMath.add(a, b);
        return SafeMath.mul(sum, c);
    }

    function linkedExtensionEvaluationOrder() external returns (uint256) {
        return receiver().combine(argument());
    }

    function receiver() internal returns (uint256) {
        require(next == 0);
        next = 1;
        return 10;
    }

    function argument() internal returns (uint256) {
        require(next == 1);
        next = 2;
        return 2;
    }
}
