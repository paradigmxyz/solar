// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    uint256 public value;
    
    function setValue(uint256 v) external {
        value = v;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
    
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}

contract LowLevelCalls {
    uint256 public value;
    
    function callTarget(address target, uint256 v) external returns (bool) {
        (bool success, ) = target.call(abi.encodeWithSignature("setValue(uint256)", v));
        return success;
    }
    
    function staticCallTarget(address target) external view returns (uint256) {
        (bool success, bytes memory data) = target.staticcall(abi.encodeWithSignature("getValue()"));
        require(success);
        return abi.decode(data, (uint256));
    }
    
    function delegateCallTarget(address target, uint256 v) external returns (bool) {
        (bool success, ) = target.delegatecall(abi.encodeWithSignature("setValue(uint256)", v));
        return success;
    }
}
