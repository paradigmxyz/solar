// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AbiEncoding {
    function encodeUint(uint256 a) external pure returns (bytes memory) {
        return abi.encode(a);
    }
    
    function encodePacked(uint256 a, uint256 b) external pure returns (bytes memory) {
        return abi.encodePacked(a, b);
    }
    
    function encodeMultiple(uint256 a, uint256 b, uint256 c) external pure returns (bytes memory) {
        return abi.encode(a, b, c);
    }
    
    function decodeUint(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data, (uint256));
    }
    
    function decodeMultiple(bytes calldata data) external pure returns (uint256, uint256) {
        return abi.decode(data, (uint256, uint256));
    }
    
    function roundtrip(uint256 a) external pure returns (uint256) {
        bytes memory encoded = abi.encode(a);
        return abi.decode(encoded, (uint256));
    }
}
