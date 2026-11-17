// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract MappingKeys {
    mapping(string => address) byString;
    mapping(bytes => uint256) byBytes;
    mapping(string => mapping(bytes => uint256)) nested;
    mapping(bytes32 => uint256) byValue;
}
