// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Hashing {
    bytes private stored;

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

    function hashSha256(bytes calldata data) external pure returns (bytes32) {
        return sha256(data);
    }

    function hashRipemd160(bytes calldata data) external pure returns (bytes20) {
        return ripemd160(data);
    }

    function setStored(bytes memory data) external {
        stored = data;
    }

    function hashStoredPacked(bytes calldata suffix) external view returns (bytes32) {
        return keccak256(abi.encodePacked(stored, suffix));
    }

    function hashStoredConcat(bytes calldata suffix) external view returns (bytes32) {
        return keccak256(bytes.concat(stored, suffix));
    }

    function decodeStoredUint() external view returns (uint256) {
        return abi.decode(stored, (uint256));
    }

    function hashStoredSha256() external view returns (bytes32) {
        return sha256(stored);
    }

    function hashStoredRipemd160() external view returns (bytes20) {
        return ripemd160(stored);
    }

}
