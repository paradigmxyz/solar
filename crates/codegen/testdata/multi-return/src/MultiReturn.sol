// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IMultiReturn {
    function getTwo() external pure returns (uint256, uint256);
    function getThree() external pure returns (uint256, uint256, uint256);
}

contract MultiReturn {
    function getTwo() external pure returns (uint256, uint256) {
        return (1, 2);
    }
    
    function getThree() external pure returns (uint256, uint256, uint256) {
        return (10, 20, 30);
    }
    
    function testTwo() external view returns (uint256, uint256) {
        (uint256 a, uint256 b) = this.getTwo();
        return (a, b);
    }
    
    function testThree() external view returns (uint256, uint256, uint256) {
        (uint256 a, uint256 b, uint256 c) = this.getThree();
        return (a, b, c);
    }
    
    function testPartialCapture() external view returns (uint256) {
        (, uint256 b) = this.getTwo();
        return b;
    }
    
    // Simple test: return fixed values as tuple
    function simpleReturn() external pure returns (uint256, uint256) {
        return (111, 222);
    }
    
    // Test that calls simpleReturn and returns the result
    function testSimpleReturn() external view returns (uint256, uint256) {
        (uint256 a, uint256 b) = this.simpleReturn();
        return (a, b);
    }
    
    function callVia(address callee) external view returns (uint256, uint256) {
        (uint256 a, uint256 b) = IMultiReturn(callee).getTwo();
        return (a, b);
    }
}
