// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RuntimeCodeChild {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract AbiEncoding {
    function encodeUint(uint256 a) external pure returns (bytes memory) {
        return abi.encode(a);
    }
    
    function encodePacked(uint256 a, uint256 b) external pure returns (bytes memory) {
        return abi.encodePacked(a, b);
    }

    function encodePackedArray(bytes32[] memory values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
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

    function codeLength(address account) external view returns (uint256) {
        return account.code.length;
    }

    function codeHash(address account) external view returns (bytes32) {
        return account.codehash;
    }

    function code(address account) external view returns (bytes memory) {
        return account.code;
    }

    function addressFromBytes20(bytes20 value) external pure returns (address) {
        return address(value);
    }

    function runtimeCodeLength() external pure returns (uint256) {
        return type(RuntimeCodeChild).runtimeCode.length;
    }
}
