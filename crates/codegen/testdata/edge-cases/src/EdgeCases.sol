// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract EdgeCases {
    function addMax() external pure returns (uint256) {
        return type(uint256).max;
    }
    
    function addZero(uint256 a) external pure returns (uint256) {
        return a + 0;
    }
    
    function mulZero(uint256 a) external pure returns (uint256) {
        return a * 0;
    }
    
    function mulOne(uint256 a) external pure returns (uint256) {
        return a * 1;
    }
    
    function divOne(uint256 a) external pure returns (uint256) {
        return a / 1;
    }
    
    function subSame(uint256 a) external pure returns (uint256) {
        return a - a;
    }
    
    function modSame(uint256 a) external pure returns (uint256) {
        if (a == 0) return 0;
        return a % a;
    }
    
    function maxInt() external pure returns (int256) {
        return type(int256).max;
    }
    
    function minInt() external pure returns (int256) {
        return type(int256).min;
    }
    
    function maxUint8() external pure returns (uint8) {
        return type(uint8).max;
    }
    
    function identityBool(bool b) external pure returns (bool) {
        return b;
    }
    
    function negateBool(bool b) external pure returns (bool) {
        return !b;
    }
}
