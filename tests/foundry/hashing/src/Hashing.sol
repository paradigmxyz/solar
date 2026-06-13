// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Hashing {
    function hashUint(uint256 a) external pure returns (bytes32) {
        return keccak256(abi.encode(a));
    }
    
    function hashTwo(uint256 a, uint256 b) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }
    
    function hashPacked(uint256 a, uint256 b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b));
    }
    
    function compareHashes(uint256 a, uint256 b) external pure returns (bool) {
        bytes32 h1 = keccak256(abi.encode(a));
        bytes32 h2 = keccak256(abi.encode(b));
        return h1 == h2;
    }
    
    function hashBytes(bytes calldata data) external pure returns (bytes32) {
        return keccak256(data);
    }
}
