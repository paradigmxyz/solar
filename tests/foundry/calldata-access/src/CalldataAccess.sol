// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CalldataAccess {
    struct Dynamic {
        uint256 a;
        uint256[] b;
    }

    function nested(uint256[][] calldata p) external pure returns (uint256[][] calldata) {
        return p;
    }

    function encodeNested(uint256[][] calldata p) external pure returns (bytes memory) {
        return abi.encode(p);
    }

    function byteArrays(bytes[] calldata p) external pure returns (bytes[] calldata) {
        return p;
    }

    function encodeByteArrays(bytes[] calldata p) external pure returns (bytes memory) {
        return abi.encode(p);
    }

    function row(uint256[][] calldata p, uint256 i) external pure returns (uint256[] calldata) {
        return p[i];
    }

    function item(bytes[] calldata p, uint256 i) external pure returns (bytes calldata) {
        return p[i];
    }

    function structs(Dynamic[] calldata p) external pure returns (Dynamic[] calldata) {
        return p;
    }

    function member(Dynamic calldata p) external pure returns (Dynamic calldata) {
        return p;
    }
}
