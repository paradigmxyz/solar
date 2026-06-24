// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AbiVectorFixture {
    function f(
        uint256 value,
        bool flag,
        bytes memory data,
        string memory text
    ) external pure returns (uint256, bool, bytes32, bytes32, uint256, uint256) {
        return (
            value,
            flag,
            keccak256(data),
            keccak256(bytes(text)),
            data.length,
            bytes(text).length
        );
    }
}
